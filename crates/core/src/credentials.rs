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
        .map_err(|e| e.to_string())
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
    #[test]
    fn service_name_is_not_provider_secret() {
        assert_eq!(super::SERVICE, "zest");
    }
}
