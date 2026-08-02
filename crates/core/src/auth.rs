//! Detecting which providers are already signed in.
//!
//! Zest does **not** implement OAuth. Each vendor CLI already performs its own
//! login and writes credentials to disk; Zest reads whether that happened and
//! nothing more. Implementing three vendor OAuth flows would be the most fragile
//! code in the project, and it would break without notice.
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

use std::path::{Path, PathBuf};

/// What a provider's sign-in looks like from the outside.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
pub struct ProviderSlot {
    /// Stable id used by config, routing rules and the usage ledger.
    pub id: &'static str,
    pub label: &'static str,
    /// How this provider is authenticated, in words the picker can show.
    pub method: &'static str,
    pub status: AuthStatus,
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

/// `$CODEX_HOME/auth.json`, else `~/.codex/auth.json`.
///
/// Same location the Codex CLI writes and that LimeBot's `codex-oauth.mjs`
/// reads — this is a proven path, not a guess.
pub fn detect_codex() -> AuthStatus {
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
}
