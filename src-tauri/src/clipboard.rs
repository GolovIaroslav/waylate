use std::process::{Command, Stdio};

pub fn read_primary_selection() -> Result<String, String> {
    run_text_command("wl-paste", &["--primary", "--no-newline"])
}

pub fn read_clipboard() -> Result<String, String> {
    run_text_command("wl-paste", &["--no-newline"])
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

fn run_text_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("Could not start {program}: {err}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
