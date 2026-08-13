//! Durable, desktop-local project grouping.
//!
//! Spaces are a view concern. They never change the project root used by the
//! agent, the location of `.zest/`, or the isolation policy for external
//! workers. Keeping this state separate from `known-workspaces.json` also
//! means an older Zest build can still read the project list unchanged.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use zest_core::fsutil::atomic_write_json;

pub const DEFAULT_SPACE_ID: &str = "space:default";
pub const DEFAULT_SPACE_NAME: &str = "Default";
const MAX_SPACE_NAME_CHARS: usize = 60;
const MAX_EMOJI_CHARS: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpaceRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpaceState {
    #[serde(default)]
    pub spaces: Vec<SpaceRecord>,
    /// Canonical display path -> space id. Default membership is sparse: a
    /// missing key means the project belongs to Default.
    #[serde(default)]
    pub memberships: HashMap<String, String>,
    #[serde(default = "default_space_id")]
    pub active_space_id: String,
    /// Last project opened while each Space was active.
    #[serde(default)]
    pub last_workspace_by_space_id: HashMap<String, String>,
}

fn default_space_id() -> String {
    DEFAULT_SPACE_ID.to_string()
}

impl Default for SpaceState {
    fn default() -> Self {
        Self {
            spaces: vec![SpaceRecord {
                id: DEFAULT_SPACE_ID.to_string(),
                name: DEFAULT_SPACE_NAME.to_string(),
                emoji: None,
            }],
            memberships: HashMap::new(),
            active_space_id: DEFAULT_SPACE_ID.to_string(),
            last_workspace_by_space_id: HashMap::new(),
        }
    }
}

impl SpaceState {
    pub fn load(path: &Path) -> Self {
        let loaded = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default();
        loaded.normalized()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        atomic_write_json(path, &self.clone().normalized()).map_err(|error| error.to_string())
    }

    pub fn normalized(mut self) -> Self {
        let mut seen_ids = HashSet::new();
        let mut spaces = Vec::with_capacity(self.spaces.len() + 1);

        for mut space in self.spaces.drain(..) {
            space.id = space.id.trim().to_string();
            if space.id.is_empty() || !seen_ids.insert(space.id.clone()) {
                continue;
            }
            space.name = normalize_name(&space.name, DEFAULT_SPACE_NAME);
            space.emoji = normalize_emoji(space.emoji);
            spaces.push(space);
        }

        if let Some(default) = spaces.iter_mut().find(|space| space.id == DEFAULT_SPACE_ID) {
            default.name = normalize_name(&default.name, DEFAULT_SPACE_NAME);
        } else {
            spaces.insert(
                0,
                SpaceRecord {
                    id: DEFAULT_SPACE_ID.to_string(),
                    name: DEFAULT_SPACE_NAME.to_string(),
                    emoji: None,
                },
            );
        }

        let valid_ids: HashSet<&str> = spaces.iter().map(|space| space.id.as_str()).collect();
        self.memberships.retain(|path, id| {
            !path.trim().is_empty() && valid_ids.contains(id.as_str()) && id != DEFAULT_SPACE_ID
        });
        self.last_workspace_by_space_id
            .retain(|id, path| valid_ids.contains(id.as_str()) && !path.trim().is_empty());
        if !valid_ids.contains(self.active_space_id.as_str()) {
            self.active_space_id = DEFAULT_SPACE_ID.to_string();
        }
        self.spaces = spaces;
        self
    }

    pub fn space(&self, id: &str) -> Option<&SpaceRecord> {
        self.spaces.iter().find(|space| space.id == id)
    }

    pub fn space_mut(&mut self, id: &str) -> Option<&mut SpaceRecord> {
        self.spaces.iter_mut().find(|space| space.id == id)
    }

    pub fn space_for_project<'a>(&'a self, project_key: &str) -> &'a str {
        self.memberships
            .get(project_key)
            .filter(|id| self.space(id).is_some())
            .map(String::as_str)
            .unwrap_or(DEFAULT_SPACE_ID)
    }

    pub fn create_space(
        &mut self,
        id: String,
        name: &str,
        emoji: Option<String>,
    ) -> Result<(), String> {
        let name = normalize_name(name, "");
        if name.is_empty() {
            return Err("Space name cannot be empty.".to_string());
        }
        if self
            .spaces
            .iter()
            .any(|space| space.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!("A Space named \"{name}\" already exists."));
        }
        self.spaces.push(SpaceRecord {
            id,
            name,
            emoji: normalize_emoji(emoji),
        });
        Ok(())
    }

    pub fn update_space(
        &mut self,
        id: &str,
        name: &str,
        emoji: Option<String>,
    ) -> Result<(), String> {
        let name = normalize_name(name, "");
        if name.is_empty() {
            return Err("Space name cannot be empty.".to_string());
        }
        if self
            .spaces
            .iter()
            .any(|space| space.id != id && space.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!("A Space named \"{name}\" already exists."));
        }
        let Some(space) = self.space_mut(id) else {
            return Err("That Space no longer exists.".to_string());
        };
        space.name = name;
        space.emoji = normalize_emoji(emoji);
        Ok(())
    }

    pub fn set_project_space(&mut self, project_key: &str, space_id: &str) -> Result<(), String> {
        if self.space(space_id).is_none() {
            return Err("That Space no longer exists.".to_string());
        }
        if space_id == DEFAULT_SPACE_ID {
            self.memberships.remove(project_key);
        } else {
            self.memberships
                .insert(project_key.to_string(), space_id.to_string());
        }

        // A moved project should not be restored into the old Space the next
        // time it is selected. The destination gets it only when it is the
        // currently active Space; moving is not the same as opening it.
        self.last_workspace_by_space_id
            .retain(|_, path| path != project_key);
        Ok(())
    }

    pub fn remember_active_workspace(&mut self, project_key: &str) {
        let active = self.active_space_id.clone();
        if self.space_for_project(project_key) == active {
            self.last_workspace_by_space_id
                .insert(active, project_key.to_string());
        }
    }

    pub fn forget_project(&mut self, project_key: &str) {
        self.memberships.remove(project_key);
        self.last_workspace_by_space_id
            .retain(|_, path| path != project_key);
    }

    pub fn delete_space(&mut self, id: &str) -> Result<(), String> {
        if id == DEFAULT_SPACE_ID {
            return Err("The Default Space cannot be deleted.".to_string());
        }
        if self.space(id).is_none() {
            return Err("That Space no longer exists.".to_string());
        }
        self.spaces.retain(|space| space.id != id);
        self.memberships.retain(|_, space_id| space_id != id);
        self.last_workspace_by_space_id.remove(id);
        if self.active_space_id == id {
            self.active_space_id = DEFAULT_SPACE_ID.to_string();
        }
        Ok(())
    }
}

fn normalize_name(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    trimmed.chars().take(MAX_SPACE_NAME_CHARS).collect()
}

fn normalize_emoji(value: Option<String>) -> Option<String> {
    let value = value?.trim().to_string();
    if value.is_empty() {
        return None;
    }
    Some(value.chars().take(MAX_EMOJI_CHARS).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_materializes_default_and_repairs_orphans() {
        let state = SpaceState {
            spaces: vec![SpaceRecord {
                id: "space:custom".to_string(),
                name: "  Work  ".to_string(),
                emoji: Some("🚀".to_string()),
            }],
            memberships: HashMap::from([
                ("known".to_string(), "space:custom".to_string()),
                ("orphan".to_string(), "space:missing".to_string()),
            ]),
            active_space_id: "space:missing".to_string(),
            last_workspace_by_space_id: HashMap::from([(
                "space:missing".to_string(),
                "orphan".to_string(),
            )]),
        }
        .normalized();

        assert_eq!(state.spaces[0].id, DEFAULT_SPACE_ID);
        assert_eq!(state.active_space_id, DEFAULT_SPACE_ID);
        assert_eq!(state.memberships.len(), 1);
        assert!(state.last_workspace_by_space_id.is_empty());
    }

    #[test]
    fn default_membership_is_sparse() {
        let mut state = SpaceState::default();
        state
            .create_space("space:work".to_string(), "Work", None)
            .unwrap();
        state.set_project_space("C:/repo", "space:work").unwrap();
        assert_eq!(state.space_for_project("C:/repo"), "space:work");
        state
            .set_project_space("C:/repo", DEFAULT_SPACE_ID)
            .unwrap();
        assert_eq!(state.space_for_project("C:/repo"), DEFAULT_SPACE_ID);
        assert!(!state.memberships.contains_key("C:/repo"));
    }

    #[test]
    fn forgetting_a_project_removes_membership_and_last_workspace() {
        let mut state = SpaceState::default();
        state
            .create_space("space:work".to_string(), "Work", None)
            .unwrap();
        state.set_project_space("C:/repo", "space:work").unwrap();
        state.active_space_id = "space:work".to_string();
        state.remember_active_workspace("C:/repo");

        state.forget_project("C:/repo");

        assert_eq!(state.space_for_project("C:/repo"), DEFAULT_SPACE_ID);
        assert!(state.last_workspace_by_space_id.is_empty());
    }

    #[test]
    fn default_cannot_be_deleted() {
        let mut state = SpaceState::default();
        assert!(state.delete_space(DEFAULT_SPACE_ID).is_err());
    }
}
