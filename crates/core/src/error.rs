use thiserror::Error;

#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("http transport: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    /// Non-2xx from the Messages API. Body is the raw error envelope.
    #[error("api returned {status}: {body}")]
    Api { status: u16, body: String },

    /// An `event: error` frame arrived mid-stream (e.g. `overloaded_error`),
    /// or the stream was malformed.
    #[error("stream {kind}: {message}")]
    Stream { kind: String, message: String },

    /// The turn ended for a reason the caller has to decide about:
    /// `refusal`, `max_tokens`, or an unrecognized stop reason.
    #[error("turn stopped: {0}")]
    StoppedEarly(String),

    /// User (or session controller) cancelled the in-flight turn.
    #[error("turn cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, HarnessError>;
