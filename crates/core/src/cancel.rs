//! Cooperative cancellation for agent turns.
//!
//! Desktop arms a flag when the user cancels; the agent checks between provider
//! calls and tool executions. Mid-stream HTTP abort is a later refinement.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared cancel signal for one in-flight turn.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
