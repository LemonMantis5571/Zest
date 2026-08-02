//! Project-scoped sticky session state, keyed by provider.
//!
//! Replaces global `last-model` / `last-effort` / `last-thread-id` scalars with
//! an atomic `.zest/session-state.json` map. Legacy scalars are migrated only
//! when the sticky thread (if any) is owned by the active provider.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fsutil;

const STATE_FILE: &str = "session-state.json";
const LEGACY_THREAD: &str = "last-thread-id";
const LEGACY_MODEL: &str = "last-model";
const LEGACY_EFFORT: &str = "last-effort";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSessionPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSessionState {
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderSessionPrefs>,
}

impl ProjectSessionState {
    pub fn path(root: &Path) -> PathBuf {
        root.join(".zest").join(STATE_FILE)
    }

    /// Load project state, migrating legacy scalars when ownership matches.
    pub fn load(root: &Path, active_provider: &str) -> Self {
        let path = Self::path(root);
        let mut state = match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<ProjectSessionState>(&raw).unwrap_or_default(),
            Err(_) => ProjectSessionState::default(),
        };
        state.migrate_legacy(root, active_provider);
        state
    }

    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        let path = Self::path(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        fsutil::atomic_write_json(&path, self)
    }

    pub fn get(&self, provider_id: &str) -> ProviderSessionPrefs {
        self.providers.get(provider_id).cloned().unwrap_or_default()
    }

    pub fn set_thread(&mut self, provider_id: &str, thread_id: impl Into<String>) {
        self.providers
            .entry(provider_id.to_string())
            .or_default()
            .thread_id = Some(thread_id.into());
    }

    pub fn set_model_effort(
        &mut self,
        provider_id: &str,
        model: impl Into<String>,
        effort: impl Into<String>,
    ) {
        let entry = self.providers.entry(provider_id.to_string()).or_default();
        entry.model = Some(model.into());
        entry.effort = Some(effort.into());
    }

    /// Clear model+effort for a provider (atomic reset from the desktop).
    pub fn clear_model_effort(&mut self, provider_id: &str) {
        if let Some(entry) = self.providers.get_mut(provider_id) {
            entry.model = None;
            entry.effort = None;
        }
    }

    fn migrate_legacy(&mut self, root: &Path, active_provider: &str) {
        if self.providers.contains_key(active_provider) {
            // Already have a slot — still allow filling missing fields from legacy.
        }

        let project = root.join(".zest");
        let legacy_thread =
            read_trimmed(project.join(LEGACY_THREAD)).or_else(|| read_user_legacy(LEGACY_THREAD));
        let legacy_model =
            read_trimmed(project.join(LEGACY_MODEL)).or_else(|| read_user_legacy(LEGACY_MODEL));
        let legacy_effort =
            read_trimmed(project.join(LEGACY_EFFORT)).or_else(|| read_user_legacy(LEGACY_EFFORT));

        if legacy_thread.is_none() && legacy_model.is_none() && legacy_effort.is_none() {
            return;
        }

        // Ownership gate: only migrate when the sticky thread belongs to this
        // provider (or there is no thread id to check).
        let ownership_ok = match &legacy_thread {
            None => true,
            Some(id) => thread_owned_by(root, id, active_provider),
        };
        if !ownership_ok {
            return;
        }

        let entry = self
            .providers
            .entry(active_provider.to_string())
            .or_default();
        if entry.thread_id.is_none() {
            entry.thread_id = legacy_thread;
        }
        if entry.model.is_none() {
            entry.model = legacy_model;
        }
        if entry.effort.is_none() {
            entry.effort = legacy_effort;
        }

        // Persist migrated map; best-effort delete legacy project scalars.
        let _ = self.save(root);
        let _ = std::fs::remove_file(project.join(LEGACY_THREAD));
        let _ = std::fs::remove_file(project.join(LEGACY_MODEL));
        let _ = std::fs::remove_file(project.join(LEGACY_EFFORT));
    }
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    let value = std::fs::read_to_string(path).ok()?;
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn read_user_legacy(name: &str) -> Option<String> {
    let path = dirs::config_dir()?.join("zest").join(name);
    read_trimmed(path)
}

fn thread_owned_by(root: &Path, thread_id: &str, provider_id: &str) -> bool {
    let path = root
        .join(".zest")
        .join("threads")
        .join(format!("{thread_id}.json"));
    let Ok(raw) = std::fs::read_to_string(path) else {
        // Missing thread — allow migration of model/effort alone.
        return true;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    match value.get("providerId").and_then(|v| v.as_str()) {
        Some(id) => id == provider_id,
        // Legacy threads without provider_id: do not claim for a new provider.
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zest-prefs-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".zest").join("threads")).unwrap();
        dir
    }

    #[test]
    fn migrates_legacy_only_when_thread_owned() {
        let root = scratch("own");
        let threads = root.join(".zest").join("threads");
        std::fs::write(
            threads.join("thread-a.json"),
            r#"{"version":1,"id":"thread-a","createdAt":1,"updatedAt":1,"providerId":"codex","wireFormat":"anthropic_messages","messages":[],"agentMessages":[]}"#,
        )
        .unwrap();
        std::fs::write(root.join(".zest").join(LEGACY_THREAD), "thread-a").unwrap();
        std::fs::write(root.join(".zest").join(LEGACY_MODEL), "gpt-5.6-luna").unwrap();

        let state = ProjectSessionState::load(&root, "codex");
        let prefs = state.get("codex");
        assert_eq!(prefs.thread_id.as_deref(), Some("thread-a"));
        assert_eq!(prefs.model.as_deref(), Some("gpt-5.6-luna"));

        // Different provider must not inherit the codex thread — even if legacy
        // scalars somehow remain.
        std::fs::write(root.join(".zest").join(LEGACY_THREAD), "thread-a").unwrap();
        let other = ProjectSessionState::load(&root, "claude");
        assert_ne!(other.get("claude").thread_id.as_deref(), Some("thread-a"));
        assert!(!other.providers.contains_key("claude") || other.get("claude").thread_id.is_none());
    }

    #[test]
    fn save_round_trip_is_atomic_json() {
        let root = scratch("save");
        let mut state = ProjectSessionState::default();
        state.set_thread("codex", "thread-1");
        state.set_model_effort("codex", "gpt-5.6-sol", "high");
        state.save(&root).unwrap();
        let loaded = ProjectSessionState::load(&root, "codex");
        assert_eq!(loaded.get("codex").thread_id.as_deref(), Some("thread-1"));
        assert_eq!(loaded.get("codex").model.as_deref(), Some("gpt-5.6-sol"));
    }
}
