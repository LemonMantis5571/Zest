//! Provider abstraction.
//!
//! A provider is one authenticated backend: Anthropic on a Claude login, Codex on
//! a ChatGPT login, or an explicitly configured OpenAI-compatible endpoint. The
//! agent loop must not know which backend it is talking to.
//!
//! Crucially, *how* a provider is reached is an implementation detail behind this
//! trait. Anthropic is reached natively; another backend may be reached through a
//! gateway that re-exposes it as the Messages API. The agent loop cannot tell the
//! difference, which lets a gateway be swapped for a native client later without
//! anything above noticing.

pub mod anthropic;
pub mod openai_compatible;
pub mod registry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::anthropic::types::{Message, ToolDef, Usage, DEFAULT_MODEL};
use crate::auth::AuthStatus;
use crate::config::ProviderConfig;
use crate::error::Result;

/// Build a picker/validation catalogue from config without loading credentials.
pub fn descriptor_from_config(provider_id: &str, config: &ProviderConfig) -> ProviderDescriptor {
    match config {
        ProviderConfig::Anthropic { model, .. } => {
            let default_model = model.clone().unwrap_or_else(|| DEFAULT_MODEL.to_string());
            ProviderDescriptor {
                id: provider_id.to_string(),
                default_model: default_model.clone(),
                models: catalogue_for_provider(provider_id, &default_model, &[], &[]),
            }
        }
        ProviderConfig::Gateway {
            model,
            models,
            efforts,
            ..
        } => ProviderDescriptor {
            id: provider_id.to_string(),
            default_model: model.clone(),
            models: catalogue_for_provider(provider_id, model, models, efforts),
        },
        ProviderConfig::OpenaiCompatible { model, models, .. } => ProviderDescriptor {
            id: provider_id.to_string(),
            default_model: model.clone(),
            // The OpenAI-compatible adapter deliberately does not send a
            // vendor-specific reasoning/effort field yet. Do not advertise a
            // selector that would look authoritative but cannot affect the
            // request.
            models: catalogue_without_efforts(model, models),
        },
    }
}

/// Fallback catalogue when a picker id is not present in `zest.toml`.
pub fn descriptor_for_picker_id(provider_id: &str) -> ProviderDescriptor {
    let default_model = match provider_id {
        "codex" => "gpt-5.6-sol".to_string(),
        "claude" | "anthropic" => DEFAULT_MODEL.to_string(),
        "antigravity" => "gemini-3.1-pro-high".to_string(),
        _ => DEFAULT_MODEL.to_string(),
    };
    ProviderDescriptor {
        id: provider_id.to_string(),
        default_model: default_model.clone(),
        models: catalogue_for_provider(provider_id, &default_model, &[], &[]),
    }
}

/// Efforts every provider understands today (Anthropic + CLIProxyAPI mapping).
pub const STANDARD_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// One selectable model and the efforts it accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    /// When non-empty, only these efforts are valid for this model.
    pub efforts: Vec<String>,
    /// Conservative context window used by the desktop meter. Providers can
    /// still report a measured input count; this is only the model catalogue's
    /// capacity hint.
    #[serde(default)]
    pub context_window: u64,
    /// Whether the model catalogue expects standard function/tool definitions.
    #[serde(default = "default_true")]
    pub supports_tools: bool,
    /// Whether the catalogue explicitly opts into image input.
    #[serde(default)]
    pub supports_vision: bool,
}

fn default_true() -> bool {
    true
}

/// Conservative built-in capacities for the models Zest knows without model
/// discovery. Explicit provider catalogues remain authoritative for model ids;
/// these values only keep the UI honest when no capacity was configured.
pub fn context_window_for_model(model: &str) -> u64 {
    let model = model.to_ascii_lowercase();
    if model.contains("gpt-5.6") || model.contains("luna") || model.contains("codex") {
        256_000
    } else if model.contains("claude") {
        200_000
    } else {
        128_000
    }
}

fn model_spec(id: String, efforts: Vec<String>) -> ModelSpec {
    ModelSpec {
        context_window: context_window_for_model(&id),
        id,
        efforts,
        supports_tools: true,
        supports_vision: false,
    }
}

/// Static catalogue a provider exposes for pickers and session validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    pub id: String,
    pub default_model: String,
    pub models: Vec<ModelSpec>,
}

/// Normalize UI / env effort aliases to the wire form.
pub fn normalize_effort(effort: &str) -> String {
    match effort.trim().to_ascii_lowercase().as_str() {
        "low" | "medium" | "high" | "xhigh" | "max" => effort.trim().to_ascii_lowercase(),
        "extra" | "extra high" | "extra_high" => "xhigh".into(),
        "med" => "medium".into(),
        _ => "high".into(),
    }
}

/// Built-in Codex catalogue used when `zest.toml` omits `models` for provider `codex`.
///
/// Mirrors the desktop picker (`CODEX_MODELS` in the UI). Keep these in sync.
pub const CODEX_KNOWN_MODELS: &[&str] = &[
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.4-mini",
];

/// Build a catalogue from an optional allow-list.
///
/// When `models` is empty, only `default_model` is accepted (generic gateways).
/// Prefer [`catalogue_for_provider`] for known provider ids. When `efforts` is
/// empty, [`STANDARD_EFFORTS`] is used.
pub fn catalogue_from_lists(
    default_model: &str,
    models: &[String],
    efforts: &[String],
) -> Vec<ModelSpec> {
    let efforts: Vec<String> = if efforts.is_empty() {
        STANDARD_EFFORTS.iter().map(|s| (*s).to_string()).collect()
    } else {
        efforts.to_vec()
    };
    let mut ids: Vec<String> = if models.is_empty() {
        vec![default_model.to_string()]
    } else {
        models.to_vec()
    };
    if !ids.iter().any(|m| m == default_model) {
        ids.insert(0, default_model.to_string());
    }
    ids.into_iter()
        .map(|id| ModelSpec {
            ..model_spec(id, efforts.clone())
        })
        .collect()
}

/// Build a model catalogue for a provider whose transport does not expose a
/// reasoning-effort control. Empty effort lists mean “no effort selector” to
/// the desktop while model validation still remains authoritative.
pub fn catalogue_without_efforts(default_model: &str, models: &[String]) -> Vec<ModelSpec> {
    let mut ids: Vec<String> = if models.is_empty() {
        vec![default_model.to_string()]
    } else {
        models.to_vec()
    };
    if !ids.iter().any(|m| m == default_model) {
        ids.insert(0, default_model.to_string());
    }
    ids.into_iter()
        .map(|id| ModelSpec {
            ..model_spec(id, Vec::new())
        })
        .collect()
}

/// Like [`catalogue_from_lists`], but provider `codex` gets [`CODEX_KNOWN_MODELS`]
/// when the config omit `models` — so sticky/UI picks (Sol/Terra/Luna) validate.
pub fn catalogue_for_provider(
    provider_id: &str,
    default_model: &str,
    models: &[String],
    efforts: &[String],
) -> Vec<ModelSpec> {
    if models.is_empty() && provider_id == "codex" {
        let builtin: Vec<String> = CODEX_KNOWN_MODELS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        return catalogue_from_lists(default_model, &builtin, efforts);
    }
    catalogue_from_lists(default_model, models, efforts)
}

fn validate_against(
    models: &[ModelSpec],
    provider_id: &str,
    model: &str,
    effort: &str,
) -> std::result::Result<(), String> {
    let spec = models.iter().find(|m| m.id == model).ok_or_else(|| {
        let known: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
        format!(
            "model `{model}` is not supported by provider `{provider_id}` (known: {})",
            known.join(", ")
        )
    })?;
    if !spec.efforts.is_empty() && !spec.efforts.iter().any(|e| e == effort) {
        return Err(format!(
            "effort `{effort}` is not supported for model `{model}` on provider `{provider_id}` (known: {})",
            spec.efforts.join(", ")
        ));
    }
    Ok(())
}

/// One model turn, described without reference to any provider's wire format.
///
/// `effort` and `thinking` are *requests*, not commands. A provider maps them
/// onto its own controls or ignores them — which is why the agent loop no longer
/// carries a flag for whether the backend understands Anthropic's extensions.
pub struct TurnRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    /// Budgets reasoning *and* response text together on providers that think.
    pub max_tokens: u32,
    pub effort: Option<String>,
    pub thinking: bool,
    /// When set, the provider races the HTTP/SSE work against this token and
    /// aborts the body on cancel (drop).
    pub cancel: Option<crate::cancel::CancelToken>,
}

/// Incremental output, for rendering. Everything here is also present in the
/// final `Completion` — a front-end that ignores these still gets a correct turn.
#[derive(Debug)]
pub enum StreamEvent<'a> {
    Text(&'a str),
    Thinking(&'a str),
    ToolCallStart {
        name: &'a str,
        id: &'a str,
    },
    /// Emitted after a local tool finishes. `summary` is a short preview of the body.
    /// `metadata` is a typed UI/persist side-channel (never model wire content).
    ToolCallResult {
        name: &'a str,
        id: &'a str,
        summary: &'a str,
        is_error: bool,
        path: Option<&'a str>,
        diff: Option<&'a str>,
        metadata: Option<crate::tools::ToolMetadata>,
    },
    /// A provider-independent interactive question requested by the model.
    /// The agent waits for the front-end answer before emitting the matching
    /// tool result and continuing the turn.
    QuestionNeeded {
        question_id: String,
        tool_call_id: String,
        prompt: String,
        choices: Vec<String>,
        multiple: bool,
        placeholder: Option<String>,
    },
    /// The endpoint served a different model than the one asked for.
    ///
    /// Emitted at most once per turn and **only on disagreement** — silence
    /// means the request was honoured. Worth surfacing because nothing else can
    /// tell you: a gateway may route anywhere, and a model's own account of
    /// which model it is amounts to a guess.
    ModelSubstituted {
        requested: String,
        served: String,
    },
    /// A gated tool is waiting on the user (write/exec). Owned strings so the
    /// preview can outlive the tool-call stack frame.
    ApprovalNeeded {
        approval_id: String,
        tool_name: String,
        tool_call_id: String,
        risk: crate::tools::approval::ToolRisk,
        path: String,
        summary: String,
        diff: String,
    },
}

/// Throughput headroom as reported by the provider.
///
/// This answers "can I send right now", **not** "how much of my plan is left".
/// Those are different numbers and merging them produces a confident lie — see
/// `memory/decisions.md`. Subscription quota has no equivalent endpoint on any of
/// the CLI-login providers, so it is metered locally instead.
///
/// `None` on a provider means it reports nothing, which is itself information the
/// ledger has to represent rather than silently treat as zero.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateLimitSnapshot {
    pub requests_limit: Option<u64>,
    pub requests_remaining: Option<u64>,
    pub requests_reset: Option<String>,
    pub input_tokens_remaining: Option<u64>,
    pub output_tokens_remaining: Option<u64>,
    /// RFC 3339, stored raw. Nothing parses dates yet, so no date dependency.
    pub tokens_reset: Option<String>,
    pub retry_after_secs: Option<u64>,
}

impl RateLimitSnapshot {
    /// True when the provider reported nothing at all.
    pub fn is_empty(&self) -> bool {
        self.requests_limit.is_none()
            && self.requests_remaining.is_none()
            && self.requests_reset.is_none()
            && self.input_tokens_remaining.is_none()
            && self.output_tokens_remaining.is_none()
            && self.tokens_reset.is_none()
            && self.retry_after_secs.is_none()
    }
}

#[derive(Debug)]
pub struct Completion {
    /// The assistant turn's content blocks, in index order. Push this back into
    /// the message history verbatim.
    pub content: Vec<Value>,
    pub stop_reason: Option<String>,
    pub usage: Usage,
    /// Whether the endpoint actually reported token usage for this turn.
    pub usage_available: bool,
    pub limits: Option<RateLimitSnapshot>,
    /// The model the endpoint says actually served this turn.
    ///
    /// Distinct from the model that was *requested*, and the only trustworthy
    /// statement of which one ran: asking the model itself yields a guess, and a
    /// gateway is free to route a request anywhere. `None` means the endpoint
    /// did not say, which is not the same as agreeing.
    pub served_model: Option<String>,
}

/// Send the smallest possible real turn, to find out whether this provider can
/// actually serve one.
///
/// Presence of a credentials file is not the same as a working session: a
/// gateway can hold an account it has put into cooldown, or a key can be
/// revoked, and neither shows up on disk. The only honest way to say "signed
/// in" is to have been served.
///
/// Costs a few tokens, so this belongs on an explicit action — after a sign-in,
/// not on every render.
pub async fn probe(provider: &dyn Provider, model: &str) -> Result<()> {
    let request = TurnRequest {
        model: model.to_string(),
        system: None,
        messages: vec![Message::user_text("hi")],
        tools: Vec::new(),
        max_tokens: 1,
        effort: None,
        // Thinking would ignore max_tokens: 1 and make the cheapest possible
        // probe an expensive one.
        thinking: false,
        cancel: None,
    };
    let mut sink = |_: StreamEvent<'_>| {};
    match provider.stream_turn(&request, &mut sink).await {
        Ok(_) => Ok(()),
        // `max_tokens` is the expected way for this to end: the turn was
        // served, which is the entire question being asked.
        Err(crate::error::HarnessError::StoppedEarly(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// Stable identifier used by configuration and the usage ledger.
    fn id(&self) -> &str;

    fn default_model(&self) -> &str;

    /// Models this provider accepts, with per-model effort allow-lists.
    ///
    /// Default: only [`Self::default_model`] with [`STANDARD_EFFORTS`].
    fn models(&self) -> Vec<ModelSpec> {
        catalogue_from_lists(self.default_model(), &[], &[])
    }

    /// Picker / validation view of this provider.
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: self.id().to_string(),
            default_model: self.default_model().to_string(),
            models: self.models(),
        }
    }

    /// Reject unknown model / effort pairs before a turn spends quota.
    fn validate_selection(&self, model: &str, effort: &str) -> std::result::Result<(), String> {
        validate_against(&self.models(), self.id(), model, effort)
    }

    /// Whether this provider can be used right now. Rendered by the launch
    /// picker and checked before starting a parent conversation here.
    fn auth_status(&self) -> AuthStatus;

    /// Whether the endpoint honours Anthropic prompt caching (`cache_control`).
    ///
    /// Defaults to false, which is the honest answer for anything that is not
    /// Anthropic's own API. A gateway fronting a GPT or Gemini backend has no
    /// equivalent, and sending the field there is at best ignored and at worst
    /// a 400.
    fn supports_prompt_cache(&self) -> bool {
        false
    }

    /// The callback must be `Send`: provider futures are `Send` so that delegated
    /// sub-agents can run concurrently on the tokio runtime.
    async fn stream_turn(
        &self,
        req: &TurnRequest,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
    ) -> Result<Completion>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_models_list_accepts_only_default() {
        let cat = catalogue_from_lists("gpt-5.6-sol", &[], &[]);
        assert_eq!(cat.len(), 1);
        assert_eq!(cat[0].id, "gpt-5.6-sol");
        assert!(cat[0].efforts.contains(&"high".into()));
    }

    #[test]
    fn models_list_includes_default_if_missing() {
        let models = vec!["gpt-5.4".into()];
        let cat = catalogue_from_lists("gpt-5.6-sol", &models, &["low".into()]);
        assert_eq!(cat[0].id, "gpt-5.6-sol");
        assert_eq!(cat[1].id, "gpt-5.4");
        assert_eq!(cat[0].efforts, vec!["low".to_string()]);
    }

    #[test]
    fn codex_builtin_catalogue_includes_luna() {
        let cat = catalogue_for_provider("codex", "gpt-5.6-sol", &[], &[]);
        assert!(cat.iter().any(|m| m.id == "gpt-5.6-luna"));
        assert!(cat.iter().any(|m| m.id == "gpt-5.6-terra"));
        assert!(cat.iter().any(|m| m.id == "gpt-5.6-sol"));
    }

    #[test]
    fn other_gateway_empty_models_stays_default_only() {
        let cat = catalogue_for_provider("other", "gpt-5.6-sol", &[], &[]);
        assert_eq!(cat.len(), 1);
        assert_eq!(cat[0].id, "gpt-5.6-sol");
    }

    #[test]
    fn openai_compatible_catalogue_does_not_advertise_effort_controls() {
        let cat = catalogue_without_efforts(
            "deepseek-v4-flash",
            &["deepseek-v4-flash".into(), "deepseek-v4-pro".into()],
        );
        assert_eq!(cat.len(), 2);
        assert!(cat.iter().all(|model| model.efforts.is_empty()));
    }
}
