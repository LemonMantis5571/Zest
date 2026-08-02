use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

use super::Tool;

const MAX_BYTES: usize = 64 * 1024;

/// Read a text file, confined to a project root.
///
/// The path in a tool call is model output, not user input — it gets the same
/// treatment as anything else off the wire. Every path is canonicalized and
/// checked against the root before it reaches the filesystem, which closes
/// `..`, absolute paths, and symlinks pointing outside the tree.
pub struct ReadFile {
    root: PathBuf,
}

impl ReadFile {
    pub fn new(root: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            root: std::fs::canonicalize(root)?,
        })
    }

    fn resolve(&self, raw: &str) -> std::result::Result<PathBuf, String> {
        let candidate = self.root.join(raw);
        // canonicalize resolves `..` and symlinks; it also requires the file to
        // exist, so a missing file is reported here rather than at open time.
        let resolved = std::fs::canonicalize(&candidate)
            .map_err(|e| format!("cannot resolve `{raw}`: {e}"))?;

        if !resolved.starts_with(&self.root) {
            return Err(format!("`{raw}` resolves outside the project root"));
        }
        Ok(resolved)
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
         suggests. Paths are relative to the project root."
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

    async fn run(&self, input: Value) -> std::result::Result<String, String> {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing required field `path`".to_string())?;

        let resolved = self.resolve(path)?;
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
