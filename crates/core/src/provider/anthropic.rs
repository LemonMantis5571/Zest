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

use super::{Completion, Provider, StreamEvent, TurnRequest};
use crate::anthropic::client::AnthropicClient;
use crate::anthropic::types::{OutputConfig, Request, Thinking, DEFAULT_MODEL};
use crate::auth::AuthStatus;
use crate::error::Result;

pub struct AnthropicProvider {
    id: String,
    client: AnthropicClient,
    default_model: String,
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
        Ok(Self {
            id: "anthropic".to_string(),
            client: AnthropicClient::new(api_key)?,
            default_model: DEFAULT_MODEL.to_string(),
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
        Ok(Self {
            id: id.into(),
            client: AnthropicClient::new(api_key)?.with_base_url(base_url),
            default_model: default_model.into(),
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

    async fn stream_turn(
        &self,
        req: &TurnRequest,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
    ) -> Result<Completion> {
        let wire = Request {
            model: req.model.clone(),
            max_tokens: req.max_tokens,
            stream: true,
            system: req.system.clone(),
            messages: req.messages.clone(),
            tools: req.tools.clone(),
            thinking: (self.extensions && req.thinking).then(Thinking::default),
            output_config: match (self.extensions, req.effort.as_ref()) {
                (true, Some(effort)) => Some(OutputConfig {
                    effort: effort.clone(),
                }),
                _ => None,
            },
        };

        self.client.stream(&wire, on_event).await
    }
}
