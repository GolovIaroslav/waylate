use crate::config::AppPaths;
use std::fs;

const AUTOSTART_FILE: &str = "dev.jar.waylate.desktop";

pub fn sync(paths: &AppPaths, enabled: bool) -> Result<(), String> {
    let base_config = paths
        .config_dir
        .parent()
        .ok_or_else(|| "Could not resolve XDG config home".to_string())?;
    let autostart_dir = base_config.join("autostart");
    let autostart_file = autostart_dir.join(AUTOSTART_FILE);

    if !enabled {
        if autostart_file.exists() {
            fs::remove_file(&autostart_file)
                .map_err(|err| format!("Could not remove {}: {err}", autostart_file.display()))?;
        }
        return Ok(());
    }

    fs::create_dir_all(&autostart_dir)
        .map_err(|err| format!("Could not create {}: {err}", autostart_dir.display()))?;
    // Desktop sessions (systemd-managed Plasma/GNOME logins especially) run autostart .desktop
    // entries with a minimal PATH that usually excludes ~/.local/bin, so a bare "Exec=waylate"
    // silently fails to launch at login. Use the absolute path to the running binary instead.
    let exe_path = std::env::current_exe()
        .map_err(|err| format!("Could not resolve the running executable path: {err}"))?;
    fs::write(
        &autostart_file,
        format!(
            "[Desktop Entry]\nType=Application\nName=Waylate\nComment=Start Waylate tray app\nExec=\"{}\"\nIcon=dev.jar.waylate\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exe_path.display()
        ),
    )
    .map_err(|err| format!("Could not write {}: {err}", autostart_file.display()))
}
