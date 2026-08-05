//! Context-window estimates for the chat chrome.
//!
//! Honest labels: last-turn `input_tokens` from the API when available; otherwise
//! a char/4 estimate over system + conversation (no tool-schema stringify).

use serde::Serialize;
use zest_core::Agent;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageView {
    pub used_tokens: u64,
    pub window_tokens: u64,
    pub remaining_tokens: u64,
    pub percent_full: f64,
    /// `last_turn` | `estimate`
    pub source: String,
    pub system_tokens: u64,
    pub conversation_tokens: u64,
    pub message_count: usize,
    pub checkpoint_count: usize,
    pub can_compact: bool,
}

pub fn context_window_for_model(model: &str) -> u64 {
    zest_core::context_window_for_model(model)
}

fn chars_to_tok(chars: u64) -> u64 {
    if chars == 0 {
        0
    } else {
        (chars / 4).max(1)
    }
}

pub fn estimate_context(agent: &Agent, checkpoint_count: usize) -> ContextUsageView {
    let window = agent
        .descriptor()
        .models
        .into_iter()
        .find(|model| model.id == agent.model)
        .map(|model| model.context_window)
        .filter(|window| *window > 0)
        .unwrap_or_else(|| context_window_for_model(&agent.model));

    let system_tokens = chars_to_tok(agent.system.as_deref().unwrap_or("").chars().count() as u64);
    let conversation_tokens: u64 = agent
        .messages
        .iter()
        .map(|message| {
            chars_to_tok(
                message
                    .content
                    .iter()
                    .map(|block| block.to_string().len() as u64)
                    .sum(),
            )
        })
        .sum();

    let (used, source) = match &agent.last_usage {
        Some(u) if u.input_tokens > 0 => (u.input_tokens as u64, "last_turn"),
        _ => (system_tokens + conversation_tokens, "estimate"),
    };

    let remaining = window.saturating_sub(used);
    let percent_full = if window == 0 {
        0.0
    } else {
        ((used as f64) / (window as f64) * 100.0).min(100.0)
    };

    ContextUsageView {
        used_tokens: used,
        window_tokens: window,
        remaining_tokens: remaining,
        percent_full,
        source: source.into(),
        system_tokens,
        conversation_tokens,
        message_count: agent.messages.len(),
        checkpoint_count,
        can_compact: conversation_tokens > 4_000 && agent.messages.len() >= 4,
    }
}
