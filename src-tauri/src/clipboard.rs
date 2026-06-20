use std::process::{Command, Stdio};

pub fn read_primary_selection() -> Result<String, String> {
    read_text(true)
}

pub fn read_clipboard() -> Result<String, String> {
    read_text(false)
}

pub fn write_clipboard(text: &str) -> Result<(), String> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Could not start wl-copy: {err}"))?;

    if let Some(stdin) = &mut child.stdin {
        use std::io::Write;
        stdin
            .write_all(text.as_bytes())
            .map_err(|err| format!("Could not send text to wl-copy: {err}"))?;
    }

    let status = child
        .wait()
        .map_err(|err| format!("Could not wait for wl-copy: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("wl-copy failed".into())
    }
}

fn read_text(primary: bool) -> Result<String, String> {
    let types = list_types(primary)?;
    let mime = preferred_text_mime(&types)
        .ok_or_else(|| "Clipboard does not contain text".to_string())?;

    let mut args = Vec::new();
    if primary {
        args.push("--primary");
    }
    args.extend(["--no-newline", "--type", mime]);

    run_text_command("wl-paste", &args)
}

fn list_types(primary: bool) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    if primary {
        args.push("--primary");
    }
    args.push("--list-types");

    let raw = run_text_command("wl-paste", &args)?;
    Ok(raw.lines().map(|line| line.trim().to_string()).collect())
}

fn preferred_text_mime(types: &[String]) -> Option<&str> {
    for preferred in ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "TEXT", "STRING"] {
        if types.iter().any(|mime| mime == preferred) {
            return Some(preferred);
        }
    }
    types
        .iter()
        .find(|mime| mime.starts_with("text/"))
        .map(String::as_str)
}

fn run_text_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("Could not start {program}: {err}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_string())
        .map_err(|_| "Clipboard text is not valid UTF-8".to_string())
}

#[cfg(test)]
mod tests {
    use super::preferred_text_mime;

    #[test]
    fn rejects_image_only_clipboard() {
        let types = vec!["image/png".to_string(), "application/octet-stream".to_string()];
        assert_eq!(preferred_text_mime(&types), None);
    }

    #[test]
    fn prefers_plain_text_clipboard() {
        let types = vec!["image/png".to_string(), "text/plain;charset=utf-8".to_string()];
        assert_eq!(preferred_text_mime(&types), Some("text/plain;charset=utf-8"));
    }
}
