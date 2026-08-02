//! Streaming Messages API client.
//!
//! Owns the HTTP request and the SSE transport. Rebuilding the assistant turn
//! from the event stream lives in `accumulate.rs` so it can be tested against a
//! recorded transcript rather than only over the network.

use futures_util::StreamExt;
use serde_json::Value;

use super::accumulate::TurnAccumulator;
use super::sse::SseParser;
use super::types::{Request, API_BASE, API_VERSION};
use crate::error::{HarnessError, Result};
use crate::provider::{Completion, RateLimitSnapshot, StreamEvent};

pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicClient {
    pub fn new(api_key: String) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder().build()?,
            api_key,
            base_url: API_BASE.to_string(),
        })
    }

    /// Point at something other than the Anthropic API — a gateway that speaks
    /// the Messages API on behalf of another backend, or a local mock.
    ///
    /// Takes an origin, not a full endpoint: `http://127.0.0.1:8317`, not
    /// `.../v1/messages`.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
    }

    pub async fn stream(
        &self,
        req: &Request,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
    ) -> Result<Completion> {
        let resp = self
            .http
            .post(self.endpoint())
            .header("x-api-key", &self.api_key)
            // Gateways commonly read the bearer header instead. Sending both is
            // harmless — the real API ignores it.
            .header("authorization", format!("Bearer {}", self.api_key))
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(req)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(HarnessError::Api {
                status: status.as_u16(),
                body,
            });
        }

        // Read the headers before touching the body — `bytes_stream()` consumes
        // the response.
        let limits = rate_limits_from_headers(resp.headers());

        let mut accumulator = TurnAccumulator::new();
        let mut parser = SseParser::default();
        let mut body = resp.bytes_stream();

        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            for payload in parser.feed(&chunk) {
                let event: Value = serde_json::from_str(&payload)?;
                accumulator.push(&event, on_event)?;
            }

            if accumulator.is_done() {
                break;
            }
        }

        Ok(accumulator.finish(limits))
    }
}

/// Throughput headroom, if the endpoint reports it.
///
/// Anthropic sends these; a gateway generally does not, and `None` is the honest
/// answer there rather than a fabricated zero.
fn rate_limits_from_headers(headers: &reqwest::header::HeaderMap) -> Option<RateLimitSnapshot> {
    let text = |key: &str| {
        headers
            .get(key)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let number = |key: &str| text(key).and_then(|v| v.parse::<u64>().ok());

    let snapshot = RateLimitSnapshot {
        requests_limit: number("anthropic-ratelimit-requests-limit"),
        requests_remaining: number("anthropic-ratelimit-requests-remaining"),
        requests_reset: text("anthropic-ratelimit-requests-reset"),
        input_tokens_remaining: number("anthropic-ratelimit-input-tokens-remaining"),
        output_tokens_remaining: number("anthropic-ratelimit-output-tokens-remaining"),
        tokens_reset: text("anthropic-ratelimit-tokens-reset"),
        retry_after_secs: number("retry-after"),
    };

    (!snapshot.is_empty()).then_some(snapshot)
}
