//! Minimal tool approval gate.
//!
//! Read tools run without prompting. Write/exec/sensitive-read tools pause for
//! an [`Approver`] decision before execution. Decisions are session-scoped —
//! nothing is persisted to disk here.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// How dangerous a tool invocation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    Read,
    /// Explicit read of a likely-secret file — requires per-call approval.
    Sensitive,
    Write,
    Exec,
}

impl ToolRisk {
    pub fn requires_approval(self) -> bool {
        matches!(self, Self::Sensitive | Self::Write | Self::Exec)
    }
}

/// What the UI (or CLI) should show before a gated tool runs.
#[derive(Debug, Clone)]
pub struct ApprovalPreview {
    pub path: String,
    pub summary: String,
    pub diff: String,
}

/// Request handed to an [`Approver`].
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub approval_id: String,
    pub tool_name: String,
    pub tool_call_id: String,
    pub risk: ToolRisk,
    pub preview: ApprovalPreview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    AllowOnce,
    Deny,
}

/// Front-end hook: desktop waits on the user; CLI may auto-deny or prompt.
#[async_trait]
pub trait Approver: Send + Sync {
    /// Reserve a wait slot **before** `ApprovalNeeded` is emitted so a fast
    /// UI click cannot race the registration.
    async fn prepare(&self, _approval_id: &str) {}

    async fn decide(&self, request: &ApprovalRequest) -> ApprovalDecision;
}

/// Safe default when no front-end is wired — deny every gated call.
pub struct DenyApprover;

#[async_trait]
impl Approver for DenyApprover {
    async fn decide(&self, _request: &ApprovalRequest) -> ApprovalDecision {
        ApprovalDecision::Deny
    }
}

/// Test helper that allows every gated call.
pub struct AllowApprover;

#[async_trait]
impl Approver for AllowApprover {
    async fn decide(&self, _request: &ApprovalRequest) -> ApprovalDecision {
        ApprovalDecision::AllowOnce
    }
}

#[cfg(test)]
mod characterization {
    use super::*;

    #[test]
    fn tool_risk_approval_defaults() {
        assert!(!ToolRisk::Read.requires_approval());
        assert!(ToolRisk::Sensitive.requires_approval());
        assert!(ToolRisk::Write.requires_approval());
        assert!(ToolRisk::Exec.requires_approval());
    }

    #[test]
    fn tool_risk_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&ToolRisk::Write).unwrap(),
            "\"write\""
        );
        assert_eq!(
            serde_json::from_str::<ToolRisk>("\"sensitive\"").unwrap(),
            ToolRisk::Sensitive
        );
        assert_eq!(
            serde_json::from_str::<ToolRisk>("\"exec\"").unwrap(),
            ToolRisk::Exec
        );
    }

    #[tokio::test]
    async fn deny_approver_is_safe_default() {
        let decision = DenyApprover
            .decide(&ApprovalRequest {
                approval_id: "a1".into(),
                tool_name: "write_file".into(),
                tool_call_id: "t1".into(),
                risk: ToolRisk::Write,
                preview: ApprovalPreview {
                    path: "f.txt".into(),
                    summary: "write f.txt".into(),
                    diff: "".into(),
                },
            })
            .await;
        assert_eq!(decision, ApprovalDecision::Deny);
    }
}
