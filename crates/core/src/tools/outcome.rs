//! Tool results visible to the model, plus optional typed UI metadata.
//!
//! The model only ever sees [`ToolOutcome::body`]. Front-ends may also receive
//! [`ToolMetadata`] (delegation provenance, measured spend) without stuffing
//! structured JSON into the wire `tool_result`.

use serde::{Deserialize, Serialize};

/// What a tool returns after execution.
#[derive(Debug, Clone)]
pub struct ToolOutcome {
    /// Model-visible result string (also summarized for the UI when metadata
    /// does not replace the card copy).
    pub body: String,
    /// Optional typed side-channel for the UI / persistence. Never sent on the
    /// Messages API wire as structured content.
    pub metadata: Option<ToolMetadata>,
}

impl ToolOutcome {
    pub fn text(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            metadata: None,
        }
    }

    pub fn with_metadata(body: impl Into<String>, metadata: ToolMetadata) -> Self {
        Self {
            body: body.into(),
            metadata: Some(metadata),
        }
    }
}

impl From<String> for ToolOutcome {
    fn from(body: String) -> Self {
        Self::text(body)
    }
}

impl From<&str> for ToolOutcome {
    fn from(body: &str) -> Self {
        Self::text(body)
    }
}

/// Measured token/request delta attributed to one tool run (Zest metering).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDelta {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// One provider skipped while resolving a delegate target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedProvider {
    pub provider_id: String,
    pub reason: String,
}

/// Typed tool side-channel. Extend with new variants; unknown variants must not
/// break older UIs (serde will fail closed on load — prefer additive fields).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolMetadata {
    Delegation {
        provider_id: String,
        model: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        routing_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<SkippedProvider>,
        #[serde(default)]
        usage_delta: UsageDelta,
    },
}

impl ToolMetadata {
    pub fn delegation_label(&self) -> Option<String> {
        match self {
            Self::Delegation {
                provider_id, model, ..
            } => Some(format!("Delegated to {provider_id} · {model}")),
        }
    }
}
