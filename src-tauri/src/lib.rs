mod autostart;
mod clipboard;
mod config;
mod engines;
mod gpu_runtime;
mod history;
mod models;
mod runtime;
mod secrets;
mod translation;

use chrono::Utc;
use config::{AppConfig, AppPaths, InstalledModelMetadata};
use models::{InstallState, ModelCatalogEntry, ModelProfile, ProviderKind};
use runtime::{RuntimeManager, RuntimeReport};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
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
    installed_model_ids: Vec<String>,
    model_states: Vec<ModelInstallState>,
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
    llama_cuda_reported: bool,
    total_memory_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathReport {
    config_dir: String,
    data_dir: String,
    models_dir: String,
    history_db: String,
    logs_dir: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationProgress {
    status: String,
    translated_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ModelInstallState {
    model_id: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallManifest {
    version: u8,
    repo: String,
    files: Vec<String>,
}

#[derive(Debug, Clone)]
struct HfFile {
    path: String,
    size: Option<u64>,
}

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let config = config::load(&state.paths)?;
    let mut catalog = models::catalog();
    // For snapshot purposes, we bridge spec catalog models into the legacy ModelProfile list
    // so the existing UI components can still render basic info.
    for spec in models::model_catalog() {
        if !catalog.iter().any(|p| p.id == spec.id) {
            catalog.push(spec.into());
        }
    }
    let model_states = collect_model_states(&state.paths, &config, &catalog);
    let entries = if config.history_enabled {
        history::list(&state.paths.history_db, 30)?
    } else {
        Vec::new()
    };
    Ok(AppSnapshot {
        config: config.clone(),
        installed_model_ids: model_states
            .iter()
            .filter(|state| state.status == "installed")
            .map(|state| state.model_id.clone())
            .collect(),
        model_states,
        catalog,
        history: entries,
        environment: environment_report(&state.paths, &config),
        runtime: state.runtime.report(&config),
        has_deepl_key: secrets::has("deepl"),
        has_google_key: secrets::has("google"),
        has_yandex_key: secrets::has("yandex"),
        has_local_key: secrets::has("openai-compatible"),
        paths: PathReport {
            config_dir: state.paths.config_dir.display().to_string(),
            data_dir: state.paths.data_dir.display().to_string(),
            models_dir: state.paths.models_dir.display().to_string(),
            history_db: state.paths.history_db.display().to_string(),
            logs_dir: state.paths.data_dir.join("logs").display().to_string(),
        },
    })
}

#[tauri::command]
fn list_model_profiles() -> Vec<ModelCatalogEntry> {
    models::model_catalog()
}

#[tauri::command]
fn get_model_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<InstallState, String> {
    let config = config::load(&state.paths)?;
    let profile = models::model_catalog()
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "Unknown model profile".to_string())?;
    let target = state.paths.models_dir.join(&profile.id);
    if profile.files.is_empty() {
        return Ok(InstallState::NotInstalled);
    }
    if has_persisted_spec_install(&config, &target, &profile)
        && has_spec_install_manifest(&target, &profile)
    {
        return Ok(InstallState::Ready);
    }
    // Files are fully present even without a manifest (e.g. download interrupted after write but
    // before manifest was written). Heal the manifest and report ready.
    if all_spec_files_complete(&target, &profile) {
        let _ = write_spec_install_manifest(&target, &profile);
        let _ = persist_spec_install_metadata(&state.paths, &profile, &target);
        return Ok(InstallState::Ready);
    }
    if spec_dir_has_partial_files(&target, &profile) {
        return Ok(InstallState::Failed {
            message: "Previous download did not finish.".into(),
        });
    }
    if profile.engine == models::EngineKind::OpenAiCompatible
        && !config.openai_endpoint.trim().is_empty()
    {
        return Ok(InstallState::Ready);
    }
    Ok(InstallState::NotInstalled)
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
async fn translate_text(
    app: AppHandle,
    state: State<'_, AppState>,
    request: TranslationRequest,
) -> Result<TranslationResponse, String> {
    let config = config::load(&state.paths)?;
    let paths = state.paths.clone();
    let runtime = state.runtime.clone();
    let request_for_worker = request.clone();
    let app_for_worker = app.clone();
    let config_for_worker = config.clone();
    let response = tauri::async_runtime::spawn_blocking(move || {
        let mut sent_any_progress = false;
        let mut emit_partial = |partial: &str| -> Result<(), String> {
            sent_any_progress = true;
            emit_translation_progress(&app_for_worker, "streaming", partial);
            Ok(())
        };
        let response = translation::translate_with_progress(
            &paths,
            &runtime,
            &config_for_worker,
            &request_for_worker,
            &mut emit_partial,
        )?;
        if !sent_any_progress {
            emit_translation_progress(&app_for_worker, "done", &response.translated_text);
        }
        Ok::<TranslationResponse, String>(response)
    })
    .await
    .map_err(|err| err.to_string())??;
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
    let mut pending = state
        .pending
        .lock()
        .map_err(|_| "Pending state is poisoned")?;
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
fn delete_history_entry(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    history::delete(&state.paths.history_db, id)
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    opener::open(path).map_err(|err| err.to_string())
}

#[tauri::command]
fn read_runtime_log(state: State<'_, AppState>, name: String) -> Result<String, String> {
    let file_name = match name.as_str() {
        "llama-server.log" => name,
        _ => return Err("Unknown runtime log.".into()),
    };
    let path = runtime::runtime_log_path(&state.paths, &file_name);
    if !path.exists() {
        return Ok("".into());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
    let mut lines = content.lines().rev().take(120).collect::<Vec<_>>();
    lines.reverse();
    Ok(lines.join("\n"))
}

#[tauri::command]
fn cancel_model_download(state: State<'_, AppState>) -> Result<(), String> {
    let mut download = state
        .download
        .lock()
        .map_err(|_| "Download state is poisoned")?;
    download.cancel_requested = true;
    Ok(())
}

#[tauri::command]
fn cancel_model_install(state: State<'_, AppState>, profile_id: String) -> Result<(), String> {
    let mut download = state
        .download
        .lock()
        .map_err(|_| "Download state is poisoned")?;
    if download.active_model.as_deref() == Some(profile_id.as_str()) {
        download.cancel_requested = true;
    }
    Ok(())
}

#[tauri::command]
async fn install_model(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<String, String> {
    let profile = models::model_catalog()
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "Unknown model profile".to_string())?;
    if !profile.downloadable {
        return Err("This model is not ready for download yet.".into());
    }
    install_spec_model(app, state, profile).await
}

#[tauri::command]
fn uninstall_model(state: State<'_, AppState>, profile_id: String) -> Result<(), String> {
    let profile = models::model_catalog()
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "Unknown model profile".to_string())?;
    let target = state.paths.models_dir.join(&profile.id);
    let _ = state.runtime.shutdown_profile(&profile.id);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|err| format!("Could not delete {}: {err}", target.display()))?;
    }
    clear_installed_model_metadata(&state.paths, &profile.id)?;
    Ok(())
}

#[tauri::command]
async fn download_catalog_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
) -> Result<String, String> {
    let base_config = config::load(&state.paths)?;
    let profile = models::catalog()
        .into_iter()
        .find(|item| item.id == model_id)
        .ok_or_else(|| "Unknown model profile".to_string())?;
    let repo = profile.hf_repo.clone().ok_or_else(|| {
        "This model profile is not downloadable from the built-in catalog".to_string()
    })?;

    {
        let mut download = state
            .download
            .lock()
            .map_err(|_| "Download state is poisoned")?;
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
        download_huggingface_repo(
            &app_for_download,
            &paths,
            &runtime,
            &profile,
            &repo,
            base_config,
        )
    })
    .await
    .map_err(|err| err.to_string())?;

    {
        let mut download = state
            .download
            .lock()
            .map_err(|_| "Download state is poisoned")?;
        download.active_model = None;
        download.cancel_requested = false;
    }

    result
}

/// Synthetic model id the GPU runtime download reports progress under, so the existing
/// `model-download-progress` listener and progress bar can render it without new plumbing.
const GPU_DOWNLOAD_ID: &str = "gpu-runtime";
const VULKAN_DOWNLOAD_ID: &str = "vulkan-runtime";

/// Download the self-contained CUDA onnxruntime bundle and the fp16 weights, flip the GPU
/// flag, then restart so ORT loads the GPU library before any translation call.
#[tauri::command]
async fn enable_gpu_acceleration(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let paths = state.paths.clone();
    let app_for_worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        // The runtime is the bulk (~2.5 GB); give it the first 80% of the bar.
        emit_download(
            &app_for_worker,
            GPU_DOWNLOAD_ID,
            "downloading",
            "Preparing GPU runtime",
            0.0,
            0,
            None,
        );
        gpu_runtime::download_gpu_runtime(&paths, &mut |frac, msg| {
            emit_download(
                &app_for_worker,
                GPU_DOWNLOAD_ID,
                "downloading",
                msg,
                (frac * 0.8).clamp(0.0, 0.8),
                0,
                None,
            );
        })?;
        gpu_runtime::download_fp16_model(&paths, &mut |frac, msg| {
            emit_download(
                &app_for_worker,
                GPU_DOWNLOAD_ID,
                "downloading",
                msg,
                (0.8 + frac * 0.18).clamp(0.8, 0.98),
                0,
                None,
            );
        })?;

        let mut config = config::load(&paths)?;
        config.gpu_enabled = true;
        config::save(&paths, &config)?;
        emit_download(
            &app_for_worker,
            GPU_DOWNLOAD_ID,
            "done",
            "Restarting on GPU",
            1.0,
            0,
            None,
        );
        Ok(())
    })
    .await
    .map_err(|err| err.to_string())??;

    // ORT resolves its library once per process, so the GPU runtime only takes effect after a
    // restart. We no longer restart automatically here — the window vanishing with no warning
    // felt like a crash. The frontend now asks the user to restart once the download is done.
    let _ = app;
    Ok(())
}

/// Restart the app on user request (e.g. to apply GPU acceleration). `restart` diverges.
#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart()
}

/// Unload all in-memory translation models: stop managed GGUF servers and drop cached
/// ONNX sessions so RAM/VRAM is freed until the next translation reloads on demand.
#[tauri::command]
fn unload_models(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.shutdown_all()?;
    engines::onnx_mt::unload_all();
    Ok(())
}

/// Turn GPU translation off and restart back onto the bundled CPU runtime. The downloaded
/// GPU runtime and fp16 weights are left on disk so re-enabling is instant.
#[tauri::command]
fn disable_gpu_acceleration(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut config = config::load(&state.paths)?;
    config.gpu_enabled = false;
    config::save(&state.paths, &config)?;
    app.restart()
}

/// Download the Vulkan-enabled llama-server binary (~31 MB) and, if no GGUF translation
/// model is installed yet, also download Tencent Hy-MT2 1.8B (~1.1 GB). Then set
/// `vulkan_gpu_enabled = true` and unload running servers so the next translation starts
/// the Vulkan binary. No app restart needed.
#[tauri::command]
async fn enable_vulkan_acceleration(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let paths = state.paths.clone();
    let app_for_worker = app.clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        // Step 1 — Vulkan binary (0 → 0.45).
        emit_download(
            &app_for_worker,
            VULKAN_DOWNLOAD_ID,
            "downloading",
            "Downloading Vulkan runtime",
            0.0,
            0,
            None,
        );
        gpu_runtime::download_vulkan_runtime(&paths, &mut |frac, msg| {
            emit_download(
                &app_for_worker,
                VULKAN_DOWNLOAD_ID,
                "downloading",
                msg,
                (frac * 0.45).clamp(0.0, 0.45),
                0,
                None,
            );
        })?;

        // Step 2 — GGUF model (0.45 → 0.95) only if none is installed yet.
        let gguf_id = "tencent-hy-mt2-1.8b-gguf";
        let gguf_file = "Hy-MT2-1.8B-Q4_K_M.gguf";
        let gguf_dir = paths.models_dir.join(gguf_id);
        let gguf_path = gguf_dir.join(gguf_file);
        if !gguf_path.is_file() {
            std::fs::create_dir_all(&gguf_dir).map_err(|err| err.to_string())?;
            let url = format!(
                "https://huggingface.co/tencent/Hy-MT2-1.8B-GGUF/resolve/main/{gguf_file}"
            );
            let part = gguf_dir.join(format!("{gguf_file}.part"));
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(60 * 60))
                .build()
                .map_err(|err| err.to_string())?;
            download_hf_file_with_progress(
                &client,
                &url,
                &part,
                &mut |downloaded, total| {
                    let model_frac = total.map(|t| downloaded as f64 / t as f64).unwrap_or(0.0);
                    emit_download(
                        &app_for_worker,
                        VULKAN_DOWNLOAD_ID,
                        "downloading",
                        "Downloading translation model",
                        (0.45 + model_frac * 0.5).clamp(0.45, 0.95),
                        downloaded,
                        total,
                    );
                },
            )?;
            std::fs::rename(&part, &gguf_path).map_err(|err| {
                format!("Could not finalize {gguf_file}: {err}")
            })?;
        }

        // Step 3 — update config.
        let mut config = config::load(&paths)?;
        config.vulkan_gpu_enabled = true;
        // If currently on an ONNX model, switch to the GGUF model so Vulkan is actually used.
        let is_onnx = config.model_id.ends_with("-onnx");
        if is_onnx {
            config.model_id = gguf_id.into();
        }
        config::save(&paths, &config)?;

        emit_download(
            &app_for_worker,
            VULKAN_DOWNLOAD_ID,
            "done",
            "GPU ready",
            1.0,
            0,
            None,
        );
        Ok(())
    })
    .await
    .map_err(|err| err.to_string())??;

    // Unload CPU llama-server so the next translation picks the Vulkan binary.
    state.runtime.shutdown_all()?;
    engines::onnx_mt::unload_all();
    Ok(())
}

/// Turn Vulkan (AMD/Intel) GPU translation off. The Vulkan binary is kept on disk so
/// re-enabling is instant (no re-download).
#[tauri::command]
fn disable_vulkan_acceleration(state: State<'_, AppState>) -> Result<(), String> {
    let mut config = config::load(&state.paths)?;
    config.vulkan_gpu_enabled = false;
    config::save(&state.paths, &config)?;
    state.runtime.shutdown_all()?;
    engines::onnx_mt::unload_all();
    Ok(())
}

/// Download a single file from HuggingFace with per-chunk progress callbacks.
fn download_hf_file_with_progress(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), String> {
    use std::io::Write as _;
    let mut response = client
        .get(url)
        .send()
        .map_err(|err| format!("Could not download {url}: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Download returned an error for {url}: {err}"))?;
    let total = response.content_length();
    let mut out =
        File::create(dest).map_err(|err| format!("Could not create {}: {err}", dest.display()))?;
    let mut buf = [0u8; 256 * 1024];
    let mut downloaded = 0u64;
    let mut last_emit = 0u64;
    loop {
        let n = response
            .read(&mut buf)
            .map_err(|err| format!("Read error from {url}: {err}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])
            .map_err(|err| format!("Write error to {}: {err}", dest.display()))?;
        downloaded += n as u64;
        if downloaded - last_emit >= 8 * 1024 * 1024 {
            last_emit = downloaded;
            on_progress(downloaded, total);
        }
    }
    Ok(())
}

async fn install_spec_model(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: ModelCatalogEntry,
) -> Result<String, String> {
    {
        let mut download = state
            .download
            .lock()
            .map_err(|_| "Download state is poisoned")?;
        if download.active_model.is_some() {
            return Err("Another model download is already running.".into());
        }
        download.active_model = Some(profile.id.clone());
        download.cancel_requested = false;
    }

    let paths = state.paths.clone();
    let app_for_download = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        download_spec_model_files(&app_for_download, &paths, &profile)
    })
    .await
    .map_err(|err| err.to_string())?;

    {
        let mut download = state
            .download
            .lock()
            .map_err(|_| "Download state is poisoned")?;
        download.active_model = None;
        download.cancel_requested = false;
    }

    result
}

fn download_spec_model_files(
    app: &AppHandle,
    paths: &AppPaths,
    profile: &ModelCatalogEntry,
) -> Result<String, String> {
    if profile.files.is_empty() {
        return Err("This model has no downloadable files yet.".into());
    }

    let target = paths.models_dir.join(&profile.id);
    std::fs::create_dir_all(&target).map_err(|err| err.to_string())?;
    clear_installed_model_metadata(paths, &profile.id)?;
    clear_install_manifest(&target);
    ensure_enough_disk_space(paths, &target, profile)?;

    emit_download(
        app,
        &profile.id,
        "starting",
        "Checking files",
        0.02,
        0,
        Some(profile.estimated_download_bytes),
    );

    let client = reqwest::blocking::Client::new();
    let mut downloaded = existing_spec_downloaded_bytes(&target, profile);
    let total = if profile.estimated_download_bytes > 0 {
        Some(profile.estimated_download_bytes)
    } else {
        None
    };

    for file in &profile.files {
        if is_download_cancelled(app, &profile.id)? {
            emit_download(
                app,
                &profile.id,
                "cancelled",
                "Download cancelled",
                0.0,
                downloaded,
                total,
            );
            return Err("Download cancelled.".into());
        }

        let local_path = target.join(&file.destination);
        if is_complete_spec_file(&local_path, file)? {
            continue;
        }
        if local_path.is_file() {
            let _ = std::fs::remove_file(&local_path);
        }
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }

        let part_path = local_path.with_extension(format!(
            "{}part",
            local_path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| format!("{value}."))
                .unwrap_or_default()
        ));
        let mut resume_from = part_path
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        if resume_from > 0 {
            downloaded += resume_from;
            emit_download(
                app,
                &profile.id,
                "downloading",
                &file.destination,
                download_ratio(downloaded, total),
                downloaded,
                total,
            );
        }

        let url = format!(
            "https://huggingface.co/{}/resolve/main/{}",
            file.repo, file.path
        );
        let mut request = client.get(url);
        if resume_from > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
        }
        let mut response = request
            .send()
            .map_err(|err| format!("Could not download {}: {err}", file.path))?
            .error_for_status()
            .map_err(|err| format!("Hugging Face returned an error for {}: {err}", file.path))?;
        if resume_from > 0 && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            downloaded = downloaded.saturating_sub(resume_from);
            resume_from = 0;
        }
        let mut output = std::fs::OpenOptions::new()
            .create(true)
            .append(resume_from > 0)
            .write(true)
            .truncate(resume_from == 0)
            .open(&part_path)
            .map_err(|err| format!("Could not create {}: {err}", local_path.display()))?;
        let mut buffer = [0_u8; 128 * 1024];
        let mut file_downloaded = resume_from;
        loop {
            if is_download_cancelled(app, &profile.id)? {
                emit_download(
                    app,
                    &profile.id,
                    "cancelled",
                    "Download cancelled",
                    0.0,
                    downloaded,
                    total,
                );
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
                &file.destination,
                download_ratio(downloaded, total),
                downloaded,
                total,
            );
        }
        if let Some(expected) = file.size_bytes {
            if file_downloaded != expected {
                return Err(format!(
                    "Downloaded {} bytes for {}, expected {}.",
                    file_downloaded, file.path, expected
                ));
            }
        }
        std::fs::rename(&part_path, &local_path)
            .map_err(|err| format!("Could not finalize {}: {err}", local_path.display()))?;
        if file.sha256.is_some() {
            emit_download(
                app,
                &profile.id,
                "verifying",
                &file.destination,
                download_ratio(downloaded, total),
                downloaded,
                total,
            );
            verify_spec_file(&local_path, file)?;
        }
    }

    write_spec_install_manifest(&target, profile)?;
    persist_spec_install_metadata(paths, profile, &target)?;
    emit_download(app, &profile.id, "done", "Ready", 1.0, downloaded, total);
    Ok(target.display().to_string())
}

fn download_huggingface_repo(
    app: &AppHandle,
    paths: &AppPaths,
    runtime: &Arc<RuntimeManager>,
    profile: &ModelProfile,
    repo: &str,
    mut config: AppConfig,
) -> Result<String, String> {
    let target = paths.models_dir.join(&profile.id);
    std::fs::create_dir_all(&target).map_err(|err| err.to_string())?;
    clear_install_manifest(&target);
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
    let files = if profile.download_filenames.is_empty() {
        files
    } else {
        files
            .into_iter()
            .filter(|file| {
                profile
                    .download_filenames
                    .iter()
                    .any(|name| name == &file.path)
            })
            .collect::<Vec<_>>()
    };
    if files.is_empty() {
        return Err("Built-in catalog entry is missing its curated download file.".into());
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
            emit_download(
                app,
                &profile.id,
                "cancelled",
                "Download cancelled",
                0.0,
                downloaded,
                total,
            );
            return Err("Download cancelled.".into());
        }

        let local_path = target.join(&file.path);
        if let Some(size) = file.size {
            if local_path.exists()
                && local_path
                    .metadata()
                    .map(|metadata| metadata.len())
                    .unwrap_or_default()
                    == size
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
        let part_path = local_path.with_extension(format!(
            "{}part",
            local_path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| format!("{value}."))
                .unwrap_or_default()
        ));
        let mut output = File::create(&part_path)
            .map_err(|err| format!("Could not create {}: {err}", local_path.display()))?;
        let mut buffer = [0_u8; 128 * 1024];
        let mut file_downloaded = 0_u64;
        loop {
            if is_download_cancelled(app, &profile.id)? {
                emit_download(
                    app,
                    &profile.id,
                    "cancelled",
                    "Download cancelled",
                    0.0,
                    downloaded,
                    total,
                );
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
        std::fs::rename(&part_path, &local_path)
            .map_err(|err| format!("Could not finalize {}: {err}", local_path.display()))?;
    }

    write_install_manifest(&target, repo, profile)?;

    config.model_id = profile.id.clone();
    match profile.provider {
        ProviderKind::Custom if profile.id != "custom-local" => {
            config.custom_backend_mode = "managed-gguf".into();
            config.openai_model = profile.name.clone();
            if let Some(style) = &profile.managed_prompt_style {
                config.local_prompt_style = style.clone();
            }
            if let Some(template) = &profile.managed_prompt_template {
                config.local_prompt_template = template.clone();
            }
            if let Some(context) = profile.managed_context_size {
                config.local_context_size = context;
            }
        }
        ProviderKind::OpenAiCompatible => {
            if let Some(endpoint) = &profile.default_endpoint {
                config.openai_endpoint = endpoint.clone();
            }
            config.openai_model = profile.name.clone();
        }
        _ => {}
    }
    config::save(paths, &config)?;
    if config.local_model_policy == "fast"
        && profile.provider == ProviderKind::Custom
        && profile.id != "custom-local"
    {
        emit_download(
            app,
            &profile.id,
            "preparing",
            "Loading model into memory",
            0.99,
            downloaded,
            total,
        );
        let _ = runtime.ensure_catalog_llama_server(paths, &config, profile, &profile.id);
    }

    emit_download(app, &profile.id, "done", "Ready", 1.0, downloaded, total);
    Ok(target.display().to_string())
}

fn huggingface_files(
    client: &reqwest::blocking::Client,
    repo: &str,
) -> Result<Vec<HfFile>, String> {
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
    let download = state
        .download
        .lock()
        .map_err(|_| "Download state is poisoned")?;
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

fn emit_translation_progress(app: &AppHandle, status: &str, translated_text: &str) {
    let _ = app.emit(
        "translation-progress",
        TranslationProgress {
            status: status.into(),
            translated_text: translated_text.into(),
        },
    );
}

fn ensure_enough_disk_space(
    paths: &AppPaths,
    target: &Path,
    profile: &ModelCatalogEntry,
) -> Result<(), String> {
    let current_bytes = current_model_dir_bytes(target)?;
    let needed = profile
        .estimated_disk_bytes
        .saturating_sub(current_bytes)
        .saturating_add(64 * 1024 * 1024);
    let available = free_disk_bytes(&paths.models_dir)?;
    if available < needed {
        return Err(format!(
            "Not enough free disk space for {}. Need about {}, but only {} is available.",
            profile.name,
            human_bytes(needed),
            human_bytes(available)
        ));
    }
    Ok(())
}

fn current_model_dir_bytes(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in std::fs::read_dir(path).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let entry_path = entry.path();
        if entry.file_name().to_string_lossy() == ".waylate-complete.json" {
            continue;
        }
        if entry_path.is_dir() {
            total = total.saturating_add(current_model_dir_bytes(&entry_path)?);
        } else {
            total = total.saturating_add(entry.metadata().map_err(|err| err.to_string())?.len());
        }
    }
    Ok(total)
}

fn free_disk_bytes(path: &Path) -> Result<u64, String> {
    let c_path = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| format!("Could not inspect free space for {}", path.display()))?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), stats.as_mut_ptr()) };
    if rc != 0 {
        return Err(format!(
            "Could not inspect free space for {}",
            path.display()
        ));
    }
    let stats = unsafe { stats.assume_init() };
    Ok((stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

fn human_bytes(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        format!("{:.1} GB", bytes as f64 / GIB as f64)
    } else {
        format!("{} MB", (bytes / MIB).max(1))
    }
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
                Some(
                    "Waylate could not read selected text. Paste text manually or copy it first."
                        .into(),
                ),
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
        window.unminimize().map_err(|err| err.to_string())?;
        window.show().map_err(|err| err.to_string())?;
        window.set_focus().map_err(|err| err.to_string())?;
        raise_via_wm();
        return Ok(());
    }

    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("/".into()))
        .title(title)
        .inner_size(1040.0, 720.0)
        .min_inner_size(920.0, 620.0)
        .resizable(true)
        .visible(true)
        .build()
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Tauri's own show()/set_focus()/always-on-top calls are not enough to raise a window that
/// is open but buried behind others: KWin's focus-stealing prevention silently ignores a
/// background process's own present() request, whether the window is a native Wayland
/// toplevel or (since `maybe_reexec_for_x11`) an XWayland one. `xdotool windowactivate`
/// issues a proper EWMH `_NET_ACTIVE_WINDOW` client message, which KWin does honor — this
/// mirrors how the previous Crow Translate mouse macro reliably raised its window. Best
/// effort: silently does nothing if xdotool is missing.
fn raise_via_wm() {
    let pid = std::process::id().to_string();
    let _ = Command::new("xdotool")
        .args(["search", "--pid", &pid, "windowactivate"])
        .output();
}

fn handle_args(app: &AppHandle, args: &[String]) {
    let command = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str);
    match command {
        Some("translate-selection") => {
            let (source_text, notice) = read_selection_with_fallback().unwrap_or_else(|err| {
                (
                    String::new(),
                    Some(format!("Could not read Wayland selection: {err}")),
                )
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
    let translate =
        MenuItemBuilder::with_id("translate_clipboard", "Translate clipboard").build(app)?;
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

fn environment_report(_paths: &AppPaths, config: &AppConfig) -> EnvironmentReport {
    let runtime_report = RuntimeManager::new().report(config);
    EnvironmentReport {
        desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        session_type: std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
        has_wl_clipboard: has_command("wl-paste") && has_command("wl-copy"),
        has_python: has_command("python3"),
        has_nvidia_smi: has_command("nvidia-smi"),
        has_rocm_smi: has_command("rocm-smi"),
        has_llama_server: runtime_report.llama_binary_found,
        llama_cuda_reported: runtime_report.llama_cuda_reported,
        total_memory_bytes: system_total_memory_bytes(),
    }
}

fn system_total_memory_bytes() -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::sysinfo>::uninit();
    let result = unsafe { libc::sysinfo(info.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    Some((info.totalram as u128 * info.mem_unit as u128).min(u64::MAX as u128) as u64)
}

fn has_command(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn collect_model_states(
    paths: &AppPaths,
    config: &AppConfig,
    catalog: &[ModelProfile],
) -> Vec<ModelInstallState> {
    catalog
        .iter()
        .map(|profile| ModelInstallState {
            model_id: profile.id.clone(),
            status: install_status(paths, config, profile).into(),
        })
        .collect()
}

fn install_status(paths: &AppPaths, config: &AppConfig, profile: &ModelProfile) -> &'static str {
    if let Some(spec) = models::model_catalog()
        .into_iter()
        .find(|entry| entry.id == profile.id)
    {
        return spec_install_status(paths, config, &spec);
    }
    if profile.provider == ProviderKind::Custom && profile.id != "custom-local" {
        let target = paths.models_dir.join(&profile.id);
        return catalog_install_status(&target, profile);
    }

    match profile.provider {
        ProviderKind::Custom => {
            if config.custom_backend_mode == "managed-gguf" {
                if std::path::Path::new(config.custom_model_path.trim()).is_file() {
                    "installed"
                } else {
                    "missing"
                }
            } else if !config.openai_endpoint.trim().is_empty() {
                "installed"
            } else {
                "missing"
            }
        }
        ProviderKind::DeepL => {
            if config.api_provider_enabled && secrets::has("deepl") {
                "installed"
            } else {
                "missing"
            }
        }
        ProviderKind::Google => {
            if config.api_provider_enabled && secrets::has("google") {
                "installed"
            } else {
                "missing"
            }
        }
        ProviderKind::Yandex => {
            if config.api_provider_enabled
                && secrets::has("yandex")
                && !config.yandex_folder_id.trim().is_empty()
            {
                "installed"
            } else {
                "missing"
            }
        }
        ProviderKind::OpenAiCompatible => "missing",
    }
}

fn spec_install_status(
    paths: &AppPaths,
    config: &AppConfig,
    profile: &ModelCatalogEntry,
) -> &'static str {
    let target = paths.models_dir.join(&profile.id);
    if has_persisted_spec_install(config, &target, profile)
        && has_spec_install_manifest(&target, profile)
    {
        return "installed";
    }
    if spec_dir_has_partial_files(&target, profile) {
        return "partial";
    }
    "missing"
}

fn catalog_install_status(path: &Path, profile: &ModelProfile) -> &'static str {
    if has_valid_install_manifest(path, profile)
        || has_valid_spec_install_manifest(path)
        || has_legacy_complete_install(path, profile)
    {
        return "installed";
    }
    if dir_has_any_files(path) {
        return "partial";
    }
    "missing"
}

fn has_valid_spec_install_manifest(path: &Path) -> bool {
    let manifest_path = path.join(".waylate-spec-manifest.json");
    manifest_path.is_file()
}

fn dir_has_any_files(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .map(|mut entries| entries.any(|entry| entry.is_ok()))
        .unwrap_or(false)
}

fn has_valid_install_manifest(path: &Path, profile: &ModelProfile) -> bool {
    let manifest_path = install_manifest_path(path);
    let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_str::<InstallManifest>(&raw) else {
        return false;
    };
    if manifest.version != 1 {
        return false;
    }
    if manifest.files.is_empty() {
        return false;
    }
    manifest.files.iter().all(|file| path.join(file).is_file())
        && profile
            .install_check_files
            .iter()
            .all(|file| path.join(file).exists())
}

fn has_legacy_complete_install(path: &Path, profile: &ModelProfile) -> bool {
    if profile.install_check_files.is_empty() {
        return dir_has_any_files(path);
    }
    profile
        .install_check_files
        .iter()
        .all(|file| path.join(file).exists())
}

fn has_spec_install_manifest(path: &Path, profile: &ModelCatalogEntry) -> bool {
    let Ok(raw) = std::fs::read_to_string(install_manifest_path(path)) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_str::<InstallManifest>(&raw) else {
        return false;
    };
    if manifest.version != 1 || manifest.files.is_empty() {
        return false;
    }
    profile
        .files
        .iter()
        .all(|file| path.join(&file.destination).is_file())
}

fn has_persisted_spec_install(
    config: &AppConfig,
    path: &Path,
    profile: &ModelCatalogEntry,
) -> bool {
    let Some(metadata) = config.installed_models.get(&profile.id) else {
        return false;
    };
    if metadata.manifest_version != 1 {
        return false;
    }
    if metadata.install_dir != path.display().to_string() {
        return false;
    }
    if metadata.files.len() != profile.files.len() {
        return false;
    }
    profile
        .files
        .iter()
        .all(|file| metadata.files.iter().any(|name| name == &file.destination))
}

fn is_complete_spec_file(path: &Path, file: &models::ModelFile) -> Result<bool, String> {
    if !path.is_file() {
        return Ok(false);
    }
    if let Some(expected) = file.size_bytes {
        let actual = path.metadata().map_err(|err| err.to_string())?.len();
        if actual != expected {
            return Ok(false);
        }
    }
    if let Some(expected) = &file.sha256 {
        return Ok(file_sha256(path)? == *expected);
    }
    Ok(true)
}

fn verify_spec_file(path: &Path, file: &models::ModelFile) -> Result<(), String> {
    if let Some(expected) = &file.sha256 {
        let actual = file_sha256(path)?;
        if actual != *expected {
            return Err(format!(
                "SHA256 verification failed for {}.",
                file.destination
            ));
        }
    }
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|err| {
        format!(
            "Could not open {} for SHA256 verification: {err}",
            path.display()
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|err| {
            format!(
                "Could not read {} for SHA256 verification: {err}",
                path.display()
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn all_spec_files_complete(target: &Path, profile: &ModelCatalogEntry) -> bool {
    !profile.files.is_empty()
        && profile
            .files
            .iter()
            .all(|file| is_complete_spec_file(&target.join(&file.destination), file).unwrap_or(false))
}

fn spec_dir_has_partial_files(path: &Path, profile: &ModelCatalogEntry) -> bool {
    profile
        .files
        .iter()
        .any(|file| path.join(&file.destination).exists())
        || dir_has_any_files(path)
}

fn install_manifest_path(path: &Path) -> PathBuf {
    path.join(".waylate-complete.json")
}

fn clear_install_manifest(path: &Path) {
    let _ = std::fs::remove_file(install_manifest_path(path));
}

fn clear_installed_model_metadata(paths: &AppPaths, model_id: &str) -> Result<(), String> {
    let mut config = config::load(paths)?;
    if config.installed_models.remove(model_id).is_some() {
        config::save(paths, &config)?;
    }
    Ok(())
}

fn write_install_manifest(path: &Path, repo: &str, profile: &ModelProfile) -> Result<(), String> {
    let files = if profile.download_filenames.is_empty() {
        std::fs::read_dir(path)
            .map_err(|err| err.to_string())?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let entry_path = entry.path();
                if !entry_path.is_file() {
                    return None;
                }
                let name = entry.file_name();
                let name = name.to_str()?;
                if name == ".waylate-complete.json" || name.ends_with(".part") {
                    return None;
                }
                Some(name.to_string())
            })
            .collect::<Vec<_>>()
    } else {
        profile.download_filenames.clone()
    };
    let manifest = InstallManifest {
        version: 1,
        repo: repo.into(),
        files,
    };
    let raw = serde_json::to_vec_pretty(&manifest).map_err(|err| err.to_string())?;
    std::fs::write(install_manifest_path(path), raw).map_err(|err| err.to_string())
}

fn write_spec_install_manifest(path: &Path, profile: &ModelCatalogEntry) -> Result<(), String> {
    let manifest = InstallManifest {
        version: 1,
        repo: "model-catalog-entry".into(),
        files: profile
            .files
            .iter()
            .map(|file| file.destination.clone())
            .collect(),
    };
    let raw = serde_json::to_vec_pretty(&manifest).map_err(|err| err.to_string())?;
    std::fs::write(install_manifest_path(path), raw).map_err(|err| err.to_string())
}

fn persist_spec_install_metadata(
    paths: &AppPaths,
    profile: &ModelCatalogEntry,
    install_dir: &Path,
) -> Result<(), String> {
    let mut config = config::load(paths)?;
    config.installed_models.insert(
        profile.id.clone(),
        InstalledModelMetadata {
            install_dir: install_dir.display().to_string(),
            manifest_version: 1,
            installed_at: Utc::now().to_rfc3339(),
            files: profile
                .files
                .iter()
                .map(|file| file.destination.clone())
                .collect(),
        },
    );
    config::save(paths, &config)
}

fn existing_spec_downloaded_bytes(target: &Path, profile: &ModelCatalogEntry) -> u64 {
    profile
        .files
        .iter()
        .filter_map(|file| {
            let expected = file.size_bytes?;
            let actual = target.join(&file.destination).metadata().ok()?.len();
            (actual == expected).then_some(actual)
        })
        .sum()
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

/// Wayland's xdg-activation protocol requires a fresh, input-associated token to raise an
/// already-mapped window; a background instance woken up over the single-instance IPC socket
/// has no such token, so KWin silently ignores its present()/set_focus() calls (this is why
/// the window pops up fine on a cold start — a brand-new toplevel gets focus — but not when
/// waking an already-running instance from the tray). Running under XWayland sidesteps the
/// restriction entirely, the same way the previous Crow Translate macro forced
/// `QT_QPA_PLATFORM=xcb` to reliably raise its window. Must run before GTK initializes, so we
/// re-exec once with GDK_BACKEND=x11 set; `exec` replaces the image (same PID), so the
/// single-instance lock is unaffected.
fn maybe_reexec_for_x11() {
    use std::os::unix::process::CommandExt;
    if std::env::var("GDK_BACKEND").map(|v| v == "x11").unwrap_or(false) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let err = std::process::Command::new(exe)
        .args(args)
        .env("GDK_BACKEND", "x11")
        .exec();
    eprintln!("[gtk] X11 re-exec failed ({err}); window may not raise while backgrounded");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// When GPU translation is enabled, the downloaded onnxruntime needs its sibling libraries
/// (CUDA runtime, cuDNN, abseil, ...) on the loader search path. glibc captures
/// LD_LIBRARY_PATH at process start and ignores later setenv, so we re-exec ourselves once
/// with it set before any library is dlopened. `exec` replaces the image (same PID), so the
/// single-instance lock is unaffected. A guard env var prevents an infinite loop.
fn maybe_reexec_for_gpu() {
    use std::os::unix::process::CommandExt;
    // Single source of truth for the guard env var name: a typo across the two uses below
    // would cause an infinite re-exec loop.
    const GPU_REEXEC_ENV: &str = "WAYLATE_GPU_REEXEC";
    if std::env::var_os(GPU_REEXEC_ENV).is_some() {
        return;
    }
    let Ok(paths) = AppPaths::new() else { return };
    let Ok(config) = config::load(&paths) else { return };
    if !config.gpu_enabled {
        return;
    }
    let runtime_dir = engines::onnx_mt::gpu_runtime_dir(&paths);
    if !runtime_dir.join("libonnxruntime.so").is_file() {
        return;
    }
    let dir = runtime_dir.display().to_string();
    let new_ld = match std::env::var("LD_LIBRARY_PATH") {
        Ok(existing) if !existing.trim().is_empty() => format!("{dir}:{existing}"),
        _ => dir,
    };
    let Ok(exe) = std::env::current_exe() else { return };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let err = std::process::Command::new(exe)
        .args(args)
        .env("LD_LIBRARY_PATH", new_ld)
        .env(GPU_REEXEC_ENV, "1")
        .exec();
    eprintln!("[onnx] GPU re-exec failed ({err}); continuing on CPU runtime");
}

pub fn run() {
    maybe_reexec_for_x11();
    maybe_reexec_for_gpu();

    // ORT prebuilt binaries use OpenMP, which ignores SessionBuilder::with_intra_threads.
    // Setting OMP_NUM_THREADS here ensures the actual kernel thread count matches physical cores.
    let ort_threads = (std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        / 2)
    .max(2)
    .min(8);
    unsafe { std::env::set_var("OMP_NUM_THREADS", ort_threads.to_string()) };

    // Point ORT at a concrete onnxruntime library before any ORT call (model preload).
    // With the load-dynamic feature the library is resolved once per process, so this
    // must run first; switching CPU<->GPU therefore needs a restart.
    if let Ok(paths) = AppPaths::new() {
        if let Ok(config) = config::load(&paths) {
            match engines::onnx_mt::configure_ort_dylib(&paths, &config) {
                Some(path) => eprintln!("[onnx] runtime library: {path}"),
                None => eprintln!(
                    "[onnx] no onnxruntime library found — ONNX translation will be unavailable"
                ),
            }
        }
    }

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
            paths
                .ensure()
                .map_err(|err| Box::<dyn std::error::Error>::from(err))?;
            history::init(&paths.history_db)
                .map_err(|err| Box::<dyn std::error::Error>::from(err))?;
            let runtime = Arc::new(RuntimeManager::new());
            if let Ok(config) = config::load(&paths) {
                let _ = autostart::sync(&paths, config.autostart);
                runtime.maybe_preload(&paths, &config);
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
            list_model_profiles,
            install_model,
            uninstall_model,
            cancel_model_install,
            get_model_status,
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
            delete_history_entry,
            reveal_path,
            read_runtime_log,
            download_catalog_model,
            cancel_model_download,
            enable_gpu_acceleration,
            disable_gpu_acceleration,
            enable_vulkan_acceleration,
            disable_vulkan_acceleration,
            restart_app,
            unload_models
        ])
        .run(tauri::generate_context!())
        .expect("error while running Waylate");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_ratio_uses_known_total() {
        assert_eq!(download_ratio(50, Some(100)), 0.5);
    }

    #[test]
    fn download_ratio_clamps_to_visible_range() {
        // Never show a literal 0% or 100% bar while a download is still in progress.
        assert_eq!(download_ratio(0, Some(100)), 0.02);
        assert_eq!(download_ratio(100, Some(100)), 0.99);
    }

    #[test]
    fn download_ratio_falls_back_when_total_unknown() {
        assert_eq!(download_ratio(500, None), 0.15);
        assert_eq!(download_ratio(500, Some(0)), 0.15);
    }

    #[test]
    fn human_bytes_switches_units_at_one_gib() {
        assert_eq!(human_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
        assert_eq!(human_bytes(512 * 1024 * 1024), "512 MB");
    }

    #[test]
    fn human_bytes_never_reports_zero_mb() {
        // Sub-megabyte sizes round up to "1 MB" rather than the confusing "0 MB".
        assert_eq!(human_bytes(1024), "1 MB");
    }

    #[test]
    fn install_manifest_path_is_under_model_dir() {
        let path = install_manifest_path(Path::new("/models/nllb"));
        assert_eq!(path, Path::new("/models/nllb/.waylate-complete.json"));
    }
}
