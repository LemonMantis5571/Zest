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
}

pub fn context_window_for_model(model: &str) -> u64 {
    let m = model.to_ascii_lowercase();
    if m.contains("gpt-5.6") || m.contains("luna") || m.contains("codex") {
        256_000
    } else {
        // Claude models and anything unrecognized. 200k is the conservative
        // floor — overstating the window would understate how full it is.
        200_000
    }
}

fn chars_to_tok(chars: u64) -> u64 {
    if chars == 0 {
        0
    } else {
        (chars / 4).max(1)
    }
}

pub fn estimate_context(agent: &Agent) -> ContextUsageView {
    let window = context_window_for_model(&agent.model);

    let (used, source) = match &agent.last_usage {
        Some(u) if u.input_tokens > 0 => (u.input_tokens as u64, "last_turn"),
        _ => {
            let system_chars = agent.system.as_deref().unwrap_or("").chars().count() as u64;
            let conv_chars: u64 = agent
                .messages
                .iter()
                .map(|m| {
                    m.content
                        .iter()
                        .map(|b| b.to_string().len() as u64)
                        .sum::<u64>()
                })
                .sum();
            (
                chars_to_tok(system_chars) + chars_to_tok(conv_chars),
                "estimate",
            )
        }
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
    }
}
