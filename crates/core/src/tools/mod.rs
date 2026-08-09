pub mod approval;
pub mod bash;
pub mod edit_file;
pub mod external_agent;
pub mod glob_files;
pub mod grep;
pub mod list_dir;
pub mod outcome;
pub mod prepared;
pub mod project;
pub mod question;
pub mod read_file;
pub mod read_skill;
pub mod sensitive;
pub mod walk;
pub mod web_search;
pub mod write_file;

use std::path::Path;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde_json::Value;

use crate::anthropic::types::{Message, ToolDef};
use crate::skills::SkillSet;

use self::approval::ToolRisk;
use self::bash::Bash;
use self::edit_file::EditFile;
use self::glob_files::GlobFiles;
use self::grep::Grep;
use self::list_dir::ListDir;
use self::prepared::PreparedToolCall;
use self::read_file::ReadFile;
use self::read_skill::ReadSkill;
use self::web_search::WebSearch;
use self::write_file::WriteFile;

pub use self::outcome::{ToolMetadata, ToolOutcome};
pub use self::question::{
    parse_question_input, AskUser, DenyQuestioner, QuestionRequest, Questioner, ASK_USER_TOOL,
};

/// A client-side tool.
///
/// `run` / `execute_prepared` return `Result<ToolOutcome, String>` rather than a
/// harness error type on purpose: a tool failing is a normal conversational
/// event, not a harness failure. The `Err` string goes back to the model as a
/// `tool_result` with `is_error: true` so it can adapt, rather than aborting
/// the turn. Optional [`ToolMetadata`] rides beside the body for UI/persistence
/// and is never injected into the Messages API wire as structured content.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;

    /// Defaults to read — safe tools do not need an approval prompt.
    fn risk(&self) -> ToolRisk {
        ToolRisk::Read
    }

    /// Refresh model-visible conversation context before a tool batch runs.
    /// Most tools are stateless; orchestration tools can project this into a
    /// bounded handoff for a worker.
    fn update_context(&self, _messages: &[Message]) {}

    fn uses_context(&self) -> bool {
        false
    }

    /// Build a prepared call once before optional approval + execution.
    ///
    /// Write tools snapshot path, pre-image, and diff here so approval and
    /// execution share one coherent plan.
    fn prepare(&self, input: Value) -> Result<PreparedToolCall, String> {
        Ok(PreparedToolCall::plain(self.name(), self.risk(), input))
    }

    /// Execute a previously prepared call (after approval when required).
    async fn execute_prepared(
        &self,
        prepared: PreparedToolCall,
    ) -> std::result::Result<ToolOutcome, String> {
        match prepared.plain_input() {
            Some(input) => self.run(input.clone()).await,
            None => Err(format!(
                "tool `{}` cannot execute this prepared call",
                prepared.tool_name
            )),
        }
    }

    async fn run(&self, input: Value) -> std::result::Result<ToolOutcome, String>;
}

/// Cloning shares the tools themselves — cheap, and how a delegated worker gets
/// its own registry without rebuilding anything.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.push(tool);
    }

    /// Stable order — the tool list renders at the very front of the prompt, so
    /// reordering it invalidates the entire prompt cache.
    pub fn definitions(&self) -> Vec<ToolDef> {
        self.tools
            .iter()
            .map(|t| ToolDef {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.input_schema(),
                // Set by the provider, which is the only layer that knows
                // whether the endpoint understands caching.
                cache_control: None,
            })
            .collect()
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.iter().map(|t| t.name()).collect()
    }

    pub fn risk(&self, name: &str) -> Option<ToolRisk> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.risk())
    }

    pub fn update_context(&self, messages: &[Message]) {
        for tool in &self.tools {
            if tool.uses_context() {
                tool.update_context(messages);
            }
        }
    }

    pub fn uses_context(&self) -> bool {
        self.tools.iter().any(|tool| tool.uses_context())
    }

    /// Whether any of the tools about to run actually consumes conversation
    /// context.
    ///
    /// Distinct from [`Self::uses_context`], which answers "is such a tool
    /// registered at all". Preparing the context means cloning and redacting the
    /// whole conversation, so doing it because delegation is *configured* — as
    /// opposed to because it is *about to happen* — pays that cost on every tool
    /// round of every turn, in sessions that may never delegate once.
    pub fn round_uses_context(&self, called: &[&str]) -> bool {
        self.tools
            .iter()
            .any(|tool| tool.uses_context() && called.contains(&tool.name()))
    }

    pub fn prepare(&self, name: &str, input: Value) -> Result<PreparedToolCall, String> {
        match self.tools.iter().find(|t| t.name() == name) {
            Some(tool) => tool.prepare(input),
            None => Err(format!("unknown tool: {name}")),
        }
    }

    pub async fn execute_prepared(
        &self,
        prepared: PreparedToolCall,
    ) -> std::result::Result<ToolOutcome, String> {
        let name = prepared.tool_name.clone();
        match self.tools.iter().find(|t| t.name() == name) {
            Some(tool) => tool.execute_prepared(prepared).await,
            None => Err(format!("unknown tool: {name}")),
        }
    }

    pub async fn run(&self, name: &str, input: Value) -> std::result::Result<ToolOutcome, String> {
        let prepared = self.prepare(name, input)?;
        self.execute_prepared(prepared).await
    }
}

/// Register the project-scoped read-only tools (`read_file`, `list_dir`, `glob`,
/// `grep`) plus network `web_search`. Order is stable so prompt-cache prefixes
/// stay warm.
pub fn register_read_tools(
    registry: &mut ToolRegistry,
    root: impl AsRef<Path>,
) -> std::io::Result<()> {
    let root = root.as_ref();
    registry.register(Arc::new(ReadFile::new(root)?));
    registry.register(Arc::new(ListDir::new(root)?));
    registry.register(Arc::new(GlobFiles::new(root)?));
    registry.register(Arc::new(Grep::new(root)?));
    registry.register(Arc::new(WebSearch::new()));
    Ok(())
}

/// Register `read_skill` against a shared skill registry (hot-reloadable).
pub fn register_skill_tools(registry: &mut ToolRegistry, skills: Arc<RwLock<SkillSet>>) {
    registry.register(Arc::new(ReadSkill::new(skills)));
}

/// Register the provider-independent tool that pauses for a user's answer.
/// Workers deliberately do not receive it; only the parent desktop turn owns a
/// human interaction surface.
pub fn register_question_tool(registry: &mut ToolRegistry) {
    question::register_question_tool(registry);
}

/// Register project-scoped write tools (`write_file`, `edit_file`). Requires an
/// [`Approver`] on the agent — without one, gated calls are denied.
///
/// `edit_file` goes last so adding it shifts the cached prompt prefix exactly
/// once rather than displacing every tool after it.
pub fn register_write_tools(
    registry: &mut ToolRegistry,
    root: impl AsRef<Path>,
) -> std::io::Result<()> {
    let root = root.as_ref();
    registry.register(Arc::new(WriteFile::new(root)?));
    registry.register(Arc::new(EditFile::new(root)?));
    Ok(())
}

/// Register `bash`, scoped to `root`.
///
/// Separate from the write tools because it is separately configurable and
/// separately refusable: a front-end with no [`Approver`] should not offer it
/// at all, since every non-allowlisted command would be auto-denied.
pub fn register_exec_tools(
    registry: &mut ToolRegistry,
    root: impl AsRef<Path>,
    settings: self::bash::BashSettings,
) -> std::io::Result<()> {
    registry.register(Arc::new(Bash::new(root)?.with_settings(settings)));
    Ok(())
}

#[cfg(test)]
mod characterization {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("zest-tools-char-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_tools_default_to_read_risk() {
        let dir = scratch("read-risk");
        let mut reg = ToolRegistry::new();
        register_read_tools(&mut reg, &dir).unwrap();
        for name in ["read_file", "list_dir", "glob", "grep", "web_search"] {
            assert_eq!(reg.risk(name), Some(ToolRisk::Read), "{name}");
            assert!(!reg.risk(name).unwrap().requires_approval(), "{name}");
        }
    }

    #[test]
    fn write_tools_register_in_cache_stable_order() {
        let dir = scratch("write-order");
        let mut reg = ToolRegistry::new();
        register_read_tools(&mut reg, &dir).unwrap();
        register_write_tools(&mut reg, &dir).unwrap();
        assert_eq!(
            reg.names(),
            vec![
                "read_file",
                "list_dir",
                "glob",
                "grep",
                "web_search",
                "write_file",
                "edit_file",
            ]
        );
    }

    #[test]
    fn edit_tool_prepare_reuses_the_write_path() {
        let dir = scratch("edit-prep");
        std::fs::write(dir.join("f.txt"), "before\n").unwrap();
        let mut reg = ToolRegistry::new();
        register_write_tools(&mut reg, &dir).unwrap();
        assert_eq!(reg.risk("edit_file"), Some(ToolRisk::Write));
        let prepared = reg
            .prepare(
                "edit_file",
                serde_json::json!({
                    "path": "f.txt",
                    "old_string": "before",
                    "new_string": "after"
                }),
            )
            .unwrap();
        // Dispatch must come back to edit_file, not to write_file, even though
        // the prepared kind is shared.
        assert_eq!(prepared.tool_name, "edit_file");
        assert_eq!(prepared.risk, ToolRisk::Write);
        assert!(prepared.preview.diff.contains("+after"));
    }

    #[test]
    fn write_tool_prepare_carries_write_risk_and_preview() {
        let dir = scratch("write-prep");
        let mut reg = ToolRegistry::new();
        register_write_tools(&mut reg, &dir).unwrap();
        assert_eq!(reg.risk("write_file"), Some(ToolRisk::Write));
        let prepared = reg
            .prepare(
                "write_file",
                serde_json::json!({ "path": "f.txt", "content": "x" }),
            )
            .unwrap();
        assert_eq!(prepared.risk, ToolRisk::Write);
        assert!(prepared.risk.requires_approval());
        assert_eq!(prepared.preview.path, "f.txt");
        assert!(!prepared.preview.summary.is_empty());
    }

    #[test]
    fn unknown_tool_risk_is_none_and_prepare_errors() {
        let reg = ToolRegistry::new();
        assert_eq!(reg.risk("missing"), None);
        let err = reg.prepare("missing", serde_json::json!({})).unwrap_err();
        assert!(err.contains("unknown tool"), "{err}");
    }
}
