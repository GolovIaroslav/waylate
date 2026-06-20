mod autostart;
mod clipboard;
mod config;
mod history;
mod models;
mod runtime;
mod secrets;
mod translation;

use chrono::Utc;
use config::{AppConfig, AppPaths};
use models::{ModelProfile, ProviderKind};
use runtime::{RuntimeManager, RuntimeReport};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use translation::{TranslationRequest, TranslationResponse};

struct AppState {
    paths: AppPaths,
    pending: Mutex<Option<PendingRequest>>,
    download: Mutex<DownloadControl>,
    runtime: Arc<RuntimeManager>,
}

#[derive(Debug, Default)]
struct DownloadControl {
    active_model: Option<String>,
    cancel_requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingRequest {
    mode: String,
    source_text: String,
    notice: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    config: AppConfig,
    catalog: Vec<ModelProfile>,
    history: Vec<history::HistoryEntry>,
    environment: EnvironmentReport,
    runtime: RuntimeReport,
    has_deepl_key: bool,
    has_google_key: bool,
    has_yandex_key: bool,
    has_local_key: bool,
    paths: PathReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentReport {
    desktop: String,
    session_type: String,
    has_wl_clipboard: bool,
    has_python: bool,
    has_nvidia_smi: bool,
    has_rocm_smi: bool,
    has_llama_server: bool,
    ct2_cuda_devices: u32,
    llama_cuda_reported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathReport {
    config_dir: String,
    data_dir: String,
    models_dir: String,
    history_db: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    model_id: String,
    status: String,
    message: String,
    progress: f64,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
struct HfFile {
    path: String,
    size: Option<u64>,
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
        config: config.clone(),
        catalog: models::catalog(),
        history: entries,
        environment: environment_report(&state.paths, &config),
        runtime: state.runtime.report(&state.paths, &config),
        has_deepl_key: secrets::has("deepl"),
        has_google_key: secrets::has("google"),
        has_yandex_key: secrets::has("yandex"),
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
    autostart::sync(&state.paths, next.autostart)?;
    state.runtime.cleanup_idle(&next);
    if next.local_model_policy == "fast" {
        state.runtime.maybe_preload(&state.paths, &next);
    }
    Ok(next)
}

#[tauri::command]
fn translate_text(
    state: State<'_, AppState>,
    request: TranslationRequest,
) -> Result<TranslationResponse, String> {
    let config = config::load(&state.paths)?;
    let response = translation::translate(&state.paths, &state.runtime, &config, &request)?;
    state
        .runtime
        .apply_post_translate_policy(&config, &request.model_id);
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
fn cancel_model_download(state: State<'_, AppState>) -> Result<(), String> {
    let mut download = state.download.lock().map_err(|_| "Download state is poisoned")?;
    download.cancel_requested = true;
    Ok(())
}

#[tauri::command]
async fn download_catalog_model(app: AppHandle, state: State<'_, AppState>, model_id: String) -> Result<String, String> {
    let profile = models::catalog()
        .into_iter()
        .find(|item| item.id == model_id)
        .ok_or_else(|| "Unknown model profile".to_string())?;
    let repo = profile
        .hf_repo
        .clone()
        .ok_or_else(|| "This model profile is not downloadable from the built-in catalog".to_string())?;

    {
        let mut download = state.download.lock().map_err(|_| "Download state is poisoned")?;
        if download.active_model.is_some() {
            return Err("Another model download is already running.".into());
        }
        download.active_model = Some(profile.id.clone());
        download.cancel_requested = false;
    }

    let paths = state.paths.clone();
    let app_for_download = app.clone();
    let runtime = state.runtime.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        download_huggingface_repo(&app_for_download, &paths, &runtime, &profile, &repo)
    })
    .await
    .map_err(|err| err.to_string())?;

    {
        let mut download = state.download.lock().map_err(|_| "Download state is poisoned")?;
        download.active_model = None;
        download.cancel_requested = false;
    }

    result
}

fn download_huggingface_repo(
    app: &AppHandle,
    paths: &AppPaths,
    runtime: &Arc<RuntimeManager>,
    profile: &ModelProfile,
    repo: &str,
) -> Result<String, String> {
    let target = paths.models_dir.join(&profile.id);
    std::fs::create_dir_all(&target).map_err(|err| err.to_string())?;
    emit_download(
        app,
        &profile.id,
        "starting",
        "Checking files",
        0.02,
        0,
        None,
    );

    let client = reqwest::blocking::Client::new();
    let files = huggingface_files(&client, repo)?;
    if files.is_empty() {
        return Err("Hugging Face repository has no downloadable files.".into());
    }
    let total = files.iter().filter_map(|file| file.size).sum::<u64>();
    let total = if total > 0 { Some(total) } else { None };
    let mut downloaded = existing_downloaded_bytes(&target, &files);

    emit_download(
        app,
        &profile.id,
        "downloading",
        "Downloading",
        download_ratio(downloaded, total),
        downloaded,
        total,
    );

    for file in files {
        if is_download_cancelled(app, &profile.id)? {
            emit_download(app, &profile.id, "cancelled", "Download cancelled", 0.0, downloaded, total);
            return Err("Download cancelled.".into());
        }

        let local_path = target.join(&file.path);
        if let Some(size) = file.size {
            if local_path.exists()
                && local_path.metadata().map(|metadata| metadata.len()).unwrap_or_default() == size
            {
                continue;
            }
        }
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }

        let url = format!("https://huggingface.co/{repo}/resolve/main/{}", file.path);
        let mut response = client
            .get(url)
            .send()
            .map_err(|err| format!("Could not download {}: {err}", file.path))?
            .error_for_status()
            .map_err(|err| format!("Hugging Face returned an error for {}: {err}", file.path))?;
        let mut output = File::create(&local_path)
            .map_err(|err| format!("Could not create {}: {err}", local_path.display()))?;
        let mut buffer = [0_u8; 128 * 1024];
        let mut file_downloaded = 0_u64;
        loop {
            if is_download_cancelled(app, &profile.id)? {
                emit_download(app, &profile.id, "cancelled", "Download cancelled", 0.0, downloaded, total);
                return Err("Download cancelled.".into());
            }
            let read = response
                .read(&mut buffer)
                .map_err(|err| format!("Could not read {}: {err}", file.path))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|err| format!("Could not write {}: {err}", local_path.display()))?;
            file_downloaded += read as u64;
            downloaded += read as u64;
            emit_download(
                app,
                &profile.id,
                "downloading",
                &file.path,
                download_ratio(downloaded, total),
                downloaded,
                total,
            );
        }
        if let Some(expected) = file.size {
            if file_downloaded != expected {
                return Err(format!(
                    "Downloaded {} bytes for {}, expected {}.",
                    file_downloaded, file.path, expected
                ));
            }
        }
    }

    let mut config = config::load(paths)?;
    config.model_id = profile.id.clone();
    match profile.provider {
        ProviderKind::CTranslate2 => {
            emit_download(
                app,
                &profile.id,
                "preparing",
                "Preparing translator",
                0.98,
                downloaded,
                total,
            );
            let helper = runtime::ensure_ct2_runtime(paths)?;
            config.ct2_model_path = target.display().to_string();
            config.ct2_tokenizer_path = target.display().to_string();
            config.source_lang = "auto".into();
            config.target_lang = "eng_Latn".into();
            config.ct2_helper_command = helper;
            if config.ct2_device.trim().is_empty() {
                config.ct2_device = "auto".into();
            }
        }
        ProviderKind::OpenAiCompatible => {
            if let Some(endpoint) = &profile.default_endpoint {
                config.openai_endpoint = endpoint.clone();
            }
            config.openai_model = profile.name.clone();
            config.custom_model_path = target.display().to_string();
        }
        _ => {}
    }
    config::save(paths, &config)?;
    if config.local_model_policy == "fast" && profile.provider == ProviderKind::CTranslate2 {
        emit_download(
            app,
            &profile.id,
            "preparing",
            "Loading model into memory",
            0.99,
            downloaded,
            total,
        );
        let _ = runtime.ensure_ct2_server(paths, &config, &profile.id);
    }

    emit_download(app, &profile.id, "done", "Ready", 1.0, downloaded, total);
    Ok(target.display().to_string())
}

fn huggingface_files(client: &reqwest::blocking::Client, repo: &str) -> Result<Vec<HfFile>, String> {
    let url = format!("https://huggingface.co/api/models/{repo}");
    let value: Value = client
        .get(url)
        .send()
        .map_err(|err| format!("Could not reach Hugging Face: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Hugging Face returned an error: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse Hugging Face model metadata: {err}"))?;
    let siblings = value
        .get("siblings")
        .and_then(Value::as_array)
        .ok_or_else(|| "Hugging Face model metadata did not include files.".to_string())?;
    Ok(siblings
        .iter()
        .filter_map(|item| {
            let path = item.get("rfilename")?.as_str()?;
            if path == ".gitattributes" {
                return None;
            }
            Some(HfFile {
                path: path.to_string(),
                size: item.get("size").and_then(Value::as_u64),
            })
        })
        .collect())
}

fn existing_downloaded_bytes(target: &Path, files: &[HfFile]) -> u64 {
    files
        .iter()
        .filter_map(|file| {
            let expected = file.size?;
            let actual = target.join(&file.path).metadata().ok()?.len();
            (actual == expected).then_some(actual)
        })
        .sum()
}

fn download_ratio(downloaded: u64, total: Option<u64>) -> f64 {
    match total {
        Some(total) if total > 0 => (downloaded as f64 / total as f64).clamp(0.02, 0.99),
        _ => 0.15,
    }
}

fn is_download_cancelled(app: &AppHandle, model_id: &str) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let download = state.download.lock().map_err(|_| "Download state is poisoned")?;
    Ok(download.active_model.as_deref() == Some(model_id) && download.cancel_requested)
}

fn emit_download(
    app: &AppHandle,
    model_id: &str,
    status: &str,
    message: &str,
    progress: f64,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) {
    let _ = app.emit(
        "model-download-progress",
        DownloadProgress {
            model_id: model_id.into(),
            status: status.into(),
            message: message.into(),
            progress,
            downloaded_bytes,
            total_bytes,
        },
    );
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
            "quit" => {
                let state = app.state::<AppState>();
                let _ = state.runtime.shutdown_all();
                app.exit(0);
            }
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

fn environment_report(paths: &AppPaths, config: &AppConfig) -> EnvironmentReport {
    let runtime_report = RuntimeManager::new().report(paths, config);
    EnvironmentReport {
        desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        session_type: std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
        has_wl_clipboard: has_command("wl-paste") && has_command("wl-copy"),
        has_python: has_command("python3"),
        has_nvidia_smi: has_command("nvidia-smi"),
        has_rocm_smi: has_command("rocm-smi"),
        has_llama_server: runtime_report.llama_binary_found,
        ct2_cuda_devices: runtime_report.ct2_cuda_devices,
        llama_cuda_reported: runtime_report.llama_cuda_reported,
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

fn spawn_runtime_housekeeper(paths: AppPaths, runtime: Arc<RuntimeManager>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(5));
        if let Ok(config) = config::load(&paths) {
            runtime.cleanup_idle(&config);
            runtime.maybe_preload(&paths, &config);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            handle_args(app, &args);
        }))
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let paths = AppPaths::new().map_err(|err| Box::<dyn std::error::Error>::from(err))?;
            paths.ensure().map_err(|err| Box::<dyn std::error::Error>::from(err))?;
            history::init(&paths.history_db).map_err(|err| Box::<dyn std::error::Error>::from(err))?;
            let runtime = Arc::new(RuntimeManager::new());
            if let Ok(config) = config::load(&paths) {
                let _ = autostart::sync(&paths, config.autostart);
                if config.local_model_policy == "fast" {
                    runtime.maybe_preload(&paths, &config);
                }
            }
            spawn_runtime_housekeeper(paths.clone(), runtime.clone());
            app.manage(AppState {
                paths,
                pending: Mutex::new(None),
                download: Mutex::new(DownloadControl::default()),
                runtime,
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
            download_catalog_model,
            cancel_model_download
        ])
        .run(tauri::generate_context!())
        .expect("error while running Waylate");
}
