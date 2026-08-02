//! The agent loop.
//!
//! Request the model, execute whatever tools it asks for, feed the results back,
//! repeat until it stops asking. Everything interesting about a harness lives
//! either side of this file — the provider layer above it, the tool layer and the
//! permission model below — but this is the spine.
//!
//! The loop is provider-agnostic. It describes the turn it wants and lets the
//! provider decide how to express that on the wire.
//!
//! Provider-facing history is **transactional**: mutations are staged and only
//! committed when the turn reaches a complete terminal state. Errors and
//! cancellation leave `Agent::messages` unchanged so wire history never contains
//! a half-built assistant/tool turn. UI transcript is the front-end's job.

use std::sync::{Arc, Mutex};

use crate::anthropic::types::{tool_result, tool_uses, Message};
use crate::cancel::CancelFlag;
use crate::error::{HarnessError, Result};
use crate::provider::{Provider, StreamEvent, TurnRequest};
use crate::thread::new_id;
use crate::tools::approval::{
    ApprovalDecision, ApprovalRequest, Approver, DenyApprover,
};
use crate::tools::ToolRegistry;
use crate::usage::Ledger;

pub struct Agent {
    provider: Arc<dyn Provider>,
    tools: ToolRegistry,
    /// Shared so delegated sub-agents on other providers bill into the same book.
    ledger: Option<Arc<Mutex<Ledger>>>,
    /// Gate for write/exec tools. Defaults to deny-all when unset.
    approver: Arc<dyn Approver>,
    pub model: String,
    /// Budgets reasoning *and* text together on providers that think. Streaming
    /// means there is no HTTP timeout pressure, so this is a ceiling rather than
    /// a target.
    pub max_tokens: u32,
    /// A request, not a command — providers that have no notion of effort ignore it.
    pub effort: String,
    pub system: Option<String>,
    pub messages: Vec<Message>,
}

impl Agent {
    pub fn new(provider: Arc<dyn Provider>, tools: ToolRegistry) -> Self {
        let model = provider.default_model().to_string();
        Self {
            provider,
            tools,
            ledger: None,
            approver: Arc::new(DenyApprover),
            model,
            max_tokens: 32_000,
            effort: "high".to_string(),
            system: None,
            messages: Vec::new(),
        }
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    pub fn with_ledger(mut self, ledger: Arc<Mutex<Ledger>>) -> Self {
        self.ledger = Some(ledger);
        self
    }

    /// Restore prior turns so the model sees conversation history after reopen.
    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }

    /// Hook for desktop (or a CLI prompt) to allow/deny gated tools.
    pub fn with_approver(mut self, approver: Arc<dyn Approver>) -> Self {
        self.approver = approver;
        self
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    /// Which provider this agent spends against. Keyed on by the usage ledger.
    pub fn provider_id(&self) -> &str {
        self.provider.id()
    }

    /// Registered tool names (stable order). Used to assert `delegate` wiring.
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.names()
    }

    /// Validate a model/effort pair against this agent's provider catalogue.
    pub fn validate_options(&self, model: &str, effort: &str) -> std::result::Result<(), String> {
        self.provider.validate_selection(model, effort)
    }

    /// Provider catalogue for pickers.
    pub fn descriptor(&self) -> crate::provider::ProviderDescriptor {
        self.provider.descriptor()
    }

    /// Send one user message and run to completion, executing tools as asked.
    ///
    /// Wire history is committed only after a complete terminal turn. Pass
    /// `cancel` to cooperatively abort between provider/tool steps.
    pub async fn send(
        &mut self,
        user_input: &str,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
    ) -> Result<()> {
        self.send_cancellable(user_input, on_event, None).await
    }

    pub async fn send_cancellable(
        &mut self,
        user_input: &str,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
        cancel: Option<&CancelFlag>,
    ) -> Result<()> {
        let mut staged = self.messages.clone();
        staged.push(Message::user_text(user_input));

        loop {
            Self::check_cancel(cancel)?;

            let request = TurnRequest {
                model: self.model.clone(),
                system: self.system.clone(),
                messages: staged.clone(),
                tools: self.tools.definitions(),
                max_tokens: self.max_tokens,
                effort: Some(self.effort.clone()),
                thinking: true,
            };

            let completion = match self.provider.stream_turn(&request, &mut *on_event).await {
                Ok(c) => c,
                Err(e) => {
                    // Do not commit staged history — keep prior wire messages intact.
                    return Err(e);
                }
            };

            Self::check_cancel(cancel)?;

            // Bill it before anything else can fail. A poisoned lock is skipped
            // rather than propagated — accounting must not abort a paid-for turn.
            if let Some(ledger) = &self.ledger {
                if let Ok(mut ledger) = ledger.lock() {
                    ledger.record(self.provider.id(), &completion);
                }
            }

            // Echo the assistant turn back verbatim — thinking signatures and
            // tool_use blocks both have to survive intact.
            staged.push(Message::assistant(completion.content.clone()));

            match completion.stop_reason.as_deref() {
                Some("end_turn") | None => {
                    self.messages = staged;
                    return Ok(());
                }

                Some("tool_use") => {
                    let calls = tool_uses(&completion.content);
                    if calls.is_empty() {
                        return Err(HarnessError::Other(
                            "stop_reason was tool_use but no tool_use block was present".into(),
                        ));
                    }

                    let mut results = Vec::with_capacity(calls.len());
                    for call in calls {
                        Self::check_cancel(cancel)?;
                        let (body, is_error) =
                            self.execute_tool_call(&call, on_event, cancel).await;
                        if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
                            // Tool may have returned a deny body after cancel woke
                            // the approver; still do not commit staged history.
                            return Err(HarnessError::Cancelled);
                        }
                        let summary = summarize_tool_body(&body);
                        on_event(StreamEvent::ToolCallResult {
                            name: &call.name,
                            id: &call.id,
                            summary: &summary,
                            is_error,
                        });
                        results.push(tool_result(&call.id, &body, is_error));
                    }

                    // One user message carrying every result.
                    staged.push(Message::user_blocks(results));
                }

                // A server-side tool hit its iteration cap. Resend as-is; the
                // server picks up where it left off.
                Some("pause_turn") => continue,

                Some("max_tokens") => {
                    return Err(HarnessError::StoppedEarly(
                        "hit max_tokens — raise Agent::max_tokens or lower effort".into(),
                    ))
                }

                Some("refusal") => {
                    return Err(HarnessError::StoppedEarly(
                        "the model declined this request".into(),
                    ))
                }

                Some(other) => {
                    return Err(HarnessError::StoppedEarly(format!(
                        "unrecognized stop_reason: {other}"
                    )))
                }
            }
        }
    }

    fn check_cancel(cancel: Option<&CancelFlag>) -> Result<()> {
        if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
            Err(HarnessError::Cancelled)
        } else {
            Ok(())
        }
    }

    async fn execute_tool_call(
        &self,
        call: &crate::anthropic::types::ToolUse,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
        cancel: Option<&CancelFlag>,
    ) -> (String, bool) {
        if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
            return ("turn cancelled before tool ran".into(), true);
        }

        // Prepare once before approval so preview, path, and pre-image fingerprint
        // are the same plan that will execute.
        let prepared = match self.tools.prepare(&call.name, call.input.clone()) {
            Ok(prepared) => prepared,
            Err(message) => {
                return (
                    format!("cannot prepare `{}`: {message}", call.name),
                    true,
                );
            }
        };

        let risk = prepared.risk;
        if risk.requires_approval() {
            let preview = prepared.preview.clone();
            let approval_id = new_id("approval");
            // Register the waiter before the UI sees the event.
            self.approver.prepare(&approval_id).await;

            on_event(StreamEvent::ApprovalNeeded {
                approval_id: approval_id.clone(),
                tool_name: call.name.clone(),
                tool_call_id: call.id.clone(),
                risk,
                path: preview.path.clone(),
                summary: preview.summary.clone(),
                diff: preview.diff.clone(),
            });

            let summary_for_deny = preview.summary.clone();
            let request = ApprovalRequest {
                approval_id,
                tool_name: call.name.clone(),
                tool_call_id: call.id.clone(),
                risk,
                preview,
            };

            match self.approver.decide(&request).await {
                ApprovalDecision::AllowOnce => {}
                ApprovalDecision::Deny => {
                    return (
                        format!(
                            "user denied permission to run `{}` ({summary_for_deny})",
                            call.name
                        ),
                        true,
                    );
                }
            }

            if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
                return ("turn cancelled during approval".into(), true);
            }
        }

        match self.tools.execute_prepared(prepared).await {
            Ok(output) => (output, false),
            Err(message) => (message, true),
        }
    }
}

/// Short one-line preview for UI / CLI tool result markers.
fn summarize_tool_body(body: &str) -> String {
    const MAX: usize = 160;
    let flat: String = body
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= MAX {
        return flat;
    }
    let truncated: String = flat.chars().take(MAX.saturating_sub(1)).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::types::Usage;
    use crate::auth::AuthStatus;
    use crate::provider::Completion;
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    struct FakeProvider {
        calls: AtomicUsize,
        fail_after: Option<usize>,
        stop: &'static str,
    }

    #[async_trait]
    impl Provider for FakeProvider {
        fn id(&self) -> &str {
            "fake"
        }

        fn default_model(&self) -> &str {
            "fake-model"
        }

        fn auth_status(&self) -> AuthStatus {
            AuthStatus::Ready { account: None }
        }

        async fn stream_turn(
            &self,
            _req: &TurnRequest,
            on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
        ) -> Result<Completion> {
            let n = self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            if self.fail_after == Some(n) {
                return Err(HarnessError::Other("provider boom".into()));
            }
            on_event(StreamEvent::Text("hi"));
            Ok(Completion {
                content: vec![json!({ "type": "text", "text": "hi" })],
                stop_reason: Some(self.stop.into()),
                usage: Usage::default(),
                limits: None,
            })
        }
    }

    #[tokio::test]
    async fn successful_turn_commits_wire_history() {
        let provider: Arc<dyn Provider> = Arc::new(FakeProvider {
            calls: AtomicUsize::new(0),
            fail_after: None,
            stop: "end_turn",
        });
        let mut agent = Agent::new(provider, ToolRegistry::new());
        let mut sink = |_ev: StreamEvent<'_>| {};
        agent.send("hello", &mut sink).await.unwrap();
        assert_eq!(agent.messages.len(), 2);
    }

    #[tokio::test]
    async fn provider_error_does_not_commit_staged_history() {
        let provider: Arc<dyn Provider> = Arc::new(FakeProvider {
            calls: AtomicUsize::new(0),
            fail_after: Some(0),
            stop: "end_turn",
        });
        let mut agent = Agent::new(provider, ToolRegistry::new());
        agent.messages.push(Message::user_text("prior"));
        let prior_len = agent.messages.len();
        let mut sink = |_ev: StreamEvent<'_>| {};
        let err = agent.send("new", &mut sink).await.unwrap_err();
        assert!(matches!(err, HarnessError::Other(_)));
        assert_eq!(agent.messages.len(), prior_len);
        assert_eq!(agent.messages[0].role, "user");
    }

    #[tokio::test]
    async fn cancel_flag_aborts_before_provider_call() {
        let provider: Arc<dyn Provider> = Arc::new(FakeProvider {
            calls: AtomicUsize::new(0),
            fail_after: None,
            stop: "end_turn",
        });
        let mut agent = Agent::new(provider, ToolRegistry::new());
        let cancel = CancelFlag::new();
        cancel.cancel();
        let mut sink = |_ev: StreamEvent<'_>| {};
        let err = agent
            .send_cancellable("hello", &mut sink, Some(&cancel))
            .await
            .unwrap_err();
        assert!(matches!(err, HarnessError::Cancelled));
        assert!(agent.messages.is_empty());
    }
}
