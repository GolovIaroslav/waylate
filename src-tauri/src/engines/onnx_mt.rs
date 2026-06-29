use crate::{
    config::{AppConfig, AppPaths},
    models::{EngineKind, ModelCatalogEntry},
};
use ort::{
    execution_providers::{CUDAExecutionProvider, ROCmExecutionProvider},
    session::{builder::GraphOptimizationLevel, Session},
    value::{DynValue, Tensor, TensorElementType, ValueType},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
};
use tokenizers::Tokenizer;

static MODEL_CACHE: OnceLock<Mutex<HashMap<String, LoadedOnnxModel>>> = OnceLock::new();

/// Set once at startup by `configure_ort_dylib` when the GPU onnxruntime is the chosen
/// library. The model loader reads it to prefer the fp16 weights (which the CUDA EP runs
/// on tensor cores) over the INT8 weights used on CPU.
static GPU_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn active_device(profile_id: &str) -> Option<String> {
    MODEL_CACHE
        .get()?
        .lock()
        .ok()?
        .get(profile_id)
        .map(|m| m.device.clone())
}

pub fn preload(entry: &ModelCatalogEntry, paths: &AppPaths) {
    let model_dir = paths.models_dir.join(&entry.id);
    if !model_dir.is_dir() {
        return;
    }
    let cache = MODEL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut cache) = cache.lock() {
        if !cache.contains_key(&entry.id) {
            if let Ok(loaded) = LoadedOnnxModel::load(entry, &model_dir) {
                cache.insert(entry.id.clone(), loaded);
            }
        }
    }
}

pub fn unload(profile_id: &str) {
    if let Some(cache) = MODEL_CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.remove(profile_id);
        }
    }
}

/// Private directory holding the downloaded GPU onnxruntime (libonnxruntime.so + CUDA
/// provider libs). Kept out of the system so it never clashes with the OS onnxruntime.
pub fn gpu_runtime_dir(paths: &AppPaths) -> PathBuf {
    paths.data_dir.join("runtime").join("onnxruntime-cuda")
}

/// Point ORT at a concrete onnxruntime shared library via `ORT_DYLIB_PATH`, before any
/// ORT call is made (model preload). With the `load-dynamic` feature ort resolves its
/// library exactly once per process, so this must run first and switching CPU<->GPU
/// requires an app restart.
///
/// Priority: explicit env override (dev) → GPU runtime when opted in and present →
/// bundled CPU lib next to the binary → system-installed lib (dev fallback). Returns the
/// chosen path for logging, or None when no library was found.
pub fn configure_ort_dylib(paths: &AppPaths, config: &AppConfig) -> Option<String> {
    // Respect an explicit override (debugging / unusual setups) without touching it.
    if let Ok(existing) = std::env::var("ORT_DYLIB_PATH") {
        if !existing.trim().is_empty() {
            return Some(existing);
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    // GPU runtime — only when the user opted in and it is actually installed.
    if config.gpu_enabled {
        candidates.push(gpu_runtime_dir(paths).join("libonnxruntime.so"));
    }

    // Bundled CPU library shipped next to the executable (deb/rpm/AppImage).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("libonnxruntime.so"));
        }
    }
    // Managed CPU copy, then the system library (dev fallback).
    candidates.push(
        paths
            .data_dir
            .join("runtime")
            .join("onnxruntime-cpu")
            .join("libonnxruntime.so"),
    );
    candidates.push(PathBuf::from("/usr/lib/libonnxruntime.so"));

    let chosen = candidates.into_iter().find(|p| p.is_file())?;
    // Remember whether we are running on the GPU runtime so the model loader can pick the
    // matching (fp16) weights. The GPU library lives under gpu_runtime_dir.
    let on_gpu = config.gpu_enabled && chosen.starts_with(gpu_runtime_dir(paths));
    GPU_ACTIVE.store(on_gpu, Ordering::Relaxed);
    unsafe { std::env::set_var("ORT_DYLIB_PATH", &chosen) };
    Some(chosen.display().to_string())
}

pub fn translate_with_progress(
    paths: &AppPaths,
    entry: &ModelCatalogEntry,
    text: &str,
    source_lang: &str,
    target_lang: &str,
    on_progress: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<String, String> {
    if entry.engine != EngineKind::OnnxEncoderDecoder {
        return Err("Model is not backed by the ONNX engine.".into());
    }
    if source_lang == "auto" {
        return Err(
            "Choose the source language for local ONNX translation. Auto-detect is not available yet."
                .into(),
        );
    }

    let model_dir = paths.models_dir.join(&entry.id);
    if !model_dir.is_dir() {
        return Err("This model is not installed — Download it in Settings.".into());
    }

    let cache = MODEL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().map_err(|_| "ONNX model cache is poisoned")?;
    if !cache.contains_key(&entry.id) {
        let loaded = LoadedOnnxModel::load(entry, &model_dir)?;
        cache.insert(entry.id.clone(), loaded);
    }
    let model = cache
        .get_mut(&entry.id)
        .ok_or_else(|| "ONNX model cache lost the loaded model unexpectedly".to_string())?;
    model.translate(text, source_lang, target_lang, on_progress)
}

// ---------------------------------------------------------------------------
// Model metadata precomputed at load time
// ---------------------------------------------------------------------------

struct KvInputMeta {
    name: String,
    is_encoder: bool,
    initial_dims: Vec<usize>,
    initial_ty: TensorElementType,
}

struct LoadedOnnxModel {
    tokenizer: Tokenizer,
    encoder: Session,
    /// Merged decoder (handles both step 0 and step 1+ via use_cache_branch).
    decoder: Session,
    eos_token_id: i64,
    device: String,
    hidden_size: usize,
    has_use_cache_branch: bool,
    kv_inputs: Vec<KvInputMeta>,
}

// ---------------------------------------------------------------------------
// Per-translation KV cache
//
// Self-attention KV grows by one position each step. We store these as
// Option<DynValue> and MOVE them into decoder inputs rather than cloning —
// eliminating the O(N²) Rust-side memcpy that made the old code slow.
//
// Encoder cross-attention KV is fixed-size (doesn't grow). We keep it as
// Vec<f32> and re-upload it each step. The copy is O(src_len) — constant.
// ---------------------------------------------------------------------------

struct CacheTensor {
    shape: Vec<usize>,
    data: Vec<f32>,
}

struct KvCache {
    /// Encoder cross-attention KV — fixed size, re-uploaded from Vec<f32> each step.
    encoder: HashMap<String, CacheTensor>,
    /// Self-attention KV — moved into decoder inputs each step, no data copy.
    self_attn: HashMap<String, Option<DynValue>>,
}

impl LoadedOnnxModel {
    // INT8 weights run on the CPU EP at opt=All (the proven fast path); CPU-first ordering.
    const INT8_ENCODER: &[&str] = &[
        "encoder_model_quantized.onnx",
        "encoder_model_int8.onnx",
        "encoder_model.onnx",
    ];
    const INT8_DECODER: &[&str] = &[
        "decoder_model_merged_quantized.onnx",
        "decoder_model_merged_int8.onnx",
        "decoder_model_merged.onnx",
    ];
    // fp16 weights run on the CUDA EP (tensor cores). They cannot load on the CPU EP at
    // opt=All, so the CPU fallback below switches back to INT8, not just to the CPU device.
    const FP16_ENCODER: &[&str] = &[
        "encoder_model_fp16.onnx",
        "encoder_model_quantized.onnx",
        "encoder_model_int8.onnx",
        "encoder_model.onnx",
    ];
    const FP16_DECODER: &[&str] = &[
        "decoder_model_merged_fp16.onnx",
        "decoder_model_merged_quantized.onnx",
        "decoder_model_merged_int8.onnx",
        "decoder_model_merged.onnx",
    ];

    fn load(entry: &ModelCatalogEntry, model_dir: &Path) -> Result<Self, String> {
        if GPU_ACTIVE.load(Ordering::Relaxed) {
            match Self::load_variant(
                entry,
                model_dir,
                Self::FP16_ENCODER,
                Self::FP16_DECODER,
                false,
            ) {
                Ok(model) => return Ok(model),
                // Usually CUDA out-of-memory on a small GPU. Rather than break translation,
                // fall back to INT8 forced onto the CPU EP (the always-works path).
                Err(err) => eprintln!(
                    "[onnx] GPU model load failed ({err}); falling back to CPU INT8 \
                     (free VRAM or disable GPU to silence this)"
                ),
            }
            return Self::load_variant(
                entry,
                model_dir,
                Self::INT8_ENCODER,
                Self::INT8_DECODER,
                true,
            );
        }
        Self::load_variant(entry, model_dir, Self::INT8_ENCODER, Self::INT8_DECODER, false)
    }

    fn load_variant(
        entry: &ModelCatalogEntry,
        model_dir: &Path,
        encoder_candidates: &[&str],
        decoder_candidates: &[&str],
        force_cpu: bool,
    ) -> Result<Self, String> {
        let encoder_path = required_file(model_dir, encoder_candidates)?;
        let decoder_path = required_file(model_dir, decoder_candidates)?;
        let tokenizer_path = required_file(model_dir, &["tokenizer.json"])?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|err| format!("Could not load tokenizer for {}: {err}", entry.name))?;

        let intra = intra_op_threads();
        let (encoder, device) = load_session(&encoder_path, intra, force_cpu)
            .map_err(|err| format!("Could not load encoder model: {err}"))?;
        let (decoder, _) = load_session(&decoder_path, intra, force_cpu)
            .map_err(|err| format!("Could not load decoder model: {err}"))?;

        let eos_token_id = token_id(&tokenizer, "</s>")?;

        let has_use_cache_branch = decoder
            .inputs()
            .iter()
            .any(|o| o.name() == "use_cache_branch");

        let mut hidden_size: usize = 0;
        let mut kv_inputs: Vec<KvInputMeta> = Vec::new();

        for outlet in decoder.inputs() {
            let name = outlet.name().to_string();
            match outlet.dtype() {
                ValueType::Tensor { ty, shape, .. } => {
                    if name == "encoder_hidden_states" {
                        if let Some(&dim) = shape.as_ref().last() {
                            if dim > 0 {
                                hidden_size = dim as usize;
                            }
                        }
                    }
                    if name.contains("past_key_values") {
                        let is_encoder = name.contains(".encoder.");
                        let initial_dims = concrete_initial_cache_dims(shape.as_ref());
                        kv_inputs.push(KvInputMeta {
                            name,
                            is_encoder,
                            initial_dims,
                            initial_ty: *ty,
                        });
                    }
                }
                _ => {}
            }
        }

        if hidden_size == 0 {
            return Err(
                "Could not determine encoder hidden size from decoder model spec".into(),
            );
        }

        Ok(Self {
            tokenizer,
            encoder,
            decoder,
            eos_token_id,
            device,
            hidden_size,
            has_use_cache_branch,
            kv_inputs,
        })
    }

    fn translate(
        &mut self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
        on_progress: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Result<String, String> {
        let source_lang_id = token_id(&self.tokenizer, source_lang)?;
        let target_lang_id = token_id(&self.tokenizer, target_lang)?;

        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|err| format!("Could not tokenize source text: {err}"))?;

        let mut encoder_input_ids: Vec<i64> =
            encoding.get_ids().iter().map(|id| i64::from(*id)).collect();
        encoder_input_ids.push(self.eos_token_id);
        encoder_input_ids.push(source_lang_id);
        let encoder_seq_len = encoder_input_ids.len();
        let encoder_attention_mask = vec![1_i64; encoder_seq_len];

        let encoder_hidden_states =
            self.run_encoder(&encoder_input_ids, &encoder_attention_mask)?;

        let mut decoder_tokens = vec![self.eos_token_id, target_lang_id];
        let mut kv = KvCache {
            encoder: HashMap::new(),
            self_attn: HashMap::new(),
        };

        for step in 0..512_usize {
            let use_cache = step > 0;
            let next_token = self.decode_next_token(
                &decoder_tokens,
                &encoder_attention_mask,
                &encoder_hidden_states,
                encoder_seq_len,
                &mut kv,
                use_cache,
            )?;
            if next_token == self.eos_token_id {
                break;
            }
            decoder_tokens.push(next_token);
            if step % 3 == 0 {
                let partial = self.decode_generated_tokens(&decoder_tokens)?;
                if !partial.is_empty() {
                    on_progress(&partial)?;
                }
            }
        }

        self.decode_generated_tokens(&decoder_tokens)
    }

    fn run_encoder(
        &mut self,
        input_ids: &[i64],
        attention_mask: &[i64],
    ) -> Result<Vec<f32>, String> {
        let mut inputs = HashMap::<String, DynValue>::new();
        inputs.insert("input_ids".into(), mk_i64_tensor_1d_as_2d(input_ids)?);
        inputs.insert(
            "attention_mask".into(),
            mk_i64_tensor_1d_as_2d(attention_mask)?,
        );

        let outputs = self
            .encoder
            .run(inputs)
            .map_err(|err| format!("ONNX encoder inference failed: {err}"))?;

        // Find the hidden-state key without keeping a borrow past this point.
        let hs_key: String = if outputs.contains_key("last_hidden_state") {
            "last_hidden_state".to_string()
        } else {
            outputs
                .keys()
                .next()
                .map(|k| { let s: &str = k.as_ref(); s.to_string() })
                .ok_or_else(|| "ONNX encoder returned no outputs".to_string())?
        };

        let hidden = outputs
            .get(&hs_key)
            .ok_or_else(|| "ONNX encoder hidden state key disappeared".to_string())?;
        let array = hidden
            .try_extract_array::<f32>()
            .map_err(|err| format!("Could not read encoder output: {err}"))?;
        Ok(array.iter().copied().collect())
    }

    fn decode_next_token(
        &mut self,
        decoder_tokens: &[i64],
        encoder_attention_mask: &[i64],
        encoder_hidden_states: &[f32],
        encoder_seq_len: usize,
        kv: &mut KvCache,
        use_cache: bool,
    ) -> Result<i64, String> {
        let decoder_input_ids: Vec<i64> = if use_cache {
            vec![*decoder_tokens
                .last()
                .ok_or_else(|| "Decoder input is empty".to_string())?]
        } else {
            decoder_tokens.to_vec()
        };

        let mut inputs = HashMap::<String, DynValue>::new();

        inputs.insert("input_ids".into(), mk_i64_tensor_1d_as_2d(&decoder_input_ids)?);
        inputs.insert(
            "encoder_attention_mask".into(),
            mk_i64_tensor_1d_as_2d(encoder_attention_mask)?,
        );
        // Merged decoder needs encoder_hidden_states and use_cache_branch.
        inputs.insert(
            "encoder_hidden_states".into(),
            mk_f32_tensor_3d(encoder_seq_len, self.hidden_size, encoder_hidden_states)?,
        );
        if self.has_use_cache_branch {
            inputs.insert(
                "use_cache_branch".into(),
                Tensor::from_array(([1_usize], vec![use_cache]))
                    .map_err(|e| format!("Could not build use_cache_branch tensor: {e}"))?
                    .upcast()
                    .into(),
            );
        }

        // Build KV cache inputs.
        //
        // Self-attention: MOVE the DynValue out of the Option slot (swap to None).
        // The previous step's ORT tensor is directly re-used as this step's input
        // without any Vec<f32> extraction — the main perf optimisation.
        //
        // Encoder cross-attention: fixed size (doesn't grow), re-uploaded from Vec<f32>.
        for meta in &self.kv_inputs {
            let tensor: DynValue = if meta.is_encoder {
                // Fixed-size encoder cross-attn — re-upload from Vec<f32>.
                if let Some(ct) = kv.encoder.get(&meta.name) {
                    mk_f32_tensor_dyn(&ct.shape, &ct.data)?
                } else {
                    initial_cache_tensor(&meta.initial_dims, meta.initial_ty)?
                }
            } else {
                // Growing self-attn — take() moves the DynValue without copying data.
                match kv.self_attn.get_mut(&meta.name) {
                    Some(slot) if slot.is_some() => slot.take().unwrap(),
                    _ => initial_cache_tensor(&meta.initial_dims, meta.initial_ty)?,
                }
            };
            inputs.insert(meta.name.clone(), tensor);
        }

        let outputs = self
            .decoder
            .run(inputs)
            .map_err(|err| format!("ONNX decoder inference failed: {err}"))?;

        // Phase 1: compute argmax (borrows outputs; borrow released at end of this block).
        let best_id: i64 = {
            let logits_key: String = if outputs.contains_key("logits") {
                "logits".to_string()
            } else {
                outputs
                    .keys()
                    .next()
                    .map(|k| { let s: &str = k.as_ref(); s.to_string() })
                    .ok_or_else(|| "ONNX decoder returned no outputs".to_string())?
            };
            let logits_val = outputs
                .get(&logits_key)
                .ok_or_else(|| "ONNX decoder logits disappeared".to_string())?;
            let logits = logits_val
                .try_extract_array::<f32>()
                .map_err(|err| format!("Could not read decoder logits: {err}"))?;
            let shape = logits.shape();
            if shape.len() != 3 {
                return Err(format!("Unexpected decoder logits rank: {}", shape.len()));
            }
            let vocab = *shape
                .last()
                .ok_or_else(|| "Decoder logits had no vocabulary dimension".to_string())?;
            let start = logits.len().saturating_sub(vocab);
            let mut best = 0_i64;
            let mut best_score = f32::NEG_INFINITY;
            for (idx, &score) in logits.iter().skip(start).enumerate() {
                if score > best_score {
                    best_score = score;
                    best = idx as i64;
                }
            }
            best
        }; // borrow on `outputs` released here

        // Phase 2: update KV cache from outputs (consumes outputs — no extra allocation).
        //
        // Self-attn: store new DynValue in Option slot (moved, not copied).
        // Encoder cross-attn: extract to Vec<f32> once (only on step 0, use_cache=false).
        //   - Merged decoder step 0 (use_cache=false): outputs present.*.encoder.* → store.
        //   - Merged decoder step 1+ (use_cache=true): outputs empty Constants → skip.
        for (k, val) in outputs {
            let k_str: &str = k.as_ref();
            let Some(indexed) = k_str.strip_prefix("present.") else {
                continue;
            };
            let input_name = format!("past_key_values.{indexed}");
            let is_encoder = input_name.contains(".encoder.");

            // Merged decoder on step 1+ emits empty Constant tensors for encoder KV — skip.
            if is_encoder && use_cache {
                continue;
            }

            if is_encoder {
                // Extract to Vec<f32> — this only happens once (step 0, merged decoder).
                let array = val
                    .try_extract_array::<f32>()
                    .map_err(|err| format!("Could not read encoder cache {k_str}: {err}"))?;
                if array.is_empty() {
                    continue;
                }
                kv.encoder.insert(
                    input_name,
                    CacheTensor {
                        shape: array.shape().to_vec(),
                        data: array.iter().copied().collect(),
                    },
                );
            } else {
                // Self-attn: move the DynValue directly into our cache.
                // No data extraction needed — ORT tensor reused without copying.
                kv.self_attn.insert(input_name, Some(val));
            }
        }

        Ok(best_id)
    }

    fn decode_generated_tokens(&self, decoder_tokens: &[i64]) -> Result<String, String> {
        let generated: Vec<u32> = decoder_tokens
            .iter()
            .copied()
            .skip(2)
            .filter_map(|id| u32::try_from(id).ok())
            .collect();
        self.tokenizer
            .decode(&generated, true)
            .map(|text| text.trim().to_string())
            .map_err(|err| format!("Could not decode translated tokens: {err}"))
    }
}

// ---------------------------------------------------------------------------
// Tensor helpers
// ---------------------------------------------------------------------------

/// i64 1-D slice → [1, N] tensor (batch dimension prepended).
fn mk_i64_tensor_1d_as_2d(data: &[i64]) -> Result<DynValue, String> {
    Tensor::from_array(([1_usize, data.len()], data.to_vec()))
        .map(|t| t.upcast().into())
        .map_err(|e| format!("Could not build i64 tensor: {e}"))
}

/// f32 flat slice → [1, seq, hidden] tensor.
fn mk_f32_tensor_3d(seq: usize, hidden: usize, data: &[f32]) -> Result<DynValue, String> {
    Tensor::from_array(([1_usize, seq, hidden], data.to_vec()))
        .map(|t| t.upcast().into())
        .map_err(|e| format!("Could not build f32 [1,seq,hidden] tensor: {e}"))
}

/// f32 flat slice → dynamic-shape tensor (for encoder cross-attn KV re-upload).
fn mk_f32_tensor_dyn(shape: &[usize], data: &[f32]) -> Result<DynValue, String> {
    Tensor::from_array((shape.to_vec(), data.to_vec()))
        .map(|t| t.upcast().into())
        .map_err(|e| format!("Could not build f32 tensor: {e}"))
}

fn initial_cache_tensor(dims: &[usize], ty: TensorElementType) -> Result<DynValue, String> {
    let count = element_count(dims);
    match ty {
        TensorElementType::Float32 => Tensor::from_array((dims.to_vec(), vec![0_f32; count]))
            .map(|t| t.upcast().into())
            .map_err(|e| format!("Could not build zero f32 cache tensor: {e}")),
        TensorElementType::Float16 => Err(
            "Unsupported float16 cache tensor — use a float32 or int8 quantised model".into(),
        ),
        TensorElementType::Int64 => Tensor::from_array((dims.to_vec(), vec![0_i64; count]))
            .map(|t| t.upcast().into())
            .map_err(|e| format!("Could not build zero i64 cache tensor: {e}")),
        TensorElementType::Bool => {
            Tensor::from_array((dims.to_vec(), vec![false; count.max(1)]))
                .map(|t| t.upcast().into())
                .map_err(|e| format!("Could not build zero bool cache tensor: {e}"))
        }
        other => Err(format!("Unsupported decoder cache tensor type {other:?}")),
    }
}

fn concrete_initial_cache_dims(shape: &[i64]) -> Vec<usize> {
    shape
        .iter()
        .map(|dim| if *dim > 0 { *dim as usize } else { 1 })
        .collect()
}

fn element_count(dims: &[usize]) -> usize {
    if dims.is_empty() {
        1
    } else {
        dims.iter().product()
    }
}

// ---------------------------------------------------------------------------
// Session loading
// ---------------------------------------------------------------------------

fn required_file(model_dir: &Path, candidates: &[&str]) -> Result<PathBuf, String> {
    candidates
        .iter()
        .map(|name| model_dir.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            format!(
                "Model files are incomplete in {}. Expected one of: {}",
                model_dir.display(),
                candidates.join(", ")
            )
        })
}

fn token_id(tokenizer: &Tokenizer, token: &str) -> Result<i64, String> {
    tokenizer
        .token_to_id(token)
        .map(i64::from)
        .ok_or_else(|| format!("Tokenizer does not define token '{token}'"))
}

fn intra_op_threads() -> usize {
    // Optional override for unusual hardware / support cases. Benchmarking is the only
    // reliable way to tune this, so we leave an escape hatch but never advertise it.
    if let Ok(v) = std::env::var("WAYLATE_ONNX_INTRA") {
        if let Ok(n) = v.parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    // Benchmarked on a Ryzen 5800H (8 cores / 16 threads): ~4 intra-op threads is as
    // fast as saturating all 16, while the previous default of logical/2 (=8) was
    // measurably the *slowest* point. NLLB INT8 decode is memory-bound here, so piling
    // on threads only adds contention. Use ~a quarter of logical cores, which both hits
    // the fast zone and leaves the rest of the machine responsive for a background app.
    let logical = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (logical / 4).clamp(2, 8)
}

fn load_session(path: &Path, intra: usize, force_cpu: bool) -> Result<(Session, String), String> {
    // Only attempt the GPU execution provider that matches the detected hardware.
    // Blindly trying CUDA then ROCm on every load spams the log with two guaranteed
    // failures on the common CPU-only build; route by vendor instead. `force_cpu` skips the
    // GPU EP entirely (the GPU-load fallback, e.g. after CUDA OOM) so INT8 loads on the CPU EP.
    if !force_cpu {
        match crate::runtime::detect_gpu().0.as_deref() {
            Some("nvidia") => match load_session_cuda(path, intra) {
                Ok(session) => return Ok((session, "cuda".into())),
                Err(err) => eprintln!("[onnx] CUDA EP unavailable, using CPU: {err}"),
            },
            Some("amd") => match load_session_rocm(path, intra) {
                Ok(session) => return Ok((session, "rocm".into())),
                Err(err) => eprintln!("[onnx] ROCm EP unavailable, using CPU: {err}"),
            },
            _ => {}
        }
    }
    let session = load_session_cpu(path, intra).map_err(|err| err.to_string())?;
    Ok((session, "cpu".into()))
}

fn load_session_cuda(path: &Path, intra: usize) -> Result<Session, ort::Error> {
    Session::builder()?
        .with_execution_providers([CUDAExecutionProvider::default().build().error_on_failure()])?
        .with_optimization_level(GraphOptimizationLevel::All)?
        .with_intra_threads(intra)?
        .with_inter_threads(1)?
        .with_parallel_execution(false)?
        .with_memory_pattern(false)?
        .commit_from_file(path)
}

fn load_session_rocm(path: &Path, intra: usize) -> Result<Session, ort::Error> {
    Session::builder()?
        .with_execution_providers([ROCmExecutionProvider::default().build()])?
        .with_optimization_level(GraphOptimizationLevel::All)?
        .with_intra_threads(intra)?
        .with_inter_threads(1)?
        .with_parallel_execution(false)?
        .with_memory_pattern(false)?
        .commit_from_file(path)
}

fn load_session_cpu(path: &Path, intra: usize) -> Result<Session, ort::Error> {
    Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::All)?
        .with_intra_threads(intra)?
        .with_inter_threads(1)?
        .with_parallel_execution(false)?
        .with_memory_pattern(false)?
        .commit_from_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_nllb_smoke_when_enabled() {
        if std::env::var("WAYLATE_ONNX_SMOKE").ok().as_deref() != Some("1") {
            return;
        }

        let paths = AppPaths::new().expect("app paths should resolve");
        // Exercise the real library-resolution path the app uses on startup so the
        // benchmark loads through ORT_DYLIB_PATH exactly like production does.
        configure_ort_dylib(&paths, &AppConfig::default());
        let entry = crate::models::model_catalog()
            .into_iter()
            .find(|entry| entry.id == "nllb-200-distilled-600m-onnx")
            .expect("nllb entry exists");
        let model_dir = paths.models_dir.join(&entry.id);
        let mut model = LoadedOnnxModel::load(&entry, &model_dir).expect("model should load");

        // A fixed, sentence-length input gives a deterministic token count so timing
        // across thread/mem-pattern settings is comparable run to run.
        let source = "The weather is nice today and we are going to the park to play.";
        let mut token_count = 0_usize;
        let t0 = std::time::Instant::now();
        let translated = model
            .translate(source, "eng_Latn", "rus_Cyrl", &mut |partial| {
                token_count = partial.split_whitespace().count();
                Ok(())
            })
            .expect("translation should succeed");
        let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(!translated.trim().is_empty());
        eprintln!(
            "[bench] device={} intra={} total={:.0}ms words~{} translation={}",
            model.device,
            intra_op_threads(),
            total_ms,
            token_count,
            translated
        );
    }

    /// GPU/fp16 smoke benchmark. Loads the fp16 NLLB export from WAYLATE_ONNX_FP16_DIR
    /// (which must hold encoder_model_fp16.onnx, decoder_model_merged_fp16.onnx,
    /// tokenizer.json) and reports the device it ran on. Point ORT_DYLIB_PATH at the GPU
    /// onnxruntime and set LD_LIBRARY_PATH for CUDA + cuDNN to exercise the CUDA EP.
    #[test]
    fn local_nllb_fp16_smoke_when_enabled() {
        if std::env::var("WAYLATE_ONNX_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(dir) = std::env::var("WAYLATE_ONNX_FP16_DIR") else {
            eprintln!("[onnx] WAYLATE_ONNX_FP16_DIR not set — skipping fp16 smoke");
            return;
        };
        let model_dir = std::path::PathBuf::from(dir);
        let entry = crate::models::model_catalog()
            .into_iter()
            .find(|entry| entry.id == "nllb-200-distilled-600m-onnx")
            .expect("nllb entry exists");
        let mut model = LoadedOnnxModel::load(&entry, &model_dir).expect("fp16 model should load");

        let source = "The weather is nice today and we are going to the park to play.";
        let mut token_count = 0_usize;
        let t0 = std::time::Instant::now();
        let translated = model
            .translate(source, "eng_Latn", "rus_Cyrl", &mut |partial| {
                token_count = partial.split_whitespace().count();
                Ok(())
            })
            .expect("translation should succeed");
        let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(!translated.trim().is_empty());
        eprintln!(
            "[bench] device={} intra={} total={:.0}ms words~{} translation={}",
            model.device,
            intra_op_threads(),
            total_ms,
            token_count,
            translated
        );
    }
}
