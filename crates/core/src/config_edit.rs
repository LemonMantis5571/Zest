//! Small, comment-preserving edits to the user/project `zest.toml`.

use std::path::Path;

use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::config::{Config, ExternalAgentMode, ExternalWorkspace};
use crate::fsutil::atomic_write;

#[derive(Debug, Clone)]
pub struct OpenAiProviderInput {
    pub id: String,
    pub base_url: String,
    pub model: String,
    pub models: Vec<String>,
    pub credential: String,
}

#[derive(Debug, Clone)]
pub struct ExternalAgentInput {
    pub id: String,
    pub mode: ExternalAgentMode,
    pub command: String,
    pub args: Vec<String>,
    pub model: Option<String>,
    pub workspace: ExternalWorkspace,
    pub timeout_secs: u64,
}

/// Presets for CLIs that already own their authentication session. These are
/// configuration templates only: no login command or credential is run by the
/// setup UI.
pub fn external_agent_preset(id: &str) -> Option<ExternalAgentInput> {
    match id {
        "claude" => Some(ExternalAgentInput {
            id: id.to_string(),
            mode: ExternalAgentMode::Headless,
            command: "claude".into(),
            args: vec![
                "--print".into(),
                "--verbose".into(),
                "--permission-mode".into(),
                "acceptEdits".into(),
                "--output-format".into(),
                "stream-json".into(),
                "--strict-mcp-config".into(),
                "{prompt}".into(),
            ],
            model: None,
            workspace: ExternalWorkspace::Isolated,
            timeout_secs: 900,
        }),
        "gemini" => Some(ExternalAgentInput {
            id: id.to_string(),
            mode: ExternalAgentMode::Acp,
            command: "gemini".into(),
            args: vec!["--acp".into()],
            model: None,
            workspace: ExternalWorkspace::Isolated,
            timeout_secs: 900,
        }),
        _ => None,
    }
}

pub fn add_openai_provider(path: &Path, input: &OpenAiProviderInput) -> Result<(), String> {
    let id = input.id.trim();
    let base_url = input.base_url.trim().trim_end_matches('/');
    let model = input.model.trim();
    let credential = input.credential.trim();

    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("provider id may contain only letters, numbers, `_`, and `-`".into());
    }
    if model.is_empty() {
        return Err("a default model is required".into());
    }
    if credential.is_empty() {
        return Err("a credential name is required".into());
    }
    let url =
        reqwest::Url::parse(base_url).map_err(|_| "endpoint must be a valid URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("endpoint must be an http(s) URL with a host".into());
    }

    let original = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let mut doc: DocumentMut = original
        .parse()
        .map_err(|e| format!("cannot parse existing config: {e}"))?;
    if !doc.contains_key("providers") {
        doc["providers"] = Item::Table(Table::new());
    }
    let providers = doc["providers"]
        .as_table_mut()
        .ok_or_else(|| "[providers] is not a table".to_string())?;
    let entry = providers.entry(id).or_insert(Item::Table(Table::new()));
    let provider = entry
        .as_table_mut()
        .ok_or_else(|| format!("provider `{id}` is not a table"))?;
    if let Some(kind) = provider.get("kind").and_then(Item::as_str) {
        if kind != "openai_compatible" {
            return Err(format!("provider `{id}` already has kind `{kind}`"));
        }
    }
    provider["kind"] = toml_edit::value("openai_compatible");
    provider["base_url"] = toml_edit::value(base_url);
    provider["model"] = toml_edit::value(model);
    provider["credential"] = toml_edit::value(credential);

    let mut models = Array::new();
    for value in input
        .models
        .iter()
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
    {
        models.push(Value::from(value));
    }
    if models.is_empty() {
        provider.remove("models");
    } else {
        provider["models"] = toml_edit::value(models);
    }

    let rendered = doc.to_string();
    Config::parse(&rendered).map_err(|e| e.to_string())?;
    atomic_write(path, rendered.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

pub fn upsert_external_agent(path: &Path, input: &ExternalAgentInput) -> Result<(), String> {
    let id = input.id.trim();
    let command = input.command.trim();

    validate_id(id, "agent")?;
    if command.is_empty() {
        return Err("agent command is required".into());
    }
    if input.timeout_secs == 0 || input.timeout_secs > 3_600 {
        return Err("agent timeout must be between 1 and 3600 seconds".into());
    }
    if input.mode == ExternalAgentMode::Acp && input.args.iter().any(|arg| arg.contains("{prompt}"))
    {
        return Err("ACP agents receive their prompt over stdio, not in the arguments".into());
    }

    let original = read_config(path)?;
    let mut doc: DocumentMut = original
        .parse()
        .map_err(|e| format!("cannot parse existing config: {e}"))?;
    if !doc.contains_key("agents") {
        doc["agents"] = Item::Table(Table::new());
    }
    let agents = doc["agents"]
        .as_table_mut()
        .ok_or_else(|| "[agents] is not a table".to_string())?;
    let entry = agents.entry(id).or_insert(Item::Table(Table::new()));
    let agent = entry
        .as_table_mut()
        .ok_or_else(|| format!("agent `{id}` is not a table"))?;

    agent["mode"] = toml_edit::value(match input.mode {
        ExternalAgentMode::Headless => "headless",
        ExternalAgentMode::Acp => "acp",
    });
    agent["command"] = toml_edit::value(command);

    let mut args = Array::new();
    for arg in &input.args {
        args.push(Value::from(arg.as_str()));
    }
    if args.is_empty() {
        agent.remove("args");
    } else {
        agent["args"] = toml_edit::value(args);
    }

    if let Some(model) = input
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        agent["model"] = toml_edit::value(model);
    } else {
        agent.remove("model");
    }
    agent["workspace"] = toml_edit::value(match input.workspace {
        ExternalWorkspace::Isolated => "isolated",
        ExternalWorkspace::Current => "current",
    });
    agent["timeout_secs"] = toml_edit::value(input.timeout_secs as i64);

    let rendered = doc.to_string();
    Config::parse(&rendered).map_err(|e| e.to_string())?;
    atomic_write(path, rendered.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

pub fn remove_external_agent(path: &Path, id: &str) -> Result<(), String> {
    let id = id.trim();
    validate_id(id, "agent")?;

    let original = read_config(path)?;
    let mut doc: DocumentMut = original
        .parse()
        .map_err(|e| format!("cannot parse existing config: {e}"))?;
    if let Some(agents) = doc.get_mut("agents").and_then(Item::as_table_mut) {
        agents.remove(id);
    }

    let rendered = doc.to_string();
    Config::parse(&rendered).map_err(|e| e.to_string())?;
    atomic_write(path, rendered.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn validate_id(id: &str, noun: &str) -> Result<(), String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "{noun} id may contain only letters, numbers, `_`, and `-`"
        ));
    }
    Ok(())
}

fn read_config(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_provider_without_discarding_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zest.toml");
        std::fs::write(
            &path,
            "# keep me\n[providers.anthropic]\nkind = \"anthropic\"\napi_key_env = \"KEY\"\n",
        )
        .unwrap();
        add_openai_provider(
            &path,
            &OpenAiProviderInput {
                id: "deepseek".into(),
                base_url: "https://api.deepseek.com/".into(),
                model: "deepseek-v4-flash".into(),
                models: vec!["deepseek-v4-flash".into(), "deepseek-v4-pro".into()],
                credential: "deepseek".into(),
            },
        )
        .unwrap();
        let raw = std::fs::read_to_string(path).unwrap();
        assert!(raw.contains("# keep me"));
        let config = Config::parse(&raw).unwrap();
        assert!(config.providers.contains_key("deepseek"));
    }

    #[test]
    fn preset_agent_preserves_comments_and_can_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zest.toml");
        std::fs::write(&path, "# keep me\n[default]\nprovider = \"codex\"\n").unwrap();

        upsert_external_agent(&path, &external_agent_preset("claude").unwrap()).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("# keep me"));
        assert!(raw.contains("[agents.claude]"));
        assert!(raw.contains("--verbose"));
        assert!(raw.contains("acceptEdits"));
        assert!(raw.contains("--strict-mcp-config"));
        let config = Config::parse(&raw).unwrap();
        assert_eq!(config.agents["claude"].mode, ExternalAgentMode::Headless);

        remove_external_agent(&path, "claude").unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("# keep me"));
        assert!(!raw.contains("[agents.claude]"));
        Config::parse(&raw).unwrap();
    }

    #[test]
    fn rejects_prompt_placeholder_for_acp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zest.toml");
        let mut input = external_agent_preset("gemini").unwrap();
        input.args.push("{prompt}".into());
        let error = upsert_external_agent(&path, &input).unwrap_err();
        assert!(error.contains("over stdio"));
    }
}
