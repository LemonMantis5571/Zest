//! Secret storage for API-key providers.
//!
//! The provider config contains only a stable credential reference. Secret
//! values are kept in the platform credential manager and are never serialized
//! into provider views or configuration files.

const SERVICE: &str = "zest";

pub fn get(account: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

pub fn set(account: &str, secret: &str) -> Result<(), String> {
    if secret.trim().is_empty() {
        return Err("API key cannot be empty".into());
    }
    keyring::Entry::new(SERVICE, account)
        .map_err(|e| e.to_string())?
        .set_password(secret)
        .map_err(|e| e.to_string())?;

    // Read it back through a *fresh* entry, which is how every later lookup
    // reaches it. Without a backend feature, `keyring` falls back to a mock
    // store that is per-Entry and in-memory: the write above returns Ok and the
    // secret is simply gone. That shipped once — keys looked saved and every
    // provider then reported itself unconfigured — so the write is not trusted
    // until a separate read confirms it.
    match get(account) {
        Ok(Some(stored)) if stored == secret => Ok(()),
        Ok(_) => Err(
            "the key did not persist — this build has no OS credential store \
             (keyring needs a platform backend feature)"
                .into(),
        ),
        Err(err) => Err(format!("the key could not be read back: {err}")),
    }
}

pub fn delete(account: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

pub fn present(account: &str) -> Result<bool, String> {
    Ok(get(account)?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_is_not_provider_secret() {
        assert_eq!(SERVICE, "zest");
    }

    /// A real credential store is compiled in.
    ///
    /// This is the regression that let API keys vanish: `keyring` with no
    /// backend feature uses an in-memory mock, so a round trip through two
    /// separate `Entry` values loses the secret. Touches the real OS store,
    /// under its own account name, and removes itself.
    #[test]
    fn a_secret_survives_a_round_trip_through_a_new_entry() {
        let account = format!("__zest_selftest_{}", std::process::id());
        let secret = "round-trip-canary";

        if let Err(err) = set(&account, secret) {
            // A headless Linux box has no secret service; that is an absent
            // store rather than a wrong one, so do not fail the suite on it.
            if cfg!(target_os = "linux") {
                eprintln!("no usable credential store here: {err}");
                return;
            }
            panic!("could not store a secret: {err}");
        }

        let read_back = get(&account).expect("read back");
        let _ = delete(&account);
        assert_eq!(read_back.as_deref(), Some(secret));
        assert_eq!(get(&account).expect("after delete"), None);
    }

    #[test]
    fn an_empty_secret_is_refused_before_it_reaches_the_store() {
        assert!(set("__zest_selftest_empty", "   ").is_err());
    }
}
