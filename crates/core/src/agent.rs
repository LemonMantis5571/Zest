//! The agent loop.
//!
//! Request the model, execute whatever tools it asks for, feed the results back,
//! repeat until it stops asking. Everything interesting about a harness lives
//! either side of this file — the provider layer above it, the tool layer and the
//! permission model below — but this is the spine.
//!
//! The loop is provider-agnostic. It describes the turn it wants and lets the
//! provider decide how to express that on the wire.

use std::sync::{Arc, Mutex};

use crate::anthropic::types::{tool_result, tool_uses, Message};
use crate::error::{HarnessError, Result};
use crate::provider::{Provider, StreamEvent, TurnRequest};
use crate::tools::ToolRegistry;
use crate::usage::Ledger;

pub struct Agent {
    provider: Arc<dyn Provider>,
    tools: ToolRegistry,
    /// Shared so delegated sub-agents on other providers bill into the same book.
    ledger: Option<Arc<Mutex<Ledger>>>,
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

    /// Which provider this agent spends against. Keyed on by the usage ledger.
    pub fn provider_id(&self) -> &str {
        self.provider.id()
    }

    /// Send one user message and run to completion, executing tools as asked.
    pub async fn send(
        &mut self,
        user_input: &str,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
    ) -> Result<()> {
        self.messages.push(Message::user_text(user_input));

        loop {
            let request = TurnRequest {
                model: self.model.clone(),
                system: self.system.clone(),
                messages: self.messages.clone(),
                tools: self.tools.definitions(),
                max_tokens: self.max_tokens,
                effort: Some(self.effort.clone()),
                thinking: true,
            };

            let completion = self.provider.stream_turn(&request, &mut *on_event).await?;

            // Bill it before anything else can fail. A poisoned lock is skipped
            // rather than propagated — accounting must not abort a paid-for turn.
            if let Some(ledger) = &self.ledger {
                if let Ok(mut ledger) = ledger.lock() {
                    ledger.record(self.provider.id(), &completion);
                }
            }

            // Echo the assistant turn back verbatim — thinking signatures and
            // tool_use blocks both have to survive intact.
            self.messages
                .push(Message::assistant(completion.content.clone()));

            match completion.stop_reason.as_deref() {
                Some("end_turn") | None => return Ok(()),

                Some("tool_use") => {
                    let calls = tool_uses(&completion.content);
                    if calls.is_empty() {
                        return Err(HarnessError::Other(
                            "stop_reason was tool_use but no tool_use block was present".into(),
                        ));
                    }

                    let mut results = Vec::with_capacity(calls.len());
                    for call in calls {
                        let (body, is_error) =
                            match self.tools.run(&call.name, call.input.clone()).await {
                                Ok(output) => (output, false),
                                Err(message) => (message, true),
                            };
                        results.push(tool_result(&call.id, &body, is_error));
                    }

                    // One user message carrying every result.
                    self.messages.push(Message::user_blocks(results));
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
}
