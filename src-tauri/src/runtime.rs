use crate::{config::{AppConfig, AppPaths}, models::ModelProfile};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeReport {
    pub active_profiles: Vec<RuntimeEntry>,
    pub selected_model_loaded: bool,
    pub selected_device: Option<String>,
    pub ct2_cuda_devices: u32,
    pub llama_binary_found: bool,
    pub llama_cuda_reported: bool,
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
    Ct2Server,
    LlamaServer,
}

impl RuntimeKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Ct2Server => "ct2-server",
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

    pub fn report(&self, paths: &AppPaths, config: &AppConfig) -> RuntimeReport {
        let ct2_cuda_devices = detect_ct2_cuda_devices(paths);
        let llama_binary = resolve_llama_binary(config);
        let llama_cuda_reported = llama_binary
            .as_ref()
            .map(|binary| llama_reports_cuda(binary))
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

        RuntimeReport {
            active_profiles,
            selected_model_loaded,
            selected_device,
            ct2_cuda_devices,
            llama_binary_found: llama_binary.is_some(),
            llama_cuda_reported,
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
        if config.local_model_policy != "fast" {
            return;
        }
        if config.model_id.starts_with("nllb-200") {
            let _ = self.ensure_ct2_server(paths, config, &config.model_id);
        } else if let Some(profile) = crate::models::catalog()
            .into_iter()
            .find(|profile| profile.id == config.model_id && profile.id != "custom-local" && profile.provider == crate::models::ProviderKind::Custom)
        {
            let _ = self.ensure_catalog_llama_server(paths, config, &profile, &config.model_id);
        } else if config.model_id == "custom-local" && config.custom_backend_mode == "managed-gguf" {
            let _ = self.ensure_llama_server(paths, config, &config.model_id);
        }
    }

    pub fn ensure_ct2_server(
        &self,
        paths: &AppPaths,
        config: &AppConfig,
        profile_id: &str,
    ) -> Result<ManagedEndpoint, String> {
        if config.ct2_model_path.trim().is_empty() || config.ct2_tokenizer_path.trim().is_empty() {
            return Err("This model is not installed - Download it in Settings.".into());
        }

        let server = ensure_ct2_server_script(paths)?;
        let signature = format!(
            "{}|{}|{}",
            config.ct2_model_path.trim(),
            config.ct2_tokenizer_path.trim(),
            config.ct2_device.trim()
        );

        if let Some(endpoint) = self.reuse_existing(profile_id, &signature) {
            return Ok(endpoint);
        }

        let port = pick_free_port()?;
        let endpoint = format!("http://127.0.0.1:{port}");
        let logs = runtime_log_files(paths, "ct2-server.log")?;
        let mut child = Command::new(&server)
            .arg("--model")
            .arg(config.ct2_model_path.trim())
            .arg("--tokenizer")
            .arg(config.ct2_tokenizer_path.trim())
            .arg("--device")
            .arg(config.ct2_device.trim())
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::from(logs.0))
            .stderr(Stdio::from(logs.1))
            .spawn()
            .map_err(|err| format!("Could not start warm local runtime: {err}"))?;

        let device = wait_for_ct2_health(&mut child, &endpoint)?;
        let process = ManagedProcess {
            child,
            endpoint: endpoint.clone(),
            profile_id: profile_id.into(),
            kind: RuntimeKind::Ct2Server,
            device_label: device.clone(),
            last_used_at: Instant::now(),
            signature,
        };
        self.insert_process(profile_id, process);
        Ok(ManagedEndpoint { endpoint, device })
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
        let context = profile.managed_context_size.unwrap_or(config.local_context_size);
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
        let signature = format!(
            "{}|{}|{}|{}",
            binary,
            model_path.trim(),
            context_size,
            config.ct2_device.trim()
        );

        if let Some(endpoint) = self.reuse_existing(profile_id, &signature) {
            return Ok(endpoint);
        }

        let port = pick_free_port()?;
        let endpoint = format!("http://127.0.0.1:{port}");
        let mut command = Command::new(&binary);
        command
            .arg("--model")
            .arg(model_path.trim())
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-c")
            .arg(context_size.to_string());
        if config.ct2_device.trim() != "cpu" {
            command.arg("-ngl").arg("99");
        }
        let logs = runtime_log_files(paths, "llama-server.log")?;
        let mut child = command
            .stdout(Stdio::from(logs.0))
            .stderr(Stdio::from(logs.1))
            .spawn()
            .map_err(|err| format!("Could not start managed GGUF runtime: {err}"))?;

        wait_for_http_health(&mut child, &endpoint, Duration::from_secs(60))?;
        let device = if config.ct2_device.trim() == "cpu" {
            "cpu".to_string()
        } else if llama_reports_cuda(&binary) {
            "cuda".to_string()
        } else {
            "auto".to_string()
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
        }
    }

    pub fn shutdown_profile(&self, profile_id: &str) -> Result<(), String> {
        let mut process = {
            let mut processes = self.processes.lock().map_err(|_| "Runtime state is poisoned")?;
            processes.remove(profile_id)
        };
        if let Some(process) = process.as_mut() {
            stop_process(process);
        }
        Ok(())
    }

    pub fn shutdown_all(&self) -> Result<(), String> {
        let mut processes = self.processes.lock().map_err(|_| "Runtime state is poisoned")?;
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

pub fn ensure_ct2_runtime(paths: &AppPaths) -> Result<String, String> {
    let runtime_dir = paths.data_dir.join("runtime");
    let bin_dir = runtime_dir.join("bin");
    let python = bin_dir.join("python");
    let helper = runtime_dir.join("waylate-ct2-translate");
    if !python.exists() {
        fs::create_dir_all(&runtime_dir).map_err(|err| err.to_string())?;
        let output = Command::new("python3")
            .arg("-m")
            .arg("venv")
            .arg(&runtime_dir)
            .output()
            .map_err(|err| format!("Could not create local Python runtime: {err}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
    }

    let deps_ok = Command::new(&python)
        .arg("-c")
        .arg("import ctranslate2, transformers, sentencepiece")
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !deps_ok {
        let output = Command::new(&python)
            .arg("-m")
            .arg("pip")
            .arg("install")
            .arg("--upgrade")
            .arg("ctranslate2")
            .arg("transformers")
            .arg("sentencepiece")
            .output()
            .map_err(|err| format!("Could not install local translation runtime: {err}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
    }

    let source = include_str!("../../scripts/waylate-ct2-translate");
    let body = source.lines().skip(1).collect::<Vec<_>>().join("\n");
    fs::write(&helper, format!("#!{}\n{body}\n", python.display()))
        .map_err(|err| format!("Could not write helper: {err}"))?;
    let mut permissions = fs::metadata(&helper)
        .map_err(|err| err.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&helper, permissions).map_err(|err| err.to_string())?;
    Ok(helper.display().to_string())
}

fn ensure_ct2_server_script(paths: &AppPaths) -> Result<String, String> {
    let runtime_dir = paths.data_dir.join("runtime");
    let bin_dir = runtime_dir.join("bin");
    let python = bin_dir.join("python");
    let server = runtime_dir.join("waylate-ct2-server");
    let _ = ensure_ct2_runtime(paths)?;
    let source = include_str!("../../scripts/waylate-ct2-server");
    let body = source.lines().skip(1).collect::<Vec<_>>().join("\n");
    fs::write(&server, format!("#!{}\n{body}\n", python.display()))
        .map_err(|err| format!("Could not write warm runtime server: {err}"))?;
    let mut permissions = fs::metadata(&server)
        .map_err(|err| err.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&server, permissions).map_err(|err| err.to_string())?;
    Ok(server.display().to_string())
}

fn wait_for_ct2_health(child: &mut Child, endpoint: &str) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_millis(600))
        .build()
        .map_err(|err| err.to_string())?;
    let health = format!("{endpoint}/health");
    for _ in 0..120 {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            return Err(format!("Warm local runtime exited early with status {status}."));
        }
        if let Ok(response) = client.get(&health).send() {
            if let Ok(response) = response.error_for_status() {
                if let Ok(value) = response.json::<Value>() {
                    let device = value
                        .get("device")
                        .and_then(Value::as_str)
                        .unwrap_or("cpu")
                        .to_string();
                    return Ok(device);
                }
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err("Warm local runtime did not become ready in time.".into())
}

fn wait_for_http_health(child: &mut Child, endpoint: &str, timeout: Duration) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|err| err.to_string())?;
    let health = format!("{endpoint}/health");
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            return Err(format!("Managed local runtime exited early with status {status}."));
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
    Err("Managed local runtime did not become ready in time.".into())
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

fn timeout_for_policy(config: &AppConfig) -> Option<Duration> {
    match config.local_model_policy.as_str() {
        "memory-saver" => Some(Duration::from_secs(0)),
        "balanced" => Some(Duration::from_secs(config.local_model_idle_timeout_secs.max(15))),
        _ => None,
    }
}

fn runtime_log_files(paths: &AppPaths, name: &str) -> Result<(File, File), String> {
    let logs_dir = paths.data_dir.join("logs");
    fs::create_dir_all(&logs_dir).map_err(|err| err.to_string())?;
    runtime_log_files_for_path(&logs_dir.join(name))
}

fn runtime_log_files_for_path(path: impl Into<PathBuf>) -> Result<(File, File), String> {
    let path = path.into();
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

fn detect_ct2_cuda_devices(paths: &AppPaths) -> u32 {
    let python = paths.data_dir.join("runtime").join("bin").join("python");
    if !python.exists() {
        return 0;
    }
    Command::new(python)
        .arg("-c")
        .arg("import ctranslate2; print(ctranslate2.get_cuda_device_count())")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(0)
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

pub fn translate_via_warm_ct2(
    manager: &RuntimeManager,
    paths: &AppPaths,
    config: &AppConfig,
    profile_id: &str,
    source: &str,
    target: &str,
    text: &str,
) -> Result<(String, String), String> {
    let endpoint = manager.ensure_ct2_server(paths, config, profile_id)?;
    let (value, device) = match warm_ct2_request(&endpoint.endpoint, source, target, text) {
        Ok(value) => (value, endpoint.device.clone()),
        Err(_) => {
            let _ = manager.shutdown_profile(profile_id);
            match manager.ensure_ct2_server(paths, config, profile_id) {
                Ok(restarted) => match warm_ct2_request(&restarted.endpoint, source, target, text) {
                    Ok(value) => (value, restarted.device),
                    Err(_) => return translate_via_ct2_helper(paths, config, source, target, text),
                },
                Err(_) => return translate_via_ct2_helper(paths, config, source, target, text),
            }
        }
    };

    let translated = value
        .get("translatedText")
        .and_then(Value::as_str)
        .ok_or_else(|| "Warm local runtime response did not contain a translation".to_string())?;
    Ok((translated.trim().into(), device))
}

fn translate_via_ct2_helper(
    paths: &AppPaths,
    config: &AppConfig,
    source: &str,
    target: &str,
    text: &str,
) -> Result<(String, String), String> {
    let helper = if config.ct2_helper_command.trim().is_empty() {
        ensure_ct2_runtime(paths)?
    } else {
        config.ct2_helper_command.trim().to_string()
    };
    let output = Command::new(&helper)
        .arg("--model")
        .arg(config.ct2_model_path.trim())
        .arg("--tokenizer")
        .arg(config.ct2_tokenizer_path.trim())
        .arg("--source")
        .arg(source)
        .arg("--target")
        .arg(target)
        .arg("--device")
        .arg(config.ct2_device.trim())
        .arg(text)
        .output()
        .map_err(|err| format!("Could not start local translator helper: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Local translator helper failed.".into()
        } else {
            stderr
        });
    }
    let translated = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if translated.is_empty() {
        return Err("Local translator helper returned an empty translation.".into());
    }
    let device = if config.ct2_device.trim() == "auto" {
        if detect_ct2_cuda_devices(paths) > 0 {
            "cuda".to_string()
        } else {
            "cpu".to_string()
        }
    } else {
        config.ct2_device.trim().to_string()
    };
    Ok((translated, device))
}

fn warm_ct2_request(endpoint: &str, source: &str, target: &str, text: &str) -> Result<Value, String> {
    Client::new()
        .post(format!("{endpoint}/translate"))
        .json(&json!({
            "text": text,
            "source": source,
            "target": target,
        }))
        .send()
        .map_err(|err| format!("Warm local runtime could not translate text: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Warm local runtime returned an error: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse warm local runtime response: {err}"))
}

pub fn translate_via_managed_llama(
    manager: &RuntimeManager,
    paths: &AppPaths,
    config: &AppConfig,
    profile_id: &str,
    source: &str,
    target: &str,
    text: &str,
) -> Result<(String, String), String> {
    let endpoint = manager.ensure_llama_server(paths, config, profile_id)?;
    let prompt = prompt_template(config, source, target, text);
    let translated = if config.local_prompt_style == "completion" {
        let value: Value = Client::new()
            .post(format!("{}/completion", endpoint.endpoint))
            .json(&json!({
                "prompt": prompt,
                "temperature": 0.1,
                "n_predict": 400
            }))
            .send()
            .map_err(|err| format!("Managed local model could not translate text: {err}"))?
            .error_for_status()
            .map_err(|err| format!("Managed local model returned an error: {err}"))?
            .json()
            .map_err(|err| format!("Could not parse managed local model response: {err}"))?;
        value
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| "Managed local model response did not contain completion text".to_string())?
            .trim()
            .to_string()
    } else {
        let value: Value = Client::new()
            .post(format!("{}/v1/chat/completions", endpoint.endpoint))
            .json(&json!({
                "model": "managed-local-gguf",
                "messages": [
                    {"role": "system", "content": "You are a precise translation engine. Do not explain your answer."},
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0.1,
                "stream": false
            }))
            .send()
            .map_err(|err| format!("Managed local model could not translate text: {err}"))?
            .error_for_status()
            .map_err(|err| format!("Managed local model returned an error: {err}"))?
            .json()
            .map_err(|err| format!("Could not parse managed local model response: {err}"))?;
        value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/choices/0/text").and_then(Value::as_str))
            .ok_or_else(|| "Managed local model response did not contain a translation".to_string())?
            .trim()
            .to_string()
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
) -> Result<(String, String), String> {
    let endpoint = manager.ensure_llama_server_with_model(paths, config, profile_id, model_path, context_size)?;
    let is_completion = matches!(prompt_style, Some(crate::models::PromptStyle::Completion));
    let translated = if is_completion {
        let value: Value = Client::new()
            .post(format!("{}/completion", endpoint.endpoint))
            .json(&json!({"prompt": prompt, "temperature": 0.1, "n_predict": 400}))
            .send()
            .map_err(|err| format!("Local model could not process translation: {err}"))?
            .error_for_status()
            .map_err(|err| format!("Local model returned an error: {err}"))?
            .json()
            .map_err(|err| format!("Could not parse local model response: {err}"))?;
        value
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| "Local model response did not contain completion text".to_string())?
            .trim()
            .to_string()
    } else {
        let value: Value = Client::new()
            .post(format!("{}/v1/chat/completions", endpoint.endpoint))
            .json(&json!({
                "model": "managed-local-gguf",
                "messages": [
                    {"role": "system", "content": "You are a precise translation engine. Do not explain your answer."},
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0.1,
                "stream": false
            }))
            .send()
            .map_err(|err| format!("Local model could not process translation: {err}"))?
            .error_for_status()
            .map_err(|err| format!("Local model returned an error: {err}"))?
            .json()
            .map_err(|err| format!("Could not parse local model response: {err}"))?;
        value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/choices/0/text").and_then(Value::as_str))
            .ok_or_else(|| "Local model response did not contain a translation".to_string())?
            .trim()
            .to_string()
    };
    Ok((translated, endpoint.device))
}

/// Pinned llama.cpp release used for the bundled llama-server.
/// Update this constant when bumping the bundled runtime version.
const LLAMA_SERVER_RELEASE: &str = "b5616";
const LLAMA_SERVER_ZIP_URL: &str =
    "https://github.com/ggerganov/llama.cpp/releases/download/b5616/llama-b5616-bin-ubuntu-x64.zip";
/// Path inside the zip archive where `llama-server` lives.
const LLAMA_SERVER_ZIP_ENTRY: &str = "build/bin/llama-server";

/// Resolve a usable `llama-server` binary.
///
/// Priority:
/// 1. Path configured in Advanced settings.
/// 2. Waylate-managed download at `<data_dir>/runtime/llama-server-<release>`.
/// 3. `llama-server` found in PATH.
/// 4. Auto-download from the pinned release URL and install to (2).
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

    // 2. Managed download location.
    let managed = paths
        .data_dir
        .join("runtime")
        .join(format!("llama-server-{LLAMA_SERVER_RELEASE}"));
    if managed.is_file() {
        return Ok(managed.display().to_string());
    }

    // 3. System PATH.
    if let Some(path_binary) = resolve_llama_binary_from_path() {
        return Ok(path_binary);
    }

    // 4. Auto-download.
    download_llama_server_binary(paths, &managed)
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
    if path.is_empty() { None } else { Some(path.into()) }
}

fn download_llama_server_binary(paths: &AppPaths, dest: &std::path::Path) -> Result<String, String> {
    let runtime_dir = paths.data_dir.join("runtime");
    fs::create_dir_all(&runtime_dir).map_err(|err| err.to_string())?;

    let zip_path = runtime_dir.join(format!("llama-server-{LLAMA_SERVER_RELEASE}.zip"));

    // Download the zip.
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())?;
    let mut response = client
        .get(LLAMA_SERVER_ZIP_URL)
        .send()
        .map_err(|err| format!("Could not download llama-server: {err}"))?
        .error_for_status()
        .map_err(|err| format!("llama-server download returned an error: {err}"))?;
    let mut zip_file = File::create(&zip_path)
        .map_err(|err| format!("Could not create zip file: {err}"))?;
    std::io::copy(&mut response, &mut zip_file)
        .map_err(|err| format!("Could not write zip file: {err}"))?;
    drop(zip_file);

    // Extract llama-server from the zip.
    let zip_data = fs::read(&zip_path).map_err(|err| err.to_string())?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_data))
        .map_err(|err| format!("Could not open zip: {err}"))?;
    let entry_name = LLAMA_SERVER_ZIP_ENTRY;
    let mut found = false;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|err| format!("Could not read zip entry: {err}"))?;
        if file.name() == entry_name {
            let mut out = File::create(dest)
                .map_err(|err| format!("Could not create llama-server binary: {err}"))?;
            std::io::copy(&mut file, &mut out)
                .map_err(|err| format!("Could not extract llama-server: {err}"))?;
            found = true;
            break;
        }
    }
    let _ = fs::remove_file(&zip_path);
    if !found {
        // Try alternate entry paths used in older releases (binary at root).
        let zip_data2 = fs::read(&zip_path).unwrap_or_default();
        if !zip_data2.is_empty() {
            let mut archive2 = zip::ZipArchive::new(std::io::Cursor::new(zip_data2))
                .map_err(|err| format!("Could not reopen zip: {err}"))?;
            for i in 0..archive2.len() {
                let mut file = archive2
                    .by_index(i)
                    .map_err(|err| format!("Could not read zip entry: {err}"))?;
                let name = file.name().to_string();
                if name.ends_with("llama-server") || name == "llama-server" {
                    let mut out = File::create(dest)
                        .map_err(|err| format!("Could not create llama-server binary: {err}"))?;
                    std::io::copy(&mut file, &mut out)
                        .map_err(|err| format!("Could not extract llama-server: {err}"))?;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return Err(format!(
                "llama-server not found inside the downloaded archive at '{entry_name}'. \
                 The release format may have changed — set a custom path in Advanced settings."
            ));
        }
    }

    // Make the binary executable.
    let mut perms = fs::metadata(dest)
        .map_err(|err| err.to_string())?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(dest, perms).map_err(|err| err.to_string())?;

    Ok(dest.display().to_string())
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
        assert_eq!(timeout_for_policy(&balanced), Some(Duration::from_secs(900)));
        assert_eq!(timeout_for_policy(&saver), Some(Duration::from_secs(0)));
    }
}
