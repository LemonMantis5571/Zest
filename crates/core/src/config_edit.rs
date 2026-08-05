//! Small, comment-preserving edits to the user/project `zest.toml`.

use std::path::Path;

use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::config::Config;
use crate::fsutil::atomic_write;

#[derive(Debug, Clone)]
pub struct OpenAiProviderInput {
    pub id: String,
    pub base_url: String,
    pub model: String,
    pub models: Vec<String>,
    pub credential: String,
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
}
