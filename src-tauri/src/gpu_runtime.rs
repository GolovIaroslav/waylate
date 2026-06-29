//! GPU runtime acquisition.
//!
//! Downloads and assembles a fully self-contained CUDA onnxruntime bundle into the private
//! `gpu_runtime_dir` so translation can run on the GPU without any system CUDA/cuDNN. This
//! is the backing logic for the opt-in "Accelerate" button. It is heavy (~2.5 GB) and
//! Arch/Linux-specific by design (Waylate targets Arch/KDE).
//!
//! Sources (all proven on an RTX 3060, see NEXT_SESSION.md phase C):
//! - Arch [extra] packages (`.pkg.tar.zst`): onnxruntime-cuda + its transitive deps. Their
//!   SONAMEs drift, so versions are resolved live and grabbed all at once for consistency.
//! - NVIDIA CUDA redist (`.tar.xz`): cudart/cublas/cufft, resolved from a pinned index JSON.
//! - NVIDIA cuDNN 9 redist (`.tar.xz`, cuda13 variant), resolved from a pinned index JSON.
//!
//! Every shared library is flattened into one directory (matching the LD_LIBRARY_PATH the
//! app re-execs with), and `libonnxruntime.so` is what `configure_ort_dylib` loads.

use crate::config::AppPaths;
use crate::engines::onnx_mt::gpu_runtime_dir;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;

// Arch [extra] packages providing onnxruntime-cuda and its transitive runtime deps. Filenames
// are resolved live from the Arch package JSON API so the SONAMEs match the current repo.
const ARCH_PACKAGES: &[&str] = &[
    "onnxruntime-cuda",
    "abseil-cpp",
    "protobuf",
    "nccl",
    "onednn",
    "boost-libs",
];
const ARCH_PKG_API: &str = "https://archlinux.org/packages/extra/x86_64/";
const ARCH_MIRROR: &str = "https://geo.mirror.pkgbuild.com/extra/os/x86_64/";

// NVIDIA CUDA runtime redistributables. The index pins the CUDA 13.2 toolkit; the exact
// component archive paths are read from its JSON.
const CUDA_REDIST_BASE: &str = "https://developer.download.nvidia.com/compute/cuda/redist/";
const CUDA_REDIST_INDEX: &str = "redistrib_13.2.1.json";
const CUDA_COMPONENTS: &[&str] = &["cuda_cudart", "libcublas", "libcufft"];

// NVIDIA cuDNN 9 redistributable (cuda13 build variant). Bump the index if a newer cuDNN is
// required; the cuda13 relative_path is read from its JSON.
const CUDNN_REDIST_BASE: &str = "https://developer.download.nvidia.com/compute/cudnn/redist/";
const CUDNN_REDIST_INDEX: &str = "redistrib_9.14.0.json";

// fp16 NLLB weights — only the GPU path uses these; the CPU path keeps the INT8 model.
const FP16_MODEL_ID: &str = "nllb-200-distilled-600m-onnx";
const FP16_REPO: &str = "Xenova/nllb-200-distilled-600M";
const FP16_FILES: &[(&str, &str)] = &[
    ("onnx/encoder_model_fp16.onnx", "encoder_model_fp16.onnx"),
    (
        "onnx/decoder_model_merged_fp16.onnx",
        "decoder_model_merged_fp16.onnx",
    ),
];

/// True when a usable GPU onnxruntime is already assembled on disk.
pub fn is_installed(paths: &AppPaths) -> bool {
    gpu_runtime_dir(paths).join("libonnxruntime.so").is_file()
}

/// Download and assemble the whole GPU runtime bundle. `progress` reports an overall fraction
/// in `0.0..=1.0` and a human-readable label for whatever is currently downloading.
pub fn download_gpu_runtime(
    paths: &AppPaths,
    progress: &mut dyn FnMut(f64, &str),
) -> Result<(), String> {
    // Already assembled (e.g. the user toggled GPU off then on) — don't re-fetch 2.5 GB.
    if is_installed(paths) {
        progress(1.0, "GPU runtime ready");
        return Ok(());
    }

    let dest = gpu_runtime_dir(paths);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Could not create GPU runtime dir: {err}"))?;
    }
    // Assemble into a sibling staging dir, then atomically rename the flattened libs into
    // place. This keeps `is_installed()` (which only checks `dest`) false until the bundle is
    // complete. A leftover staging dir means a previous run was interrupted — wipe it.
    let mut staging = dest.clone();
    staging.set_extension("staging");
    let _ = fs::remove_dir_all(&staging);
    let libs_out = staging.join("lib");
    fs::create_dir_all(&libs_out).map_err(|err| err.to_string())?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60 * 30))
        .build()
        .map_err(|err| err.to_string())?;

    // Resolve every archive URL up front so a stale index fails fast, before any 2 GB download.
    let mut archives: Vec<(String, String)> = Vec::new(); // (url, label)
    for pkg in ARCH_PACKAGES {
        archives.push((arch_archive_url(&client, pkg)?, format!("{pkg} (Arch)")));
    }
    for (url, label) in cuda_archive_urls(&client)? {
        archives.push((url, label));
    }
    archives.push((cudnn_archive_url(&client)?, "cuDNN 9".to_string()));

    let total = archives.len() as f64;
    for (index, (url, label)) in archives.iter().enumerate() {
        let base = index as f64 / total;
        let span = 1.0 / total;
        progress(base, &format!("Downloading {label}"));

        let archive_name = url.rsplit('/').next().unwrap_or("archive").to_string();
        let archive_path = staging.join(&archive_name);
        download_file(&client, url, &archive_path, &mut |frac| {
            // Reserve the last sliver of each archive's slice for extraction.
            progress(base + frac * span * 0.9, &format!("Downloading {label}"));
        })?;

        progress(base + span * 0.9, &format!("Installing {label}"));
        let extract_dir = staging.join(format!("x{index}"));
        extract_archive(&archive_path, &extract_dir)?;
        flatten_shared_libs(&extract_dir, &libs_out)?;
        let _ = fs::remove_file(&archive_path);
        let _ = fs::remove_dir_all(&extract_dir);
    }

    if !libs_out.join("libonnxruntime.so").is_file() {
        let _ = fs::remove_dir_all(&staging);
        return Err(
            "GPU runtime assembled but libonnxruntime.so is missing — the onnxruntime-cuda \
             package layout may have changed."
                .into(),
        );
    }

    // Swap the fully-assembled libs into the final location, then drop the staging dir.
    let _ = fs::remove_dir_all(&dest);
    fs::rename(&libs_out, &dest)
        .map_err(|err| format!("Could not finalize GPU runtime dir: {err}"))?;
    let _ = fs::remove_dir_all(&staging);

    progress(1.0, "GPU runtime ready");
    Ok(())
}

/// Download the fp16 NLLB weights next to the installed INT8 model. `progress` reports an
/// overall fraction in `0.0..=1.0`.
pub fn download_fp16_model(
    paths: &AppPaths,
    progress: &mut dyn FnMut(f64, &str),
) -> Result<(), String> {
    let dir = paths.models_dir.join(FP16_MODEL_ID);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60 * 30))
        .build()
        .map_err(|err| err.to_string())?;

    let total = FP16_FILES.len() as f64;
    for (index, (remote, local)) in FP16_FILES.iter().enumerate() {
        let base = index as f64 / total;
        let span = 1.0 / total;
        let target = dir.join(local);
        if target.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            continue;
        }
        let url = format!("https://huggingface.co/{FP16_REPO}/resolve/main/{remote}");
        let part = dir.join(format!("{local}.part"));
        download_file(&client, &url, &part, &mut |frac| {
            progress(base + frac * span, &format!("Downloading {local}"));
        })?;
        fs::rename(&part, &target)
            .map_err(|err| format!("Could not finalize {}: {err}", target.display()))?;
    }
    progress(1.0, "GPU model ready");
    Ok(())
}

// ---------------------------------------------------------------------------
// URL resolution
// ---------------------------------------------------------------------------

fn arch_archive_url(client: &reqwest::blocking::Client, pkg: &str) -> Result<String, String> {
    let url = format!("{ARCH_PKG_API}{pkg}/json/");
    let value: Value = client
        .get(&url)
        .send()
        .map_err(|err| format!("Could not reach the Arch package index for {pkg}: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Arch package index returned an error for {pkg}: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse the Arch package index for {pkg}: {err}"))?;
    let filename = value
        .get("filename")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Arch package index for {pkg} has no filename"))?;
    Ok(format!("{ARCH_MIRROR}{filename}"))
}

fn cuda_archive_urls(client: &reqwest::blocking::Client) -> Result<Vec<(String, String)>, String> {
    let url = format!("{CUDA_REDIST_BASE}{CUDA_REDIST_INDEX}");
    let index: Value = client
        .get(&url)
        .send()
        .map_err(|err| format!("Could not reach the CUDA redist index: {err}"))?
        .error_for_status()
        .map_err(|err| format!("CUDA redist index returned an error: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse the CUDA redist index: {err}"))?;
    let mut urls = Vec::new();
    for comp in CUDA_COMPONENTS {
        let rel = index
            .get(comp)
            .and_then(|c| c.get("linux-x86_64"))
            .and_then(|p| p.get("relative_path"))
            .and_then(Value::as_str)
            .ok_or_else(|| format!("CUDA redist index is missing component {comp}"))?;
        urls.push((format!("{CUDA_REDIST_BASE}{rel}"), format!("CUDA {comp}")));
    }
    Ok(urls)
}

fn cudnn_archive_url(client: &reqwest::blocking::Client) -> Result<String, String> {
    let url = format!("{CUDNN_REDIST_BASE}{CUDNN_REDIST_INDEX}");
    let index: Value = client
        .get(&url)
        .send()
        .map_err(|err| format!("Could not reach the cuDNN redist index: {err}"))?
        .error_for_status()
        .map_err(|err| format!("cuDNN redist index returned an error: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse the cuDNN redist index: {err}"))?;
    let rel = index
        .get("cudnn")
        .and_then(|c| c.get("linux-x86_64"))
        .and_then(|p| p.get("cuda13"))
        .and_then(|p| p.get("relative_path"))
        .and_then(Value::as_str)
        .ok_or_else(|| "cuDNN redist index has no cuda13 build".to_string())?;
    Ok(format!("{CUDNN_REDIST_BASE}{rel}"))
}

// ---------------------------------------------------------------------------
// Download + extract
// ---------------------------------------------------------------------------

fn download_file(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let mut response = client
        .get(url)
        .send()
        .map_err(|err| format!("Could not download {url}: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Download returned an error for {url}: {err}"))?;
    let total = response.content_length();
    let mut output =
        File::create(dest).map_err(|err| format!("Could not create {}: {err}", dest.display()))?;
    let mut buffer = [0_u8; 256 * 1024];
    let mut downloaded = 0_u64;
    let mut last_emit = 0_u64;
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|err| format!("Could not read from {url}: {err}"))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|err| format!("Could not write {}: {err}", dest.display()))?;
        downloaded += read as u64;
        // Throttle UI updates: ~every 8 MB is smooth without flooding the event channel.
        if downloaded - last_emit >= 8 * 1024 * 1024 {
            last_emit = downloaded;
            if let Some(total) = total {
                if total > 0 {
                    on_progress((downloaded as f64 / total as f64).clamp(0.0, 1.0));
                }
            }
        }
    }
    // Reject a stream the server closed early — otherwise a truncated file would be renamed
    // into place and later fed to ORT / tar as if it were complete.
    if let Some(total) = total {
        if downloaded != total {
            let _ = fs::remove_file(dest);
            return Err(format!(
                "Download of {url} was incomplete ({downloaded} of {total} bytes) — please retry"
            ));
        }
    }
    Ok(())
}

fn extract_archive(archive: &Path, into: &Path) -> Result<(), String> {
    fs::create_dir_all(into).map_err(|err| err.to_string())?;
    let name = archive
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    // The app is Linux-only; shelling out to system tar avoids pulling in zstd/xz crates.
    let mut command = Command::new("tar");
    if name.ends_with(".zst") {
        command.arg("--use-compress-program=unzstd").arg("-xf");
    } else {
        command.arg("-xJf");
    }
    let status = command
        .arg(archive)
        .arg("-C")
        .arg(into)
        .status()
        .map_err(|err| format!("Could not run tar to extract {name}: {err}"))?;
    if !status.success() {
        return Err(format!("tar failed to extract {name}"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Vulkan llama-server (AMD / Intel GPU)
// ---------------------------------------------------------------------------

/// Pinned Vulkan-enabled llama-server release — keep in sync with LLAMA_SERVER_RELEASE
/// in runtime.rs.
const VULKAN_LLAMA_RELEASE: &str = "b8987";
const VULKAN_LLAMA_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/download/b8987/llama-b8987-bin-ubuntu-vulkan-x64.tar.gz";

/// Path where the Vulkan llama-server binary is stored. Libs land in the parent dir
/// (`<data_dir>/runtime/vulkan/`), separate from the CPU binary's sibling libs.
pub fn vulkan_binary_path(paths: &AppPaths) -> std::path::PathBuf {
    paths
        .data_dir
        .join("runtime")
        .join("vulkan")
        .join(format!("llama-vulkan-{VULKAN_LLAMA_RELEASE}"))
}

/// True when the Vulkan binary and its companion libs are present on disk.
pub fn is_vulkan_installed(paths: &AppPaths) -> bool {
    let binary = vulkan_binary_path(paths);
    if !binary.is_file() {
        return false;
    }
    // Same companion libs as the CPU build; Vulkan backend is statically linked in or
    // provided via the system libvulkan.so.1 that comes with mesa/amdvlk/intel drivers.
    let dir = binary.parent().unwrap();
    ["libllama.so", "libllama-common.so.0", "libggml.so.0"]
        .iter()
        .all(|name| {
            let p = dir.join(name);
            std::fs::symlink_metadata(&p)
                .map(|m| m.file_type().is_symlink() || m.len() > 0)
                .unwrap_or(false)
        })
}

/// Download and extract the Vulkan llama-server binary. `progress` reports `0.0..=1.0`.
pub fn download_vulkan_runtime(
    paths: &AppPaths,
    progress: &mut dyn FnMut(f64, &str),
) -> Result<(), String> {
    if is_vulkan_installed(paths) {
        progress(1.0, "Vulkan runtime ready");
        return Ok(());
    }

    let binary = vulkan_binary_path(paths);
    let dir = binary.parent().unwrap();
    fs::create_dir_all(dir).map_err(|err| format!("Could not create Vulkan runtime dir: {err}"))?;

    let archive = dir.join(format!("llama-vulkan-{VULKAN_LLAMA_RELEASE}.tar.gz"));

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())?;

    progress(0.0, "Downloading Vulkan runtime");
    download_file(&client, VULKAN_LLAMA_URL, &archive, &mut |frac| {
        progress(frac * 0.9, "Downloading Vulkan runtime");
    })?;

    progress(0.9, "Installing Vulkan runtime");
    extract_vulkan_tar_gz(&archive, &binary)?;
    let _ = fs::remove_file(&archive);

    // Make the binary executable.
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&binary)
        .map_err(|err| err.to_string())?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&binary, perms).map_err(|err| err.to_string())?;

    if !is_vulkan_installed(paths) {
        return Err(
            "Vulkan runtime downloaded but companion libraries are missing — \
             the llama.cpp release layout may have changed."
                .into(),
        );
    }
    progress(1.0, "Vulkan runtime ready");
    Ok(())
}

fn extract_vulkan_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let dir = dest.parent().unwrap_or_else(|| Path::new("."));
    let file = File::open(archive).map_err(|err| err.to_string())?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    let mut found = false;
    for entry in tar
        .entries()
        .map_err(|err| format!("Could not read Vulkan tar entries: {err}"))?
    {
        let mut entry = entry.map_err(|err| format!("Could not read Vulkan tar entry: {err}"))?;
        let path = entry.path().map_err(|err| err.to_string())?.into_owned();
        let Some(filename) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if filename == "llama-server" {
            let mut out = File::create(dest)
                .map_err(|err| format!("Could not create Vulkan llama-server binary: {err}"))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|err| format!("Could not extract Vulkan llama-server: {err}"))?;
            found = true;
            continue;
        }
        let is_lib = filename.starts_with("lib") && filename.contains(".so");
        if !is_lib {
            continue;
        }
        let dest_path = dir.join(filename);
        let _ = fs::remove_file(&dest_path);
        if entry.header().entry_type().is_symlink() {
            let link = entry
                .link_name()
                .map_err(|err| err.to_string())?
                .ok_or_else(|| format!("Missing symlink target for {filename}"))?;
            std::os::unix::fs::symlink(&link, &dest_path)
                .map_err(|err| format!("Could not create symlink {}: {err}", dest_path.display()))?;
            continue;
        }
        let mut out = File::create(&dest_path)
            .map_err(|err| format!("Could not create {}: {err}", dest_path.display()))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|err| format!("Could not extract {filename}: {err}"))?;
    }
    if !found {
        return Err("llama-server not found in the Vulkan tar.gz archive.".into());
    }
    Ok(())
}

/// Move every `*.so*` under `root` into `dest` (flat). Symlinks are recreated verbatim so the
/// SONAME chain (libfoo.so -> libfoo.so.1 -> libfoo.so.1.2.3) survives flattening.
fn flatten_shared_libs(root: &Path, dest: &Path) -> Result<usize, String> {
    let mut moved = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries {
            let entry = entry.map_err(|err| err.to_string())?;
            let file_type = entry.file_type().map_err(|err| err.to_string())?;
            let path = entry.path();
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !(name_str.ends_with(".so") || name_str.contains(".so.")) {
                continue;
            }
            let target = dest.join(&name);
            let _ = fs::remove_file(&target);
            if file_type.is_symlink() {
                let link = fs::read_link(&path).map_err(|err| err.to_string())?;
                std::os::unix::fs::symlink(&link, &target)
                    .map_err(|err| format!("Could not link {}: {err}", target.display()))?;
            } else if fs::rename(&path, &target).is_err() {
                fs::copy(&path, &target)
                    .map_err(|err| format!("Could not place {}: {err}", target.display()))?;
            }
            moved += 1;
        }
    }
    Ok(moved)
}
