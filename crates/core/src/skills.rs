//! Cursor-style project / user skills (`SKILL.md` with YAML frontmatter).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Max skills kept after discovery (project overrides user on same name).
pub const MAX_SKILLS: usize = 32;
/// Per-skill file size cap (checked before allocating the body).
pub const MAX_SKILL_BYTES: usize = 64 * 1024;
/// Bodies at or under this size are candidates for inlining.
pub const INLINE_MAX_BYTES: usize = 4096;
/// Cumulative budget for all inlined skill bodies in the system prompt.
pub const INLINE_BUDGET_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillSource {
    User,
    Project,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
    pub source: SkillSource,
    pub path: PathBuf,
}

impl Skill {
    pub fn inlined(&self) -> bool {
        self.body.len() <= INLINE_MAX_BYTES
    }
}

/// Settings / IPC summary (no full body).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub source: SkillSource,
    pub path: String,
    pub inlined: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SkillSet {
    by_name: BTreeMap<String, Skill>,
    pub warnings: Vec<String>,
}

impl SkillSet {
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.by_name.get(name)
    }

    /// Look a skill up as a slash command.
    ///
    /// Case-insensitive, because the user is typing this rather than the model
    /// emitting it. Names are unique by construction (`by_name` is a map, and
    /// project skills replace user ones on a clash).
    pub fn command(&self, typed: &str) -> Option<&Skill> {
        let typed = typed.trim();
        self.by_name.get(typed).or_else(|| {
            self.by_name
                .values()
                .find(|s| s.name.eq_ignore_ascii_case(typed))
        })
    }

    /// Names + descriptions for the composer's command panel.
    pub fn command_names(&self) -> Vec<(String, String)> {
        self.by_name
            .values()
            .map(|s| (s.name.clone(), s.description.clone()))
            .collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Skill> {
        self.by_name.values()
    }

    /// Insert or replace by name (tests and controlled assembly).
    pub fn insert(&mut self, skill: Skill) {
        self.by_name.insert(skill.name.clone(), skill);
    }

    pub fn summaries(&self) -> Vec<SkillSummary> {
        self.by_name
            .values()
            .map(|s| SkillSummary {
                name: s.name.clone(),
                description: s.description.clone(),
                source: s.source,
                path: s.path.display().to_string(),
                inlined: s.inlined(),
            })
            .collect()
    }

    /// Discover skills: user home first, then project (project wins on name clash).
    pub fn discover(project_root: &Path) -> Self {
        // Two roots per scope, and the order matters because a later scan
        // overwrites an earlier one by name.
        //
        // `.agents/skills` is where the wider ecosystem installs these — it is
        // what `skills.sh` writes and what Claude Code reads. Zest looked only
        // at its own directory, so a skill installed the ordinary way sat on
        // disk, recorded in `skills-lock.json`, and was invisible here.
        //
        // It is scanned *before* `.zest/skills` in each scope, so a skill a user
        // deliberately put in Zest's own directory still wins over one that
        // arrived with a package.
        let mut set = SkillSet::default();
        if let Some(home) = dirs::home_dir() {
            set.scan_dir(&home.join(".agents").join("skills"), SkillSource::User);
            set.scan_dir(&home.join(".zest").join("skills"), SkillSource::User);
        }
        set.scan_dir(
            &project_root.join(".agents").join("skills"),
            SkillSource::Project,
        );
        set.scan_dir(
            &project_root.join(".zest").join("skills"),
            SkillSource::Project,
        );
        if set.by_name.len() > MAX_SKILLS {
            let excess: Vec<_> = set.by_name.keys().skip(MAX_SKILLS).cloned().collect();
            for name in excess {
                set.by_name.remove(&name);
            }
            set.warnings.push(format!(
                "skill limit ({MAX_SKILLS}) reached; extra skills ignored"
            ));
        }
        set
    }

    fn scan_dir(&mut self, dir: &Path, source: SkillSource) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                self.warnings
                    .push(format!("skills dir {}: read failed: {e}", dir.display()));
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            match parse_skill_file(&skill_md, source) {
                Ok(skill) => {
                    self.by_name.insert(skill.name.clone(), skill);
                }
                Err(msg) => self.warnings.push(msg),
            }
        }
    }

    /// Bullet catalogue for the system prompt.
    pub fn catalogue_markdown(&self) -> String {
        if self.by_name.is_empty() {
            return String::new();
        }
        let mut lines = Vec::new();
        for skill in self.by_name.values() {
            lines.push(format!("- `{}`: {}", skill.name, skill.description));
        }
        lines.join("\n")
    }

    /// Full bodies for skills under the per-skill and cumulative inline budgets.
    pub fn inline_markdown(&self) -> String {
        let mut parts = Vec::new();
        let mut used = 0usize;
        for skill in self.by_name.values() {
            if !skill.inlined() {
                continue;
            }
            let body = skill.body.trim();
            let piece_len = skill
                .name
                .len()
                .saturating_add(body.len())
                .saturating_add(8);
            if used.saturating_add(piece_len) > INLINE_BUDGET_BYTES {
                continue;
            }
            used = used.saturating_add(piece_len);
            parts.push(format!("## {}\n\n{body}", skill.name));
        }
        parts.join("\n\n")
    }
}

/// Minimal YAML frontmatter: `---` … `---` with `name:` / `description:` lines.
pub fn parse_skill_markdown(raw: &str, path: &Path, source: SkillSource) -> Result<Skill, String> {
    let raw = raw.trim_start_matches('\u{feff}');
    let Some(rest) = raw.strip_prefix("---") else {
        return Err(format!(
            "skill {}: missing YAML frontmatter opener",
            path.display()
        ));
    };
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))
        .unwrap_or(rest);
    let Some(end) = rest.find("\n---") else {
        return Err(format!(
            "skill {}: missing YAML frontmatter closer",
            path.display()
        ));
    };
    let front = &rest[..end];
    let body = rest[end + 4..]
        .strip_prefix('\n')
        .or_else(|| rest[end + 4..].strip_prefix("\r\n"))
        .unwrap_or(&rest[end + 4..])
        .to_string();

    let mut name = None;
    let mut description = None;
    for line in front.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(unquote(v.trim()));
        } else if let Some(v) = line.strip_prefix("description:") {
            description = Some(unquote(v.trim()));
        }
    }

    let name = name
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("skill {}: frontmatter missing `name`", path.display()))?;
    let description = description.filter(|s| !s.is_empty()).ok_or_else(|| {
        format!(
            "skill {}: frontmatter missing `description`",
            path.display()
        )
    })?;

    Ok(Skill {
        name,
        description,
        body,
        source,
        path: path.to_path_buf(),
    })
}

fn parse_skill_file(path: &Path, source: SkillSource) -> Result<Skill, String> {
    let meta =
        fs::metadata(path).map_err(|e| format!("skill {}: stat failed: {e}", path.display()))?;
    let len = meta.len() as usize;
    if len > MAX_SKILL_BYTES {
        return Err(format!(
            "skill {}: {len} bytes exceeds max {MAX_SKILL_BYTES}",
            path.display()
        ));
    }
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("skill {}: read failed: {e}", path.display()))?;
    parse_skill_markdown(&raw, path, source)
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zest-skills-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_frontmatter_and_body() {
        let skill = parse_skill_markdown(
            "---\nname: demo\ndescription: Does a thing\n---\n\n# Hello\n",
            Path::new("/tmp/demo/SKILL.md"),
            SkillSource::Project,
        )
        .unwrap();
        assert_eq!(skill.name, "demo");
        assert_eq!(skill.description, "Does a thing");
        assert!(skill.body.contains("# Hello"));
        assert!(skill.inlined());
    }

    /// The reported bug:  and Claude Code install into
    /// , and Zest read only its own directory — so an
    /// installed skill existed on disk, was recorded in ,
    /// and never appeared.
    #[test]
    fn skills_installed_the_ecosystem_way_are_discovered() {
        let root = scratch("agents-dir");
        let agents = root
            .join("proj")
            .join(".agents")
            .join("skills")
            .join("ai-seo");
        fs::create_dir_all(&agents).unwrap();
        write_skill(&agents.join("SKILL.md"), "ai-seo", "from skills.sh", "body");

        let set = SkillSet::discover(&root.join("proj"));
        let skill = set.get("ai-seo").expect("installed skill must be visible");
        assert_eq!(skill.description, "from skills.sh");
    }

    /// Both roots are read, and a skill deliberately placed in Zest's own
    /// directory outranks one that merely arrived with a package.
    #[test]
    fn a_zest_skill_wins_over_a_packaged_one_of_the_same_name() {
        let root = scratch("agents-precedence");
        let proj = root.join("proj");
        let agents = proj.join(".agents").join("skills").join("shared");
        let zest = proj.join(".zest").join("skills").join("shared");
        fs::create_dir_all(&agents).unwrap();
        fs::create_dir_all(&zest).unwrap();
        write_skill(
            &agents.join("SKILL.md"),
            "shared",
            "packaged",
            "packaged body",
        );
        write_skill(&zest.join("SKILL.md"), "shared", "hand placed", "zest body");

        let set = SkillSet::discover(&proj);
        assert_eq!(set.get("shared").unwrap().description, "hand placed");
    }

    #[test]
    fn project_overrides_user_on_same_name() {
        let root = scratch("override");
        let user_skills = root
            .join("home")
            .join(".zest")
            .join("skills")
            .join("shared");
        let proj_skills = root
            .join("proj")
            .join(".zest")
            .join("skills")
            .join("shared");
        fs::create_dir_all(&user_skills).unwrap();
        fs::create_dir_all(&proj_skills).unwrap();
        write_skill(
            &user_skills.join("SKILL.md"),
            "shared",
            "from user",
            "user body",
        );
        write_skill(
            &proj_skills.join("SKILL.md"),
            "shared",
            "from project",
            "project body",
        );

        // Discover with a fake home by scanning manually.
        let mut set = SkillSet::default();
        set.scan_dir(
            &root.join("home").join(".zest").join("skills"),
            SkillSource::User,
        );
        set.scan_dir(
            &root.join("proj").join(".zest").join("skills"),
            SkillSource::Project,
        );
        let skill = set.get("shared").unwrap();
        assert_eq!(skill.description, "from project");
        assert_eq!(skill.source, SkillSource::Project);
    }

    fn write_skill(path: &Path, name: &str, desc: &str, body: &str) {
        let mut f = fs::File::create(path).unwrap();
        writeln!(f, "---\nname: {name}\ndescription: {desc}\n---\n\n{body}").unwrap();
    }
}
