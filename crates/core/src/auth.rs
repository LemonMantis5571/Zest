//! Detecting which providers are already signed in.
//!
//! Zest does **not** implement OAuth. Each vendor CLI (or local gateway) already
//! performs its own login and writes credentials to disk; Zest reads whether
//! that happened and nothing more. Implementing three vendor OAuth flows would
//! be the most fragile code in the project, and it would break upstream without
//! notice.
//!
//! Two rules this module holds to:
//!
//! 1. **Never read or surface a secret.** Detection checks that a credential
//!    store exists and is well-formed. It does not extract tokens, and no value
//!    from a credential file is ever logged or returned.
//! 2. **Never claim "not logged in" when the real answer is "can't tell".** Some
//!    providers keep credentials somewhere we cannot inspect — an OS keychain, an
//!    encrypted blob. Reporting those as logged-out would push the user to
//!    re-authenticate for no reason, so they get `Unknown` instead.
//!
//! Connecting from the UI is a **native shell** over vendor OAuth: spawn the
//! login process with no console window, let the system browser finish ChatGPT/
//! Claude sign-in, then re-detect. Zest never exchanges tokens itself.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What a provider's sign-in looks like from the outside.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthStatus {
    /// A credential store was found and is well-formed.
    ///
    /// `account` is a display label (an email, a plan name) when the provider
    /// exposes one in a non-secret field, `None` when it does not. It is never a
    /// token or any part of one.
    Ready { account: Option<String> },

    /// The credential store is absent. `fix` is the command that creates it.
    NotLoggedIn { fix: String },

    /// The provider is installed but its credentials are somewhere we cannot
    /// inspect. Offer it and let the request fail with a real error rather than
    /// pre-emptively greying it out.
    Unknown { reason: String },

    /// Bring-your-own-key with no key supplied yet.
    Unconfigured,
}

impl AuthStatus {
    /// Whether the UI should let the user pick this provider.
    ///
    /// `Unknown` counts as selectable on purpose — see the module note.
    pub fn selectable(&self) -> bool {
        matches!(self, AuthStatus::Ready { .. } | AuthStatus::Unknown { .. })
    }
}

/// One row in the launch picker.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderSlot {
    /// Stable id used by config, routing rules and the usage ledger.
    pub id: &'static str,
    pub label: &'static str,
    /// How this provider is authenticated, in words the picker can show.
    pub method: &'static str,
    pub status: AuthStatus,
}

/// Resolved spawn plan for a Connect action. Owned paths so gateway binaries
/// under `tools/` work without being on PATH.
#[derive(Debug, Clone)]
pub struct LoginSpawn {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Short title for the waiting screen ("Sign in with ChatGPT").
    pub browser_title: &'static str,
    /// Body copy while the system browser completes OAuth.
    pub browser_body: &'static str,
}

/// Every provider Zest knows how to look for, in display order.
pub fn detect_all() -> Vec<ProviderSlot> {
    vec![
        ProviderSlot {
            id: "codex",
            label: "Codex",
            method: "ChatGPT sign-in",
            status: detect_codex(),
        },
        ProviderSlot {
            id: "claude",
            label: "Claude",
            method: "Claude sign-in",
            status: detect_claude(),
        },
        ProviderSlot {
            id: "antigravity",
            label: "Antigravity",
            method: "Google sign-in",
            status: detect_antigravity(),
        },
        ProviderSlot {
            id: "byok",
            label: "API key",
            method: "Bring your own key",
            status: detect_byok(),
        },
    ]
}

/// Codex readiness for Zest's default path.
///
/// When a local CLIProxyAPI install is present, Ready means the gateway's own
/// credential store under `~/.cli-proxy-api` exists — that is what live turns
/// spend. Otherwise fall back to the Codex CLI's `auth.json`.
pub fn detect_codex() -> AuthStatus {
    if find_cliproxy().is_some() {
        return if gateway_auth_present() {
            AuthStatus::Ready { account: None }
        } else {
            AuthStatus::NotLoggedIn {
                fix: "Connect in Zest (ChatGPT sign-in)".into(),
            }
        };
    }

    let home = match std::env::var("CODEX_HOME") {
        Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => match home_dir() {
            Some(h) => h.join(".codex"),
            None => {
                return AuthStatus::Unknown {
                    reason: "no home directory".into(),
                }
            }
        },
    };

    match well_formed_json(&home.join("auth.json")) {
        Some(true) => AuthStatus::Ready { account: None },
        Some(false) => AuthStatus::Unknown {
            reason: "auth.json is present but unreadable".into(),
        },
        None => AuthStatus::NotLoggedIn {
            fix: "codex login".into(),
        },
    }
}

/// True when `~/.cli-proxy-api` has at least one well-formed JSON file.
///
/// Presence and parseability only — file contents are never returned or logged.
pub fn gateway_auth_present() -> bool {
    let Some(dir) = home_dir().map(|h| h.join(".cli-proxy-api")) else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if well_formed_json(&path) == Some(true) {
            return true;
        }
    }
    false
}

/// Claude Code keeps credentials outside a plain file on some platforms (an OS
/// keychain on macOS, not a readable file on Windows), so a missing file is not
/// evidence of being logged out.
pub fn detect_claude() -> AuthStatus {
    let Some(dir) = home_dir().map(|h| h.join(".claude")) else {
        return AuthStatus::Unknown {
            reason: "no home directory".into(),
        };
    };

    if !dir.exists() {
        return AuthStatus::NotLoggedIn {
            fix: "claude login".into(),
        };
    }

    // Only trust an explicit credentials file. Its absence means "installed, but
    // credentials live somewhere we can't see" — not "logged out".
    match well_formed_json(&dir.join(".credentials.json")) {
        Some(true) => AuthStatus::Ready { account: None },
        _ => AuthStatus::Unknown {
            reason: "Claude is installed but stores credentials outside a readable file"
                .into(),
        },
    }
}

/// Antigravity keeps a data directory under `~/.gemini/antigravity`. The Gemini
/// CLI writes `~/.gemini/oauth_creds.json`; Antigravity itself does not, so a
/// present data directory alone is not proof of a session.
pub fn detect_antigravity() -> AuthStatus {
    let Some(gemini) = home_dir().map(|h| h.join(".gemini")) else {
        return AuthStatus::Unknown {
            reason: "no home directory".into(),
        };
    };

    if let Some(true) = well_formed_json(&gemini.join("oauth_creds.json")) {
        return AuthStatus::Ready { account: None };
    }

    if gemini.join("antigravity").is_dir() {
        return AuthStatus::Unknown {
            reason: "Antigravity is installed but its session is not in a readable file".into(),
        };
    }

    AuthStatus::NotLoggedIn {
        fix: "sign in to Antigravity".into(),
    }
}

/// A key in the environment. Deliberately checks presence only — the value is
/// never inspected, compared, or reported.
pub fn detect_byok() -> AuthStatus {
    let present = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY"]
        .iter()
        .any(|k| std::env::var(k).map(|v| !v.trim().is_empty()).unwrap_or(false));

    if present {
        AuthStatus::Ready { account: None }
    } else {
        AuthStatus::Unconfigured
    }
}

/// Whether Connect can launch a login for this provider.
pub fn can_start_login(provider_id: &str) -> bool {
    resolve_login(provider_id).is_some()
}

/// Shell command the vendor CLI expects for sign-in, as `"program", ["args"…]`.
///
/// For Codex this is the *fallback* (`codex login`). Prefer [`resolve_login`],
/// which may point at CLIProxyAPI instead.
pub fn login_command(provider_id: &str) -> Option<(&'static str, &'static [&'static str])> {
    match provider_id {
        "codex" => Some(("codex", &["login"])),
        "claude" => Some(("claude", &["login"])),
        "antigravity" | "byok" => None,
        _ => None,
    }
}

/// Resolve what Connect should spawn. Codex prefers a local CLIProxyAPI binary
/// when `tools/CLIProxyAPI` (or `ZEST_CLIPROXY_PATH`) is available.
pub fn resolve_login(provider_id: &str) -> Option<LoginSpawn> {
    match provider_id {
        "codex" => {
            if let Some((exe, config)) = find_cliproxy() {
                return Some(LoginSpawn {
                    program: exe,
                    args: vec![
                        "-config".into(),
                        config.to_string_lossy().into_owned(),
                        "-codex-login".into(),
                    ],
                    browser_title: "Sign in with ChatGPT",
                    browser_body: "Finish in your browser. This window will update when you’re done.",
                });
            }
            Some(LoginSpawn {
                program: PathBuf::from("codex"),
                args: vec!["login".into()],
                browser_title: "Sign in with ChatGPT",
                browser_body: "Finish in your browser. This window will update when you’re done.",
            })
        }
        "claude" => Some(LoginSpawn {
            program: PathBuf::from("claude"),
            args: vec!["login".into()],
            browser_title: "Sign in with Claude",
            browser_body: "Finish in your browser. This window will update when you’re done.",
        }),
        _ => None,
    }
}

/// Spawn the vendor/gateway login flow with no console window. Credentials stay
/// with the vendor — Zest only starts the process and later re-detects whether
/// a store appeared.
pub fn start_login(provider_id: &str) -> std::result::Result<LoginSpawn, String> {
    let spawn = resolve_login(provider_id).ok_or_else(|| match provider_id {
        "antigravity" => {
            "Antigravity has no CLI login Zest can launch — sign in from the Antigravity app"
                .into()
        }
        "byok" => "API key providers are configured via environment variables, not a login".into(),
        other => format!("no login command for provider `{other}`"),
    })?;

    spawn_silent(&spawn.program, &spawn.args).map_err(|e| {
        format!(
            "could not start `{} {}` — is it installed? ({e})",
            spawn.program.display(),
            spawn.args.join(" ")
        )
    })?;

    Ok(spawn)
}

fn spawn_silent(program: &Path, args: &[String]) -> std::io::Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Hide the console entirely on Windows so Connect feels like Zest, not a
    // terminal handoff. The system browser still opens for OAuth.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.spawn()?;
    Ok(())
}

/// Locate CLIProxyAPI: `ZEST_CLIPROXY_PATH`, then walk up from cwd for
/// `tools/CLIProxyAPI/cli-proxy-api[.exe]` next to `config.yaml`.
fn find_cliproxy() -> Option<(PathBuf, PathBuf)> {
    if let Ok(raw) = std::env::var("ZEST_CLIPROXY_PATH") {
        let exe = PathBuf::from(raw.trim());
        if exe.is_file() {
            let config = exe.parent()?.join("config.yaml");
            if config.is_file() {
                return Some((exe, config));
            }
        }
    }

    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..8 {
        let base = dir.join("tools").join("CLIProxyAPI");
        let exe = base.join(cliproxy_bin_name());
        let config = base.join("config.yaml");
        if exe.is_file() && config.is_file() {
            return Some((exe, config));
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn cliproxy_bin_name() -> &'static str {
    if cfg!(windows) {
        "cli-proxy-api.exe"
    } else {
        "cli-proxy-api"
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// `Some(true)` = present and parses as JSON, `Some(false)` = present but not,
/// `None` = absent.
///
/// The parsed value is dropped immediately. Nothing inside a credential file is
/// read out, and the file's contents never leave this function.
fn well_formed_json(path: &Path) -> Option<bool> {
    if !path.is_file() {
        return None;
    }
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Some(false);
    };
    Some(serde_json::from_str::<serde_json::Value>(&raw).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_is_selectable_but_logged_out_is_not() {
        // The whole point: "can't tell" must not be rendered as "logged out".
        assert!(AuthStatus::Unknown {
            reason: "keychain".into()
        }
        .selectable());
        assert!(AuthStatus::Ready { account: None }.selectable());
        assert!(!AuthStatus::NotLoggedIn {
            fix: "codex login".into()
        }
        .selectable());
        assert!(!AuthStatus::Unconfigured.selectable());
    }

    #[test]
    fn missing_credential_file_is_absent_not_malformed() {
        assert_eq!(well_formed_json(Path::new("./definitely-not-here.json")), None);
    }

    #[test]
    fn detect_all_covers_every_provider_slot() {
        let slots = detect_all();
        let ids: Vec<_> = slots.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["codex", "claude", "antigravity", "byok"]);
    }

    #[test]
    fn login_command_covers_cli_backed_providers_only() {
        assert_eq!(login_command("codex"), Some(("codex", &["login"][..])));
        assert_eq!(login_command("claude"), Some(("claude", &["login"][..])));
        assert_eq!(login_command("antigravity"), None);
        assert_eq!(login_command("byok"), None);
        assert_eq!(login_command("unknown"), None);
    }

    #[test]
    fn resolve_login_covers_cli_backed_providers() {
        assert!(resolve_login("claude").is_some());
        assert!(resolve_login("codex").is_some());
        assert!(resolve_login("antigravity").is_none());
        assert!(resolve_login("byok").is_none());
    }

    #[test]
    fn start_login_rejects_providers_without_a_cli() {
        assert!(start_login("byok").is_err());
        assert!(start_login("antigravity").is_err());
    }

    #[test]
    fn gateway_auth_absent_dir_is_false() {
        // Home may or may not have a gateway store; the helper must not panic.
        let _ = gateway_auth_present();
    }
}
