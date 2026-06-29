use crate::{
    config::{AppConfig, AppPaths},
    models::ModelProfile,
};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    net::TcpListener,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeReport {
    pub active_profiles: Vec<RuntimeEntry>,
    pub selected_model_loaded: bool,
    pub selected_device: Option<String>,
    pub onnx_device: Option<String>,
    pub llama_binary_found: bool,
    pub llama_cuda_reported: bool,
    pub llama_vulkan_reported: bool,
    /// Physical GPU vendor detected on the machine: "nvidia" | "amd" | "intel" | None.
    /// Used by the UI to decide whether to offer GPU acceleration.
    pub gpu_vendor: Option<String>,
    /// Human-friendly GPU name for display, e.g. "NVIDIA GeForce RTX 3060 Laptop GPU".
    pub gpu_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEntry {
    pub profile_id: String,
    pub kind: String,
    pub device: String,
    pub idle_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct ManagedEndpoint {
    pub endpoint: String,
    pub device: String,
}

#[derive(Debug)]
pub struct RuntimeManager {
    processes: Mutex<HashMap<String, ManagedProcess>>,
}

#[derive(Debug)]
struct ManagedProcess {
    child: Child,
    endpoint: String,
    profile_id: String,
    kind: RuntimeKind,
    device_label: String,
    last_used_at: Instant,
    signature: String,
}

#[derive(Debug)]
enum RuntimeKind {
    LlamaServer,
}

impl RuntimeKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::LlamaServer => "llama-server",
        }
    }
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
        }
    }

    pub fn report(&self, config: &AppConfig) -> RuntimeReport {
        let llama_binary = resolve_llama_binary(config);
        let llama_cuda_reported = llama_binary
            .as_ref()
            .map(|binary| llama_reports_cuda(binary))
            .unwrap_or(false);
        let llama_vulkan_reported = llama_binary
            .as_ref()
            .map(|binary| llama_reports_vulkan(binary))
            .unwrap_or(false);

        let mut active_profiles = Vec::new();
        let mut selected_model_loaded = false;
        let mut selected_device = None;
        if let Ok(mut processes) = self.processes.lock() {
            let dead = collect_dead_profiles(&mut processes);
            for profile_id in dead {
                if let Some(mut process) = processes.remove(&profile_id) {
                    stop_process(&mut process);
                }
            }
            for process in processes.values() {
                let entry = RuntimeEntry {
                    profile_id: process.profile_id.clone(),
                    kind: process.kind.as_str().into(),
                    device: process.device_label.clone(),
                    idle_seconds: process.last_used_at.elapsed().as_secs(),
                };
                if process.profile_id == config.model_id {
                    selected_model_loaded = true;
                    selected_device = Some(process.device_label.clone());
                }
                active_profiles.push(entry);
            }
        }

        let onnx_device = crate::engines::onnx_mt::active_device(&config.model_id);
        let (gpu_vendor, gpu_name) = detect_gpu().clone();

        RuntimeReport {
            active_profiles,
            selected_model_loaded,
            selected_device,
            onnx_device,
            llama_binary_found: llama_binary.is_some(),
            llama_cuda_reported,
            llama_vulkan_reported,
            gpu_vendor,
            gpu_name,
        }
    }

    pub fn cleanup_idle(&self, config: &AppConfig) {
        let timeout = timeout_for_policy(config);
        let mut to_stop = Vec::new();
        if let Ok(mut processes) = self.processes.lock() {
            let dead = collect_dead_profiles(&mut processes);
            to_stop.extend(dead);
            if let Some(timeout) = timeout {
                for (profile_id, process) in processes.iter() {
                    if process.last_used_at.elapsed() >= timeout {
                        to_stop.push(profile_id.clone());
                    }
                }
            }
            to_stop.sort();
            to_stop.dedup();
            for profile_id in &to_stop {
                if let Some(mut process) = processes.remove(profile_id) {
                    stop_process(&mut process);
                }
            }
        }
    }

    pub fn maybe_preload(&self, paths: &AppPaths, config: &AppConfig) {
        // ONNX models preload regardless of policy — they are the default beginner path.
        if let Some(entry) = crate::models::model_catalog()
            .into_iter()
            .find(|e| e.id == config.model_id && e.engine == crate::models::EngineKind::OnnxEncoderDecoder)
        {
            let paths_clone = paths.clone();
            thread::spawn(move || {
                crate::engines::onnx_mt::preload(&entry, &paths_clone);
            });
            return;
        }

        if config.local_model_policy != "fast" {
            return;
        }
        if let Some(profile) = crate::models::catalog().into_iter().find(|profile| {
            profile.id == config.model_id
                && profile.id != "custom-local"
                && profile.provider == crate::models::ProviderKind::Custom
        }) {
            let _ = self.ensure_catalog_llama_server(paths, config, &profile, &config.model_id);
        } else if config.model_id == "custom-local" && config.custom_backend_mode == "managed-gguf"
        {
            let _ = self.ensure_llama_server(paths, config, &config.model_id);
        }
    }

    pub fn ensure_llama_server(
        &self,
        paths: &AppPaths,
        config: &AppConfig,
        profile_id: &str,
    ) -> Result<ManagedEndpoint, String> {
        self.ensure_llama_server_with_model(
            paths,
            config,
            profile_id,
            config.custom_model_path.trim(),
            config.local_context_size,
        )
    }

    pub fn ensure_catalog_llama_server(
        &self,
        paths: &AppPaths,
        config: &AppConfig,
        profile: &ModelProfile,
        profile_id: &str,
    ) -> Result<ManagedEndpoint, String> {
        let model_path = resolve_catalog_gguf_path(paths, profile)
            .ok_or_else(|| "This model is not installed - Download it in Settings.".to_string())?;
        let context = profile
            .managed_context_size
            .unwrap_or(config.local_context_size);
        self.ensure_llama_server_with_model(paths, config, profile_id, &model_path, context)
    }

    fn ensure_llama_server_with_model(
        &self,
        paths: &AppPaths,
        config: &AppConfig,
        profile_id: &str,
        model_path: &str,
        context_size: u32,
    ) -> Result<ManagedEndpoint, String> {
        if model_path.trim().is_empty() {
            return Err("This model is not installed - Download it in Settings.".into());
        }
        if !model_path.trim().ends_with(".gguf") {
            return Err("Managed local mode currently supports GGUF files only.".into());
        }
        let binary = ensure_managed_llama_binary(paths, config)
            .map_err(|err| format!("Could not prepare llama-server: {err}"))?;
        let signature = format!("{}|{}|{}", binary, model_path.trim(), context_size);

        if let Some(endpoint) = self.reuse_existing(profile_id, &signature) {
            return Ok(endpoint);
        }

        let port = pick_free_port()?;
        let endpoint = format!("http://127.0.0.1:{port}");
        let mut command = Command::new(&binary);
        if let Some(runtime_dir) = managed_llama_library_dir(&binary) {
            let mut ld_path = runtime_dir.display().to_string();
            if let Ok(existing) = std::env::var("LD_LIBRARY_PATH") {
                if !existing.trim().is_empty() {
                    ld_path.push(':');
                    ld_path.push_str(&existing);
                }
            }
            command.env("LD_LIBRARY_PATH", ld_path);
        }
        command
            .arg("--model")
            .arg(model_path.trim())
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-c")
            .arg(context_size.to_string())
            .arg("-ngl")
            .arg("99");
        if profile_id == "translategemma-4b-gguf" {
            // This GGUF ships a strict Jinja chat template that fails llama-server's
            // load-time validation (it requires a structured content object), which
            // aborts the server before it can serve. Force the built-in gemma template.
            command.arg("--chat-template").arg("gemma");
        }
        let log_path = runtime_log_path(paths, "llama-server.log");
        let logs = runtime_log_files_for_path(&log_path)?;
        let mut child = command
            .stdout(Stdio::from(logs.0))
            .stderr(Stdio::from(logs.1))
            .spawn()
            .map_err(|err| format!("Could not start managed GGUF runtime: {err}"))?;

        wait_for_http_health(&mut child, &endpoint, Duration::from_secs(60), &log_path)?;
        let device = if llama_reports_cuda(&binary) {
            "cuda".to_string()
        } else if llama_reports_vulkan(&binary) {
            "vulkan".to_string()
        } else {
            "cpu".to_string()
        };
        let process = ManagedProcess {
            child,
            endpoint: endpoint.clone(),
            profile_id: profile_id.into(),
            kind: RuntimeKind::LlamaServer,
            device_label: device.clone(),
            last_used_at: Instant::now(),
            signature,
        };
        self.insert_process(profile_id, process);
        Ok(ManagedEndpoint { endpoint, device })
    }

    pub fn touch(&self, profile_id: &str) {
        if let Ok(mut processes) = self.processes.lock() {
            if let Some(process) = processes.get_mut(profile_id) {
                process.last_used_at = Instant::now();
            }
        }
    }

    pub fn apply_post_translate_policy(&self, config: &AppConfig, profile_id: &str) {
        self.touch(profile_id);
        if config.local_model_policy == "memory-saver" {
            let _ = self.shutdown_profile(profile_id);
            crate::engines::onnx_mt::unload(profile_id);
        }
    }

    pub fn shutdown_profile(&self, profile_id: &str) -> Result<(), String> {
        let mut process = {
            let mut processes = self
                .processes
                .lock()
                .map_err(|_| "Runtime state is poisoned")?;
            processes.remove(profile_id)
        };
        if let Some(process) = process.as_mut() {
            stop_process(process);
        }
        Ok(())
    }

    pub fn shutdown_all(&self) -> Result<(), String> {
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| "Runtime state is poisoned")?;
        let mut owned = HashMap::new();
        std::mem::swap(&mut *processes, &mut owned);
        drop(processes);
        for (_, mut process) in owned {
            stop_process(&mut process);
        }
        Ok(())
    }

    fn reuse_existing(&self, profile_id: &str, signature: &str) -> Option<ManagedEndpoint> {
        let mut stale = false;
        let mut endpoint = None;
        if let Ok(mut processes) = self.processes.lock() {
            if let Some(process) = processes.get_mut(profile_id) {
                if process.signature == signature
                    && process.child.try_wait().ok().flatten().is_none()
                    && health_ok(&process.endpoint)
                {
                    process.last_used_at = Instant::now();
                    endpoint = Some(ManagedEndpoint {
                        endpoint: process.endpoint.clone(),
                        device: process.device_label.clone(),
                    });
                } else {
                    stale = true;
                }
            }
            if stale {
                if let Some(mut process) = processes.remove(profile_id) {
                    stop_process(&mut process);
                }
            }
        }
        endpoint
    }

    fn insert_process(&self, profile_id: &str, process: ManagedProcess) {
        if let Ok(mut processes) = self.processes.lock() {
            if let Some(mut stale) = processes.remove(profile_id) {
                stop_process(&mut stale);
            }
            processes.insert(profile_id.into(), process);
        }
    }
}

fn wait_for_http_health(
    child: &mut Child,
    endpoint: &str,
    timeout: Duration,
    log_path: &Path,
) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|err| err.to_string())?;
    let health = format!("{endpoint}/health");
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            return Err(format!(
                "Managed local runtime exited early with status {status}. See {}.",
                log_path.display()
            ));
        }
        if client
            .get(&health)
            .send()
            .and_then(|response| response.error_for_status())
            .is_ok()
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(300));
    }
    Err(format!(
        "Managed local runtime did not become ready in time. See {}.",
        log_path.display()
    ))
}

fn health_ok(endpoint: &str) -> bool {
    Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .ok()
        .and_then(|client| client.get(format!("{endpoint}/health")).send().ok())
        .and_then(|response| response.error_for_status().ok())
        .is_some()
}

fn pick_free_port() -> Result<u16, String> {
    TcpListener::bind("127.0.0.1:0")
        .map_err(|err| format!("Could not pick a localhost port: {err}"))?
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|err| err.to_string())
}

fn stop_process(process: &mut ManagedProcess) {
    let _ = process.child.kill();
    let _ = process.child.wait();
}

fn collect_dead_profiles(processes: &mut HashMap<String, ManagedProcess>) -> Vec<String> {
    let mut dead = Vec::new();
    for (profile_id, process) in processes.iter_mut() {
        if process.child.try_wait().ok().flatten().is_some() {
            dead.push(profile_id.clone());
        }
    }
    dead
}

/// Detect the machine's GPU once and cache it — physical hardware does not change
/// while the app runs. Returns `(vendor, friendly_name)`.
///
/// `vendor` is one of "nvidia" | "amd" | "intel"; `None` means no discrete/known GPU
/// was found (or detection tools are missing). Only "nvidia" and "amd" are candidates
/// for translation acceleration; "intel" is reported for completeness but not offered.
pub(crate) fn detect_gpu() -> &'static (Option<String>, Option<String>) {
    static GPU_INFO: OnceLock<(Option<String>, Option<String>)> = OnceLock::new();
    GPU_INFO.get_or_init(|| {
        // Preferred: nvidia-smi gives an exact model name and confirms a usable driver.
        if let Ok(output) = Command::new("nvidia-smi")
            .args(["--query-gpu=name", "--format=csv,noheader"])
            .output()
        {
            if output.status.success() {
                let name = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty());
                if let Some(name) = name {
                    return (Some("nvidia".to_string()), Some(name));
                }
            }
        }

        // Fallback: parse `lspci` so we still detect a card when no driver tools exist.
        if let Ok(output) = Command::new("sh")
            .arg("-c")
            .arg("lspci 2>/dev/null | grep -Ei 'vga|3d controller|display'")
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
            if text.contains("nvidia") {
                return (Some("nvidia".to_string()), Some("NVIDIA GPU".to_string()));
            }
            if text.contains("amd")
                || text.contains("radeon")
                || text.contains("advanced micro devices")
            {
                return (Some("amd".to_string()), Some("AMD GPU".to_string()));
            }
            if text.contains("intel") {
                return (Some("intel".to_string()), Some("Intel GPU".to_string()));
            }
        }

        (None, None)
    })
}

fn timeout_for_policy(config: &AppConfig) -> Option<Duration> {
    match config.local_model_policy.as_str() {
        "memory-saver" => Some(Duration::from_secs(0)),
        "balanced" => Some(Duration::from_secs(
            config.local_model_idle_timeout_secs.max(15),
        )),
        _ => None,
    }
}

pub fn runtime_log_path(paths: &AppPaths, name: &str) -> PathBuf {
    let logs_dir = paths.data_dir.join("logs");
    logs_dir.join(name)
}

fn runtime_log_files_for_path(path: impl Into<PathBuf>) -> Result<(File, File), String> {
    let path = path.into();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| format!("Could not open {}: {err}", path.display()))?;
    let stderr = stdout
        .try_clone()
        .map_err(|err| format!("Could not clone {}: {err}", path.display()))?;
    Ok((stdout, stderr))
}

fn resolve_llama_binary(config: &AppConfig) -> Option<String> {
    let configured = config.local_llama_server_path.trim();
    if !configured.is_empty() {
        return Some(configured.into());
    }
    let output = Command::new("sh")
        .arg("-c")
        .arg("command -v llama-server")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    if path.is_empty() {
        None
    } else {
        Some(path.into())
    }
}

fn llama_reports_cuda(binary: &str) -> bool {
    for args in [&["--version"][..], &["--help"][..]] {
        let output = match Command::new(binary).args(args).output() {
            Ok(output) => output,
            Err(_) => continue,
        };
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .to_lowercase();
        if text.contains("cuda") || text.contains("cublas") {
            return true;
        }
    }
    false
}

fn llama_reports_vulkan(binary: &str) -> bool {
    for args in [&["--version"][..], &["--help"][..]] {
        let output = match Command::new(binary).args(args).output() {
            Ok(output) => output,
            Err(_) => continue,
        };
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .to_lowercase();
        if text.contains("vulkan") {
            return true;
        }
    }
    false
}

pub fn resolve_catalog_gguf_path(paths: &AppPaths, profile: &ModelProfile) -> Option<String> {
    let target_dir = paths.models_dir.join(&profile.id);
    if let Some(filename) = profile.download_filenames.first() {
        let path = target_dir.join(filename);
        if path.is_file() {
            return Some(path.display().to_string());
        }
    }
    std::fs::read_dir(target_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("gguf"))
                .unwrap_or(false)
        })
        .map(|path| path.display().to_string())
}

pub fn prompt_template(config: &AppConfig, source: &str, target: &str, text: &str) -> String {
    config
        .local_prompt_template
        .replace("{source}", source)
        .replace("{target}", target)
        .replace("{text}", text)
}

pub fn translate_via_managed_llama(
    manager: &RuntimeManager,
    paths: &AppPaths,
    config: &AppConfig,
    profile_id: &str,
    source: &str,
    target: &str,
    text: &str,
    on_progress: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<(String, String), String> {
    let endpoint = manager.ensure_llama_server(paths, config, profile_id)?;
    let prompt = prompt_template(config, source, target, text);
    let translated = if config.local_prompt_style == "completion" {
        stream_completion(&endpoint.endpoint, &prompt, on_progress)?
    } else {
        let system = "You are a precise translation engine. Do not explain your answer.";
        stream_chat_completion(&endpoint.endpoint, system, &prompt, on_progress)?
    };
    Ok((translated, endpoint.device))
}

/// Translate a spec catalog GGUF model. Uses a pre-built prompt string (caller handles
/// template substitution) and the model's declared PromptStyle.
pub fn translate_via_spec_llama(
    manager: &RuntimeManager,
    paths: &AppPaths,
    config: &AppConfig,
    profile_id: &str,
    model_path: &str,
    context_size: u32,
    prompt_style: &Option<crate::models::PromptStyle>,
    prompt: &str,
    on_progress: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<(String, String), String> {
    let endpoint = manager.ensure_llama_server_with_model(
        paths,
        config,
        profile_id,
        model_path,
        context_size,
    )?;
    let is_completion = matches!(prompt_style, Some(crate::models::PromptStyle::Completion));
    let translated = if is_completion {
        stream_completion(&endpoint.endpoint, prompt, on_progress)?
    } else {
        let system = "You are a precise translation engine. Do not explain your answer.";
        stream_chat_completion(&endpoint.endpoint, system, prompt, on_progress)?
    };
    Ok((translated, endpoint.device))
}

/// Translate a spec catalog GGUF model with a caller-supplied chat-completions body.
/// Used by models (TranslateGemma) whose chat template needs a structured content object
/// rather than a flat prompt string. Non-streaming: the full response is returned at once.
pub fn translate_via_spec_llama_chat(
    manager: &RuntimeManager,
    paths: &AppPaths,
    config: &AppConfig,
    profile_id: &str,
    model_path: &str,
    context_size: u32,
    body: Value,
) -> Result<(String, String), String> {
    let endpoint = manager.ensure_llama_server_with_model(
        paths,
        config,
        profile_id,
        model_path,
        context_size,
    )?;
    let value: Value = Client::new()
        .post(format!("{}/v1/chat/completions", endpoint.endpoint))
        .json(&body)
        .send()
        .map_err(|err| format!("Local model could not process translation: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Local model returned an error: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse local model response: {err}"))?;
    let translated = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/choices/0/text").and_then(Value::as_str))
        .ok_or_else(|| "Local model response did not contain a translation".to_string())?
        .trim()
        .to_string();
    Ok((translated, endpoint.device))
}

fn stream_completion(
    base_endpoint: &str,
    prompt: &str,
    on_progress: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<String, String> {
    let response = Client::new()
        .post(format!("{base_endpoint}/completion"))
        .json(&json!({
            "prompt": prompt,
            "temperature": 0.1,
            "n_predict": 512,
            "stream": true
        }))
        .send()
        .map_err(|err| format!("Local model could not process translation: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Local model returned an error: {err}"))?;

    use std::io::BufRead;
    let reader = std::io::BufReader::new(response);
    let mut full_text = String::new();
    for line in reader.lines() {
        let line = line.map_err(|err| format!("Error reading completion stream: {err}"))?;
        let data = if let Some(d) = line.strip_prefix("data: ") { d } else { continue };
        if data == "[DONE]" { break; }
        if let Ok(val) = serde_json::from_str::<Value>(data) {
            if let Some(tok) = val.get("content").and_then(Value::as_str) {
                full_text.push_str(tok);
                on_progress(full_text.trim())?;
            }
            if val.get("stop").and_then(Value::as_bool).unwrap_or(false) { break; }
        }
    }
    Ok(full_text.trim().to_string())
}

fn stream_chat_completion(
    base_endpoint: &str,
    system: &str,
    prompt: &str,
    on_progress: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<String, String> {
    let response = Client::new()
        .post(format!("{base_endpoint}/v1/chat/completions"))
        .json(&json!({
            "model": "managed-local-gguf",
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.1,
            "stream": true
        }))
        .send()
        .map_err(|err| format!("Local model could not process translation: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Local model returned an error: {err}"))?;

    parse_sse_chat_stream(response, on_progress)
}

fn parse_sse_chat_stream(
    response: reqwest::blocking::Response,
    on_progress: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<String, String> {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(response);
    let mut full_text = String::new();
    for line in reader.lines() {
        let line = line.map_err(|err| format!("Error reading chat stream: {err}"))?;
        let data = if let Some(d) = line.strip_prefix("data: ") { d } else { continue };
        if data == "[DONE]" { break; }
        if let Ok(val) = serde_json::from_str::<Value>(data) {
            if let Some(delta) = val.pointer("/choices/0/delta/content").and_then(Value::as_str) {
                full_text.push_str(delta);
                on_progress(full_text.trim())?;
            }
        }
    }
    Ok(full_text.trim().to_string())
}

/// Pinned llama.cpp release used for the bundled llama-server.
/// Update this constant when bumping the bundled runtime version.
const LLAMA_SERVER_RELEASE: &str = "b8987";
// Built from the release tag so bumping LLAMA_SERVER_RELEASE updates the URL too — no risk
// of silently downloading an old binary because one hardcoded tag was missed.
fn llama_server_zip_url() -> String {
    format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{tag}/llama-{tag}-bin-ubuntu-x64.tar.gz",
        tag = LLAMA_SERVER_RELEASE
    )
}

/// Resolve a usable `llama-server` binary.
///
/// Priority:
/// 1. Path configured in Advanced settings.
/// 2. Waylate-managed pinned binary at `<data_dir>/runtime/llama-server-<release>`.
/// 3. Auto-download the pinned release into (2).
/// 4. `llama-server` found in PATH as a last-resort fallback.
pub fn ensure_managed_llama_binary(paths: &AppPaths, config: &AppConfig) -> Result<String, String> {
    // 1. User-configured path.
    let configured = config.local_llama_server_path.trim();
    if !configured.is_empty() {
        if std::path::Path::new(configured).is_file() {
            return Ok(configured.to_string());
        }
        return Err(format!(
            "llama-server binary not found at configured path: {configured}"
        ));
    }

    // 1.5. Vulkan binary for AMD/Intel GPU (no restart needed — llama-server starts on demand).
    if config.vulkan_gpu_enabled {
        let vulkan = crate::gpu_runtime::vulkan_binary_path(paths);
        if crate::gpu_runtime::is_vulkan_installed(paths) {
            return Ok(vulkan.display().to_string());
        }
    }

    // 2. Managed CPU download location.
    let managed = paths
        .data_dir
        .join("runtime")
        .join(format!("llama-server-{LLAMA_SERVER_RELEASE}"));
    if managed_llama_runtime_complete(&managed) {
        return Ok(managed.display().to_string());
    }

    // 3. Auto-download pinned binary.
    if let Ok(downloaded) = download_llama_server_binary(paths, &managed) {
        return Ok(downloaded);
    }

    // 4. System PATH fallback.
    if let Some(path_binary) = resolve_llama_binary_from_path() {
        return Ok(path_binary);
    }

    Err("Could not find or download a compatible llama-server binary.".into())
}

fn resolve_llama_binary_from_path() -> Option<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("command -v llama-server")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    if path.is_empty() {
        None
    } else {
        Some(path.into())
    }
}

fn download_llama_server_binary(
    paths: &AppPaths,
    dest: &std::path::Path,
) -> Result<String, String> {
    let runtime_dir = paths.data_dir.join("runtime");
    fs::create_dir_all(&runtime_dir).map_err(|err| err.to_string())?;

    let zip_url = llama_server_zip_url();
    let archive_path = runtime_dir.join(format!(
        "llama-server-{LLAMA_SERVER_RELEASE}.{}",
        if zip_url.ends_with(".tar.gz") {
            "tar.gz"
        } else {
            "zip"
        }
    ));

    // Download the archive.
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())?;
    let mut response = client
        .get(&zip_url)
        .send()
        .map_err(|err| format!("Could not download llama-server: {err}"))?
        .error_for_status()
        .map_err(|err| format!("llama-server download returned an error: {err}"))?;
    let mut archive_file = File::create(&archive_path)
        .map_err(|err| format!("Could not create llama archive: {err}"))?;
    std::io::copy(&mut response, &mut archive_file)
        .map_err(|err| format!("Could not write llama archive: {err}"))?;
    drop(archive_file);

    if zip_url.ends_with(".tar.gz") {
        extract_llama_runtime_tar_gz(&archive_path, dest)?;
    } else {
        extract_llama_runtime_zip(&archive_path, dest)?;
    }
    let _ = fs::remove_file(&archive_path);

    // Make the binary executable.
    let mut perms = fs::metadata(dest)
        .map_err(|err| err.to_string())?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(dest, perms).map_err(|err| err.to_string())?;

    Ok(dest.display().to_string())
}

fn extract_llama_runtime_zip(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let zip_data = fs::read(archive_path).map_err(|err| err.to_string())?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_data))
        .map_err(|err| format!("Could not open llama runtime zip: {err}"))?;
    let runtime_dir = dest.parent().unwrap_or_else(|| Path::new("."));
    let mut companion_dir = None::<String>;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|err| format!("Could not read zip entry: {err}"))?;
        let name = file.name().to_string();
        if name.ends_with("llama-server") || name == "llama-server" {
            let mut out = File::create(dest)
                .map_err(|err| format!("Could not create llama-server binary: {err}"))?;
            std::io::copy(&mut file, &mut out)
                .map_err(|err| format!("Could not extract llama-server: {err}"))?;
            companion_dir = Path::new(&name)
                .parent()
                .map(|parent| parent.to_string_lossy().to_string());
            break;
        }
    }
    let Some(companion_dir) = companion_dir else {
        return Err("llama-server not found inside the downloaded zip archive.".into());
    };

    let zip_data = fs::read(archive_path).map_err(|err| err.to_string())?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_data))
        .map_err(|err| format!("Could not reopen llama runtime zip: {err}"))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|err| format!("Could not read zip entry: {err}"))?;
        let name = file.name().to_string();
        if !name.starts_with(&companion_dir) {
            continue;
        }
        let Some(filename) = Path::new(&name)
            .file_name()
            .and_then(|value| value.to_str())
        else {
            continue;
        };
        let is_shared_lib =
            filename.starts_with("lib") && (filename.contains(".so") || filename == "mtmd.dll");
        if !is_shared_lib {
            continue;
        }
        let dest_path = runtime_dir.join(filename);
        let mut out = File::create(&dest_path)
            .map_err(|err| format!("Could not create {}: {err}", dest_path.display()))?;
        std::io::copy(&mut file, &mut out)
            .map_err(|err| format!("Could not extract {}: {err}", filename))?;
        let mut perms = fs::metadata(&dest_path)
            .map_err(|err| err.to_string())?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest_path, perms).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn extract_llama_runtime_tar_gz(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let runtime_dir = dest.parent().unwrap_or_else(|| Path::new("."));
    let archive_file = File::open(archive_path).map_err(|err| err.to_string())?;
    let decoder = flate2::read::GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let mut found = false;
    for entry in archive
        .entries()
        .map_err(|err| format!("Could not read llama tar entries: {err}"))?
    {
        let mut entry = entry.map_err(|err| format!("Could not read llama tar entry: {err}"))?;
        let path = entry.path().map_err(|err| err.to_string())?.into_owned();
        let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if filename == "llama-server" {
            let mut out = File::create(dest)
                .map_err(|err| format!("Could not create llama-server binary: {err}"))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|err| format!("Could not extract llama-server: {err}"))?;
            found = true;
            continue;
        }
        let is_shared_lib = filename.starts_with("lib") && filename.contains(".so");
        if !is_shared_lib {
            continue;
        }
        let dest_path = runtime_dir.join(filename);
        let _ = fs::remove_file(&dest_path);
        if entry.header().entry_type().is_symlink() {
            let link_target = entry
                .link_name()
                .map_err(|err| err.to_string())?
                .ok_or_else(|| format!("Missing symlink target for {}", filename))?;
            symlink(&link_target, &dest_path).map_err(|err| {
                format!("Could not create symlink {}: {err}", dest_path.display())
            })?;
            continue;
        }
        let mut out = File::create(&dest_path)
            .map_err(|err| format!("Could not create {}: {err}", dest_path.display()))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|err| format!("Could not extract {}: {err}", filename))?;
        let mut perms = fs::metadata(&dest_path)
            .map_err(|err| err.to_string())?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest_path, perms).map_err(|err| err.to_string())?;
    }
    if !found {
        return Err("llama-server not found inside the downloaded tar.gz archive.".into());
    }
    Ok(())
}

fn managed_llama_runtime_complete(binary: &Path) -> bool {
    if !binary.is_file() {
        return false;
    }
    let runtime_dir = binary.parent().unwrap_or_else(|| Path::new("."));
    let libraries_ready = ["libllama.so", "libllama-common.so.0", "libggml.so.0"]
        .iter()
        .all(|name| {
            let path = runtime_dir.join(name);
            std::fs::symlink_metadata(&path)
                .map(|meta| meta.file_type().is_symlink() || meta.len() > 0)
                .unwrap_or(false)
        });
    libraries_ready && llama_binary_usable(binary)
}

fn managed_llama_library_dir(binary: &str) -> Option<PathBuf> {
    let path = Path::new(binary);
    if !managed_llama_runtime_complete(path) {
        return None;
    }
    path.parent().map(Path::to_path_buf)
}

fn llama_binary_usable(binary: &Path) -> bool {
    let Some(runtime_dir) = binary.parent() else {
        return false;
    };
    let mut command = Command::new(binary);
    command.arg("--version");
    if let Ok(existing) = std::env::var("LD_LIBRARY_PATH") {
        let mut ld_path = runtime_dir.display().to_string();
        if !existing.trim().is_empty() {
            ld_path.push(':');
            ld_path.push_str(&existing);
        }
        command.env("LD_LIBRARY_PATH", ld_path);
    } else {
        command.env("LD_LIBRARY_PATH", runtime_dir.display().to_string());
    }
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_template_replaces_placeholders() {
        let config = AppConfig {
            local_prompt_template: "from {source} to {target}: {text}".into(),
            ..AppConfig::default()
        };
        let rendered = prompt_template(&config, "en", "ru", "hello");
        assert_eq!(rendered, "from en to ru: hello");
    }

    #[test]
    fn timeout_policy_matches_mode() {
        let fast = AppConfig {
            local_model_policy: "fast".into(),
            ..AppConfig::default()
        };
        let balanced = AppConfig {
            local_model_policy: "balanced".into(),
            local_model_idle_timeout_secs: 900,
            ..AppConfig::default()
        };
        let saver = AppConfig {
            local_model_policy: "memory-saver".into(),
            ..AppConfig::default()
        };

        assert_eq!(timeout_for_policy(&fast), None);
        assert_eq!(
            timeout_for_policy(&balanced),
            Some(Duration::from_secs(900))
        );
        assert_eq!(timeout_for_policy(&saver), Some(Duration::from_secs(0)));
    }
}
