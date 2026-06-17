mod clipboard;
mod config;
mod history;
mod models;
mod secrets;
mod translation;

use chrono::Utc;
use config::{AppConfig, AppPaths};
use models::ModelProfile;
use serde::{Deserialize, Serialize};
use std::{process::Command, sync::Mutex};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder,
};
use translation::{TranslationRequest, TranslationResponse};

struct AppState {
    paths: AppPaths,
    pending: Mutex<Option<PendingRequest>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingRequest {
    mode: String,
    source_text: String,
    notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    config: AppConfig,
    catalog: Vec<ModelProfile>,
    history: Vec<history::HistoryEntry>,
    environment: EnvironmentReport,
    has_deepl_key: bool,
    has_google_key: bool,
    has_local_key: bool,
    paths: PathReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentReport {
    desktop: String,
    session_type: String,
    has_wl_clipboard: bool,
    has_huggingface_cli: bool,
    has_python: bool,
    has_nvidia_smi: bool,
    has_rocm_smi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathReport {
    config_dir: String,
    data_dir: String,
    models_dir: String,
    history_db: String,
}

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let config = config::load(&state.paths)?;
    let entries = if config.history_enabled {
        history::list(&state.paths.history_db, 30)?
    } else {
        Vec::new()
    };
    Ok(AppSnapshot {
        config,
        catalog: models::catalog(),
        history: entries,
        environment: environment_report(),
        has_deepl_key: secrets::has("deepl"),
        has_google_key: secrets::has("google"),
        has_local_key: secrets::has("openai-compatible"),
        paths: PathReport {
            config_dir: state.paths.config_dir.display().to_string(),
            data_dir: state.paths.data_dir.display().to_string(),
            models_dir: state.paths.models_dir.display().to_string(),
            history_db: state.paths.history_db.display().to_string(),
        },
    })
}

#[tauri::command]
fn save_config(state: State<'_, AppState>, next: AppConfig) -> Result<AppConfig, String> {
    config::save(&state.paths, &next)?;
    Ok(next)
}

#[tauri::command]
fn translate_text(
    state: State<'_, AppState>,
    request: TranslationRequest,
) -> Result<TranslationResponse, String> {
    let config = config::load(&state.paths)?;
    let response = translation::translate(&config, &request)?;
    if config.history_enabled {
        history::insert(
            &state.paths.history_db,
            &history::HistoryEntry {
                id: 0,
                created_at: Utc::now(),
                source_lang: request.source_lang,
                target_lang: request.target_lang,
                model_id: request.model_id,
                source_text: request.text,
                translated_text: response.translated_text.clone(),
            },
        )?;
    }
    Ok(response)
}

#[tauri::command]
fn read_selection_text() -> Result<String, String> {
    read_selection_with_fallback().map(|(text, _)| text)
}

#[tauri::command]
fn read_clipboard_text() -> Result<String, String> {
    clipboard::read_clipboard()
}

#[tauri::command]
fn write_clipboard_text(text: String) -> Result<(), String> {
    clipboard::write_clipboard(&text)
}

#[tauri::command]
fn take_pending_request(state: State<'_, AppState>) -> Result<Option<PendingRequest>, String> {
    let mut pending = state.pending.lock().map_err(|_| "Pending state is poisoned")?;
    Ok(pending.take())
}

#[tauri::command]
fn save_api_key(provider: String, key: String) -> Result<(), String> {
    secrets::set(&provider, &key)
}

#[tauri::command]
fn clear_api_key(provider: String) -> Result<(), String> {
    secrets::delete(&provider)
}

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Result<Vec<history::HistoryEntry>, String> {
    history::list(&state.paths.history_db, 50)
}

#[tauri::command]
fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    history::clear(&state.paths.history_db)
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    opener::open(path).map_err(|err| err.to_string())
}

#[tauri::command]
fn download_catalog_model(state: State<'_, AppState>, model_id: String) -> Result<String, String> {
    let profile = models::catalog()
        .into_iter()
        .find(|item| item.id == model_id)
        .ok_or_else(|| "Unknown model profile".to_string())?;
    let repo = profile
        .hf_repo
        .ok_or_else(|| "This model profile is not downloadable from the built-in catalog".to_string())?;
    let target = state.paths.models_dir.join(&profile.id);
    std::fs::create_dir_all(&target).map_err(|err| err.to_string())?;
    let output = Command::new("huggingface-cli")
        .arg("download")
        .arg(&repo)
        .arg("--local-dir")
        .arg(&target)
        .output()
        .map_err(|err| format!("Could not start huggingface-cli: {err}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(target.display().to_string())
}

fn read_selection_with_fallback() -> Result<(String, Option<String>), String> {
    match clipboard::read_primary_selection() {
        Ok(text) if !text.trim().is_empty() => Ok((text, None)),
        _ => match clipboard::read_clipboard() {
            Ok(text) if !text.trim().is_empty() => Ok((
                text,
                Some("Primary selection was empty, so Waylate used the clipboard.".into()),
            )),
            _ => Ok((
                String::new(),
                Some("Waylate could not read selected text. Paste text manually or copy it first.".into()),
            )),
        },
    }
}

fn set_pending(app: &AppHandle, pending: PendingRequest) -> Result<(), String> {
    let state = app.state::<AppState>();
    *state
        .pending
        .lock()
        .map_err(|_| "Pending state is poisoned")? = Some(pending);
    app.emit("waylate-pending", true)
        .map_err(|err| err.to_string())?;
    show_window(app, "Waylate")
}

fn show_window(app: &AppHandle, title: &str) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|err| err.to_string())?;
        window.set_focus().map_err(|err| err.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("/".into()))
        .title(title)
        .inner_size(900.0, 560.0)
        .min_inner_size(680.0, 420.0)
        .resizable(true)
        .visible(true)
        .build()
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn handle_args(app: &AppHandle, args: &[String]) {
    let command = args.iter().skip(1).find(|arg| !arg.starts_with('-')).map(String::as_str);
    match command {
        Some("translate-selection") => {
            let (source_text, notice) = read_selection_with_fallback().unwrap_or_else(|err| {
                (String::new(), Some(format!("Could not read Wayland selection: {err}")))
            });
            let _ = set_pending(
                app,
                PendingRequest {
                    mode: "translate".into(),
                    source_text,
                    notice,
                },
            );
        }
        Some("translate-clipboard") => {
            let source_text = clipboard::read_clipboard().unwrap_or_default();
            let _ = set_pending(
                app,
                PendingRequest {
                    mode: "translate".into(),
                    source_text,
                    notice: None,
                },
            );
        }
        Some("settings") => {
            let _ = set_pending(
                app,
                PendingRequest {
                    mode: "settings".into(),
                    source_text: String::new(),
                    notice: None,
                },
            );
        }
        _ => {}
    }
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let translate = MenuItemBuilder::with_id("translate_clipboard", "Translate clipboard").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&translate, &settings, &quit])
        .build()?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("Missing application icon")?;

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Waylate")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "translate_clipboard" => {
                let source_text = clipboard::read_clipboard().unwrap_or_default();
                let _ = set_pending(
                    app,
                    PendingRequest {
                        mode: "translate".into(),
                        source_text,
                        notice: None,
                    },
                );
            }
            "settings" => {
                let _ = set_pending(
                    app,
                    PendingRequest {
                        mode: "settings".into(),
                        source_text: String::new(),
                        notice: None,
                    },
                );
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let _ = set_pending(
                    app,
                    PendingRequest {
                        mode: "translate".into(),
                        source_text: String::new(),
                        notice: None,
                    },
                );
            }
        })
        .build(app)?;

    Ok(())
}

fn environment_report() -> EnvironmentReport {
    EnvironmentReport {
        desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        session_type: std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
        has_wl_clipboard: has_command("wl-paste") && has_command("wl-copy"),
        has_huggingface_cli: has_command("huggingface-cli"),
        has_python: has_command("python3"),
        has_nvidia_smi: has_command("nvidia-smi"),
        has_rocm_smi: has_command("rocm-smi"),
    }
}

fn has_command(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            handle_args(app, &args);
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let paths = AppPaths::new().map_err(|err| Box::<dyn std::error::Error>::from(err))?;
            paths.ensure().map_err(|err| Box::<dyn std::error::Error>::from(err))?;
            history::init(&paths.history_db).map_err(|err| Box::<dyn std::error::Error>::from(err))?;
            app.manage(AppState {
                paths,
                pending: Mutex::new(None),
            });
            setup_tray(app)?;
            let args: Vec<String> = std::env::args().collect();
            handle_args(app.handle(), &args);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            save_config,
            translate_text,
            read_selection_text,
            read_clipboard_text,
            write_clipboard_text,
            take_pending_request,
            save_api_key,
            clear_api_key,
            get_history,
            clear_history,
            reveal_path,
            download_catalog_model
        ])
        .run(tauri::generate_context!())
        .expect("error while running Waylate");
}
