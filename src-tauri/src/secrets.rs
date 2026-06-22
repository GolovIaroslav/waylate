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
    Entry::new(SERVICE, provider)
        .map_err(|err| format!("Could not open keyring entry: {err}"))?
        .set_password(value)
        .map_err(|err| format!("Could not save API key in Secret Service: {err}"))
}

pub fn delete(provider: &str) -> Result<(), String> {
    Entry::new(SERVICE, provider)
        .map_err(|err| format!("Could not open keyring entry: {err}"))?
        .delete_credential()
        .map_err(|err| format!("Could not delete API key: {err}"))
}
