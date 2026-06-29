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
