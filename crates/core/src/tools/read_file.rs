use std::path::Path;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::approval::{ApprovalPreview, ToolRisk};
use super::prepared::PreparedToolCall;
use super::project::ProjectRoot;
use super::sensitive::is_sensitive_path;
use super::Tool;

const MAX_BYTES: usize = 64 * 1024;

/// Read a text file, confined to a project root.
///
/// The path in a tool call is model output, not user input — it gets the same
/// treatment as anything else off the wire. Every path is canonicalized and
/// checked against the root before it reaches the filesystem, which closes
/// `..`, absolute paths, and symlinks pointing outside the tree.
///
/// Likely-secret files require per-call approval; discovery tools omit them.
pub struct ReadFile {
    root: ProjectRoot,
}

impl ReadFile {
    pub fn new(root: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            root: ProjectRoot::new(root)?,
        })
    }

    fn prepare_call(&self, input: Value) -> Result<PreparedToolCall, String> {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing required field `path`".to_string())?;

        let resolved = self.root.resolve(path)?;
        let rel = self.root.relativize(&resolved);

        if is_sensitive_path(&rel) {
            return Ok(PreparedToolCall::plain_with_preview(
                "read_file",
                ToolRisk::Sensitive,
                input,
                ApprovalPreview {
                    path: rel.clone(),
                    summary: format!("Read sensitive file {rel}"),
                    diff: String::new(),
                },
            ));
        }

        Ok(PreparedToolCall::plain("read_file", ToolRisk::Read, input))
    }

    async fn read_path(&self, path: &str) -> Result<String, String> {
        let resolved = self.root.resolve(path)?;
        let bytes = tokio::fs::read(&resolved)
            .await
            .map_err(|e| format!("read failed: {e}"))?;

        let truncated = bytes.len() > MAX_BYTES;
        let slice = &bytes[..bytes.len().min(MAX_BYTES)];
        let mut text = String::from_utf8_lossy(slice).into_owned();
        if truncated {
            text.push_str(&format!(
                "\n\n[truncated at {MAX_BYTES} bytes; file is {} bytes]",
                bytes.len()
            ));
        }
        Ok(text)
    }
}

#[async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a UTF-8 text file from the project. Call this whenever answering \
         depends on the actual contents of a file rather than on what its name \
         suggests. Paths are relative to the project root. Likely-secret files \
         (e.g. `.env`, private keys) require user approval."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path relative to the project root, e.g. src/main.rs"
                }
            },
            "required": ["path"],
            "additionalProperties": false
        })
    }

    fn prepare(&self, input: Value) -> Result<PreparedToolCall, String> {
        self.prepare_call(input)
    }

    async fn run(&self, input: Value) -> std::result::Result<String, String> {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing required field `path`".to_string())?;
        self.read_path(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::approval::AllowApprover;
    use crate::tools::ToolRegistry;
    use std::sync::Arc;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("zest-read-file-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn reads_a_file_under_root() {
        let dir = scratch("ok");
        std::fs::write(dir.join("note.txt"), "hello").unwrap();
        let tool = ReadFile::new(&dir).unwrap();
        let out = tool.run(json!({ "path": "note.txt" })).await.unwrap();
        assert_eq!(out, "hello");
    }

    #[tokio::test]
    async fn rejects_missing_path_and_escape() {
        let dir = scratch("bad");
        let tool = ReadFile::new(&dir).unwrap();

        let err = tool.run(json!({})).await.unwrap_err();
        assert!(err.contains("missing required field"), "{err}");

        let err = tool.run(json!({ "path": ".." })).await.unwrap_err();
        assert!(
            err.contains("outside the project root") || err.contains("cannot resolve"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn sensitive_read_requires_approval_risk() {
        let dir = scratch("secret");
        std::fs::write(dir.join(".env"), "SECRET=1\n").unwrap();
        std::fs::write(dir.join(".env.example"), "SECRET=\n").unwrap();
        let tool = ReadFile::new(&dir).unwrap();

        let prepared = tool
            .prepare(json!({ "path": ".env" }))
            .unwrap();
        assert_eq!(prepared.risk, ToolRisk::Sensitive);
        assert!(prepared.risk.requires_approval());

        let example = tool
            .prepare(json!({ "path": ".env.example" }))
            .unwrap();
        assert_eq!(example.risk, ToolRisk::Read);
        assert!(!example.risk.requires_approval());
    }

    #[tokio::test]
    async fn registry_executes_sensitive_after_prepare() {
        let dir = scratch("reg-secret");
        std::fs::write(dir.join(".env"), "SECRET=1\n").unwrap();
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(ReadFile::new(&dir).unwrap()));
        let prepared = reg.prepare("read_file", json!({ "path": ".env" })).unwrap();
        assert_eq!(prepared.risk, ToolRisk::Sensitive);
        let _ = AllowApprover; // documents the approval path
        let out = reg.execute_prepared(prepared).await.unwrap();
        assert!(out.contains("SECRET=1"));
    }
}
