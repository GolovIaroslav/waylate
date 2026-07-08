use keyring::Entry;

const SERVICE: &str = "dev.jar.waylate";

pub fn has(provider: &str) -> bool {
    get(provider)
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

pub fn get(provider: &str) -> Result<String, String> {
    Entry::new(SERVICE, provider)
        .map_err(|err| format!("Could not open keyring entry: {err}"))?
        .get_password()
        .map_err(|err| format!("No API key in Secret Service for {provider}: {err}"))
}

pub fn set(provider: &str, value: &str) -> Result<(), String> {
    // An empty value means "clear the key". Writing "" into the Secret Service leaves a
    // dangling empty entry (and can block later set() on KWallet), so delete instead.
    if value.is_empty() {
        return delete(provider);
    }
    Entry::new(SERVICE, provider)
        .map_err(|err| format!("Could not open keyring entry: {err}"))?
        .set_password(value)
        .map_err(|err| format!("Could not save API key in Secret Service: {err}"))
}

pub fn delete(provider: &str) -> Result<(), String> {
    match Entry::new(SERVICE, provider)
        .map_err(|err| format!("Could not open keyring entry: {err}"))?
        .delete_credential()
    {
        Ok(()) => Ok(()),
        // Clearing an empty field is a no-op, not an error.
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(format!("Could not delete API key: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    // Point keyring at an in-memory store once for the whole test binary, so set/get/delete
    // operate on a fake store instead of the user's real Secret Service / KWallet.
    fn use_mock_store() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            keyring_core::set_default_store(
                keyring_core::mock::Store::new().expect("create mock keyring store"),
            );
        });
    }

    #[test]
    fn set_then_get_round_trips() {
        use_mock_store();
        let provider = "round-trip-provider";
        set(provider, "sk-secret-123").expect("set should succeed");
        assert_eq!(get(provider).unwrap(), "sk-secret-123");
        assert!(has(provider));
    }

    #[test]
    fn missing_key_reports_absent() {
        use_mock_store();
        assert!(!has("never-set-provider"));
        assert!(get("never-set-provider").is_err());
    }

    #[test]
    fn empty_value_clears_the_key() {
        use_mock_store();
        let provider = "clearable-provider";
        set(provider, "to-be-cleared").expect("set should succeed");
        assert!(has(provider));
        // Writing an empty value must delete the entry, not store an empty string.
        set(provider, "").expect("clearing should succeed");
        assert!(!has(provider));
    }

    #[test]
    fn deleting_absent_key_is_ok() {
        use_mock_store();
        // Clearing a key that was never set is a no-op, not an error.
        assert!(delete("absent-provider").is_ok());
    }
}
