//! Provider backed by the Messages API.
//!
//! Serves two cases from one implementation:
//!
//! - **Native** — Anthropic's API on an Anthropic key.
//! - **Gateway** — a proxy (CLIProxyAPI, LiteLLM, …) that re-exposes some other
//!   backend as the Messages API. A Codex or Gemini login reached this way needs
//!   no second wire protocol in the harness.
//!
//! The only behavioural difference is whether Anthropic-only request fields are
//! sent. That decision belongs here rather than in the agent loop, which is why
//! `Agent` no longer carries a flag for it.

use async_trait::async_trait;
use serde_json::Value;

use super::{catalogue_from_lists, Completion, ModelSpec, Provider, StreamEvent, TurnRequest};
use crate::anthropic::client::AnthropicClient;
use crate::anthropic::types::{
    cached_system_blocks, ephemeral_cache_control, Message, OutputConfig, Request, Thinking,
    DEFAULT_MODEL,
};
use crate::auth::AuthStatus;
use crate::error::Result;

pub struct AnthropicProvider {
    id: String,
    client: AnthropicClient,
    default_model: String,
    models: Vec<ModelSpec>,
    /// Presence only — the key itself is never inspected or reported.
    has_key: bool,
    /// Whether the endpoint understands `thinking` and `output_config.effort`.
    /// False behind a gateway fronting a non-Anthropic model: those fields are
    /// meaningless there, and are dropped or rejected depending on the proxy.
    extensions: bool,
}

impl AnthropicProvider {
    /// Anthropic's own API.
    pub fn native(api_key: String) -> Result<Self> {
        let has_key = !api_key.trim().is_empty();
        let default_model = DEFAULT_MODEL.to_string();
        Ok(Self {
            id: "anthropic".to_string(),
            client: AnthropicClient::new(api_key)?,
            models: catalogue_from_lists(&default_model, &[], &[]),
            default_model,
            has_key,
            extensions: true,
        })
    }

    /// A Messages-API-speaking gateway in front of some other backend.
    ///
    /// `id` is what routing rules and the usage ledger key on, so it should name
    /// the *account* being spent (`"codex"`), not the proxy.
    pub fn gateway(
        id: impl Into<String>,
        api_key: String,
        base_url: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Result<Self> {
        let has_key = !api_key.trim().is_empty();
        let default_model = default_model.into();
        Ok(Self {
            id: id.into(),
            client: AnthropicClient::new(api_key)?.with_base_url(base_url),
            // No optional catalogue → only the configured default is accepted.
            models: catalogue_from_lists(&default_model, &[], &[]),
            default_model,
            has_key,
            extensions: false,
        })
    }

    /// Name this provider after the account it spends, not the transport.
    /// Routing rules and the usage ledger key on this.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        if !self.models.iter().any(|m| m.id == self.default_model) {
            let efforts = self
                .models
                .first()
                .map(|m| m.efforts.clone())
                .unwrap_or_else(|| {
                    super::STANDARD_EFFORTS
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect()
                });
            self.models.insert(
                0,
                ModelSpec {
                    id: self.default_model.clone(),
                    efforts,
                    context_window: super::context_window_for_model(&self.default_model),
                    supports_tools: true,
                    supports_vision: false,
                },
            );
        }
        self
    }

    /// Replace the model/effort catalogue (from gateway config allow-lists).
    pub fn with_models(mut self, models: Vec<ModelSpec>) -> Self {
        self.models = models;
        self
    }

    /// Override whether Anthropic extensions are sent.
    ///
    /// Only needed for a gateway that genuinely fronts an Anthropic model and can
    /// pass the fields through — the constructors already pick the right default.
    pub fn with_extensions(mut self, extensions: bool) -> Self {
        self.extensions = extensions;
        self
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn models(&self) -> Vec<ModelSpec> {
        self.models.clone()
    }

    /// This provider authenticates with a key it was handed, so its status is
    /// simply whether it has one. Providers backed by a vendor CLI sign-in
    /// report from `crate::auth` detection instead.
    fn auth_status(&self) -> AuthStatus {
        if self.has_key {
            AuthStatus::Ready { account: None }
        } else {
            AuthStatus::Unconfigured
        }
    }

    /// Native Anthropic only. `extensions` already means "this really is the
    /// Messages API, not a translation layer", which is exactly the condition
    /// under which `cache_control` means anything.
    fn supports_prompt_cache(&self) -> bool {
        self.extensions
    }

    async fn stream_turn(
        &self,
        req: &TurnRequest,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
    ) -> Result<Completion> {
        let caching = self.supports_prompt_cache();

        let mut tools = req.tools.clone();
        // One breakpoint on the last tool covers the entire tool list — the
        // largest fixed prefix any request has.
        if caching {
            if let Some(last) = tools.last_mut() {
                last.cache_control = Some(ephemeral_cache_control());
            }
        }

        let system = req.system.as_ref().map(|text| {
            if caching {
                // A second breakpoint here extends the cached region to cover
                // tools + system, which together are stable for a whole session.
                cached_system_blocks(text)
            } else {
                Value::String(text.clone())
            }
        });

        let mut messages = req.messages.clone();
        if caching {
            mark_conversation_prefix(&mut messages);
        }

        let wire = Request {
            model: req.model.clone(),
            max_tokens: req.max_tokens,
            stream: true,
            system,
            messages,
            tools,
            thinking: (self.extensions && req.thinking).then(Thinking::default),
            output_config: match (self.extensions, req.effort.as_ref()) {
                (true, Some(effort)) => Some(OutputConfig {
                    effort: effort.clone(),
                }),
                _ => None,
            },
        };

        self.client
            .stream_cancellable(&wire, on_event, req.cancel.as_ref())
            .await
    }
}

/// Put a rolling breakpoint near the end of the conversation so the history
/// that already exists is read from cache instead of reprocessed every turn.
///
/// It goes on the **second-to-last** message, not the last. The last message is
/// the one that just changed; a breakpoint there would write a new cache entry
/// every turn and read none of it back. One message earlier is the newest point
/// that was also present on the previous request.
fn mark_conversation_prefix(messages: &mut [Message]) {
    let Some(index) = messages.len().checked_sub(2) else {
        return;
    };
    let Some(block) = messages[index].content.last_mut() else {
        return;
    };
    // Only object-shaped blocks take cache_control. Thinking blocks carry a
    // signature that must round-trip byte for byte, so never touch those.
    let Some(map) = block.as_object_mut() else {
        return;
    };
    if map.get("type").and_then(Value::as_str) == Some("thinking") {
        return;
    }
    map.insert("cache_control".into(), ephemeral_cache_control());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn conversation(n: usize) -> Vec<Message> {
        (0..n)
            .map(|i| {
                if i % 2 == 0 {
                    Message::user_text(format!("u{i}"))
                } else {
                    Message::assistant(vec![json!({ "type": "text", "text": format!("a{i}") })])
                }
            })
            .collect()
    }

    fn cached_indices(messages: &[Message]) -> Vec<usize> {
        messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.content.iter().any(|b| b.get("cache_control").is_some()))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn rolling_breakpoint_lands_on_the_second_to_last_message() {
        // The last message is the one that just changed. A breakpoint there
        // would write a fresh entry every turn and never read one back.
        let mut messages = conversation(5);
        mark_conversation_prefix(&mut messages);
        assert_eq!(cached_indices(&messages), vec![3]);
    }

    #[test]
    fn a_single_message_conversation_gets_no_breakpoint() {
        let mut messages = conversation(1);
        mark_conversation_prefix(&mut messages);
        assert!(cached_indices(&messages).is_empty());
        let mut empty: Vec<Message> = Vec::new();
        mark_conversation_prefix(&mut empty);
    }

    #[test]
    fn a_thinking_block_is_never_annotated() {
        // Thinking blocks carry a signature that must echo back byte for byte;
        // adding a key to one would invalidate the next request.
        let mut messages = vec![
            Message::assistant(vec![
                json!({ "type": "thinking", "thinking": "hmm", "signature": "sig" }),
            ]),
            Message::user_text("next"),
        ];
        mark_conversation_prefix(&mut messages);
        assert!(cached_indices(&messages).is_empty());
        assert_eq!(messages[0].content[0]["signature"], "sig");
    }

    #[test]
    fn gateway_sends_a_plain_string_system_and_no_cache_control() {
        let provider =
            AnthropicProvider::gateway("codex", "k".into(), "http://x", "gpt-5.6-sol").unwrap();
        assert!(!provider.supports_prompt_cache());
    }

    #[test]
    fn native_provider_reports_cache_support() {
        let provider = AnthropicProvider::native("k".into()).unwrap();
        assert!(provider.supports_prompt_cache());
    }

    /// Serializing the wire request is the only way to prove the shape the API
    /// actually receives, including that untouched fields stay absent.
    #[test]
    fn cached_system_serializes_as_a_block_array() {
        let plain = Request {
            model: "m".into(),
            max_tokens: 1,
            stream: true,
            system: Some(Value::String("hello".into())),
            messages: Vec::new(),
            tools: Vec::new(),
            thinking: None,
            output_config: None,
        };
        let json = serde_json::to_value(&plain).unwrap();
        assert_eq!(json["system"], json!("hello"));

        let cached = Request {
            system: Some(cached_system_blocks("hello")),
            ..plain
        };
        let json = serde_json::to_value(&cached).unwrap();
        assert_eq!(json["system"][0]["text"], json!("hello"));
        assert_eq!(
            json["system"][0]["cache_control"]["type"],
            json!("ephemeral")
        );
    }

    #[test]
    fn tool_defs_omit_cache_control_entirely_when_unset() {
        let def = crate::anthropic::types::ToolDef {
            name: "t".into(),
            description: "d".into(),
            input_schema: json!({}),
            cache_control: None,
        };
        let json = serde_json::to_value(&def).unwrap();
        assert!(
            json.get("cache_control").is_none(),
            "a gateway must not see the field at all: {json}"
        );
    }
}
