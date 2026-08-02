//! System prompt composition: base + project custom + skills.

use std::fs;
use std::path::{Path, PathBuf};

use crate::fsutil;
use crate::skills::SkillSet;

/// Default base instructions when a front-end does not supply its own.
pub const DEFAULT_SYSTEM: &str = "\
You are Zest, a coding agent inside the user's project. You have project tools \
(list_dir, glob, grep, read_file, write_file) scoped to that project. Explore and \
read before answering. write_file requires user approval. Keep responses focused.";

/// Max bytes for `.zest/system.md` (checked before allocating the full body).
pub const MAX_CUSTOM_PROMPT_BYTES: usize = 32 * 1024;

pub fn custom_system_path(root: &Path) -> PathBuf {
    root.join(".zest").join("system.md")
}

/// Load custom system prompt. Missing file → empty string. Other I/O / size
/// errors propagate (never silent empty on failure).
pub fn load_custom_system(root: &Path) -> Result<String, String> {
    let path = custom_system_path(root);
    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    let len = meta.len() as usize;
    if len > MAX_CUSTOM_PROMPT_BYTES {
        return Err(format!(
            "{} is {len} bytes; max is {MAX_CUSTOM_PROMPT_BYTES}",
            path.display()
        ));
    }
    fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))
}

pub fn save_custom_system(root: &Path, content: &str) -> Result<(), String> {
    if content.len() > MAX_CUSTOM_PROMPT_BYTES {
        return Err(format!(
            "custom prompt is {} bytes; max is {MAX_CUSTOM_PROMPT_BYTES}",
            content.len()
        ));
    }
    let path = custom_system_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    fsutil::atomic_write(&path, content.as_bytes())
        .map_err(|e| format!("write {}: {e}", path.display()))
}

/// Compose the full system prompt.
///
/// When `custom` is non-empty it is **authoritative** for identity/persona and is
/// placed first so it overrides conflicting lines in the front-end base prompt
/// (e.g. "You are Zest…"). Skills catalogue follows.
pub fn compose_system(base: &str, custom: &str, skills: &SkillSet) -> String {
    let custom = custom.trim();
    let base = base.trim();
    let mut out = String::new();

    if !custom.is_empty() {
        out.push_str("# Project instructions\n\n");
        out.push_str(custom);
        out.push_str(
            "\n\n(The project instructions above override any conflicting persona \
or identity in the operating rules below.)\n\n# Operating rules\n\n",
        );
        out.push_str(&neutralize_fixed_identity(base));
    } else if !base.is_empty() {
        out.push_str(base);
    }

    let catalogue = skills.catalogue_markdown();
    if !catalogue.is_empty() {
        out.push_str("\n\n# Available skills\n\n");
        out.push_str(&catalogue);
        out.push_str(
            "\n\nUse the `read_skill` tool with a skill's `name` to load full \
instructions when a skill is relevant and its details are not already inlined below.",
        );
    }

    let inline = skills.inline_markdown();
    if !inline.is_empty() {
        out.push_str("\n\n# Skill details\n\n");
        out.push_str(&inline);
    }

    out
}

/// Soften a hardcoded "You are Zest…" opener so project custom identity can win.
fn neutralize_fixed_identity(base: &str) -> String {
    let trimmed = base.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("you are zest") {
        // Drop the first sentence; keep tooling / behavior rules.
        if let Some(rest) = trimmed.split_once(". ").map(|(_, r)| r) {
            return format!("You are a coding agent in the user's project. {rest}");
        }
    }
    trimmed.to_string()
}

/// Unicode-safe truncation for composed-prompt previews (char-based, not bytes).
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{truncated}…\n\n(truncated — {count} chars total)")
}

/// Load custom + discover skills and compose against `base`.
pub fn compose_for_project(base: &str, root: &Path) -> Result<(String, SkillSet), String> {
    let custom = load_custom_system(root)?;
    let skills = SkillSet::discover(root);
    let system = compose_system(base, &custom, &skills);
    Ok((system, skills))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{parse_skill_markdown, SkillSource};
    use std::path::Path;

    #[test]
    fn compose_order_base_custom_skills() {
        let mut skills = SkillSet::default();
        let skill = parse_skill_markdown(
            "---\nname: fmt\ndescription: Format code\n---\n\nDo it right.\n",
            Path::new("/x/fmt/SKILL.md"),
            SkillSource::Project,
        )
        .unwrap();
        skills.insert(skill);

        let composed = compose_system("BASE tooling", "CUSTOM LAYER", &skills);
        let custom_at = composed.find("CUSTOM LAYER").unwrap();
        let base_at = composed.find("BASE tooling").unwrap();
        let skills_at = composed.find("Available skills").unwrap();
        assert!(custom_at < base_at, "custom must precede base");
        assert!(base_at < skills_at);
        assert!(composed.contains("`fmt`: Format code"));
        assert!(composed.contains("Do it right."));
    }

    #[test]
    fn custom_identity_overrides_you_are_zest() {
        let skills = SkillSet::default();
        let composed = compose_system(
            "You are Zest, a coding agent. You have tools.",
            "You are jennie of blackpink",
            &skills,
        );
        let jennie = composed.find("You are jennie of blackpink").unwrap();
        let zest = composed.find("You are Zest");
        assert!(zest.is_none(), "fixed Zest identity should be neutralized");
        assert!(composed[jennie..].contains("You are a coding agent"));
        assert!(composed.contains("override"));
    }

    #[test]
    fn truncate_chars_is_multibyte_safe() {
        // Each emoji is one char but multiple UTF-8 bytes.
        let s = "😀😁😂😃😄😅😆😇😈";
        let out = truncate_chars(s, 3);
        assert!(out.starts_with("😀😁😂"));
        assert!(out.contains("truncated"));
        // Must not panic or split a codepoint.
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
    }

    #[test]
    fn load_custom_rejects_oversized() {
        let dir = std::env::temp_dir().join(format!(
            "zest-prompt-big-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".zest")).unwrap();
        let big = "x".repeat(MAX_CUSTOM_PROMPT_BYTES + 1);
        fs::write(dir.join(".zest").join("system.md"), &big).unwrap();
        let err = load_custom_system(&dir).unwrap_err();
        assert!(err.contains("max"), "{err}");
    }
}
