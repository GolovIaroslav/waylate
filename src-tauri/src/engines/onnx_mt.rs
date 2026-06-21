use crate::{
    config::AppPaths,
    models::{EngineKind, ModelCatalogEntry},
};
use ort::{
    session::Session,
    value::{DynTensor, Tensor, TensorElementType, ValueType},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tokenizers::Tokenizer;

static MODEL_CACHE: OnceLock<Mutex<HashMap<String, LoadedOnnxModel>>> = OnceLock::new();

pub fn translate(
    paths: &AppPaths,
    entry: &ModelCatalogEntry,
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<String, String> {
    if entry.engine != EngineKind::OnnxEncoderDecoder {
        return Err("Model is not backed by the ONNX engine.".into());
    }
    if source_lang == "auto" {
        return Err("Choose the source language for local ONNX translation. Auto-detect is not available yet.".into());
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
    model.translate(text, source_lang, target_lang)
}

struct LoadedOnnxModel {
    tokenizer: Tokenizer,
    encoder: Session,
    decoder: Session,
    eos_token_id: i64,
}

struct CacheTensor {
    shape: Vec<usize>,
    data: Vec<f32>,
}

impl LoadedOnnxModel {
    fn load(entry: &ModelCatalogEntry, model_dir: &Path) -> Result<Self, String> {
        let encoder_path = required_file(model_dir, &[
            "encoder_model_quantized.onnx",
            "encoder_model_int8.onnx",
            "encoder_model.onnx",
        ])?;
        let decoder_path = required_file(model_dir, &[
            "decoder_model_merged_quantized.onnx",
            "decoder_model_merged_int8.onnx",
            "decoder_model_merged.onnx",
        ])?;
        let tokenizer_path = required_file(model_dir, &["tokenizer.json"])?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|err| format!("Could not load tokenizer for {}: {err}", entry.name))?;
        let encoder = Session::builder()
            .map_err(|err| format!("Could not create ONNX encoder session: {err}"))?
            .commit_from_file(&encoder_path)
            .map_err(|err| format!("Could not load encoder model: {err}"))?;
        let decoder = Session::builder()
            .map_err(|err| format!("Could not create ONNX decoder session: {err}"))?
            .commit_from_file(&decoder_path)
            .map_err(|err| format!("Could not load decoder model: {err}"))?;

        let eos_token_id = token_id(&tokenizer, "</s>")?;

        Ok(Self {
            tokenizer,
            encoder,
            decoder,
            eos_token_id,
        })
    }

    fn translate(&mut self, text: &str, source_lang: &str, target_lang: &str) -> Result<String, String> {
        let source_lang_id = token_id(&self.tokenizer, source_lang)?;
        let target_lang_id = token_id(&self.tokenizer, target_lang)?;

        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|err| format!("Could not tokenize source text: {err}"))?;

        let mut encoder_input_ids: Vec<i64> = encoding.get_ids().iter().map(|id| i64::from(*id)).collect();
        encoder_input_ids.push(self.eos_token_id);
        encoder_input_ids.push(source_lang_id);
        let encoder_attention_mask = vec![1_i64; encoder_input_ids.len()];

        let encoder_hidden_states = self.run_encoder(&encoder_input_ids, &encoder_attention_mask)?;

        let mut decoder_tokens = vec![self.eos_token_id, target_lang_id];
        let mut cache = None;
        for _ in 0..256 {
            let next_token = self.decode_next_token(
                &decoder_tokens,
                &encoder_attention_mask,
                &encoder_hidden_states,
                &mut cache,
            )?;
            if next_token == self.eos_token_id {
                break;
            }
            decoder_tokens.push(next_token);
        }

        let generated: Vec<u32> = decoder_tokens
            .into_iter()
            .skip(2)
            .filter_map(|id| u32::try_from(id).ok())
            .collect();

        self.tokenizer
            .decode(&generated, true)
            .map(|text| text.trim().to_string())
            .map_err(|err| format!("Could not decode translated tokens: {err}"))
    }

    fn run_encoder(&mut self, input_ids: &[i64], attention_mask: &[i64]) -> Result<Vec<f32>, String> {
        let mut inputs = HashMap::<String, DynTensor>::new();
        inputs.insert(
            "input_ids".into(),
            Tensor::from_array(([1_usize, input_ids.len()], input_ids.to_vec()))
                .map_err(|err| format!("Could not build encoder input_ids tensor: {err}"))?
                .upcast(),
        );
        inputs.insert(
            "attention_mask".into(),
            Tensor::from_array(([1_usize, attention_mask.len()], attention_mask.to_vec()))
                .map_err(|err| format!("Could not build encoder attention_mask tensor: {err}"))?
                .upcast(),
        );

        let outputs = self
            .encoder
            .run(inputs)
            .map_err(|err| format!("ONNX encoder inference failed: {err}"))?;
        let first_output = outputs.values().next();
        let hidden = if let Some(hidden) = outputs.get("last_hidden_state") {
            hidden
        } else {
            first_output
                .as_deref()
                .ok_or_else(|| "ONNX encoder returned no outputs".to_string())?
        };
        let array = hidden
            .try_extract_array::<f32>()
            .map_err(|err| format!("Could not read encoder output tensor: {err}"))?;
        Ok(array.iter().copied().collect())
    }

    fn decode_next_token(
        &mut self,
        decoder_tokens: &[i64],
        encoder_attention_mask: &[i64],
        encoder_hidden_states: &[f32],
        cache: &mut Option<HashMap<String, CacheTensor>>,
    ) -> Result<i64, String> {
        let mut inputs = HashMap::<String, DynTensor>::new();
        let hidden_size = infer_hidden_size(self.decoder.inputs(), encoder_hidden_states.len(), encoder_attention_mask.len())?;
        let use_cache = cache.is_some();
        let decoder_input_ids = if use_cache {
            vec![*decoder_tokens.last().ok_or_else(|| "Decoder input is empty".to_string())?]
        } else {
            decoder_tokens.to_vec()
        };

        for outlet in self.decoder.inputs() {
            let name = outlet.name();
            if name == "input_ids" {
                inputs.insert(
                    name.into(),
                    Tensor::from_array(([1_usize, decoder_input_ids.len()], decoder_input_ids.clone()))
                        .map_err(|err| format!("Could not build decoder input_ids tensor: {err}"))?
                        .upcast(),
                );
                continue;
            }
            if name == "encoder_attention_mask" {
                inputs.insert(
                    name.into(),
                    Tensor::from_array(([1_usize, encoder_attention_mask.len()], encoder_attention_mask.to_vec()))
                        .map_err(|err| format!("Could not build decoder encoder_attention_mask tensor: {err}"))?
                        .upcast(),
                );
                continue;
            }
            if name == "encoder_hidden_states" {
                inputs.insert(
                    name.into(),
                    Tensor::from_array((
                        [1_usize, encoder_attention_mask.len(), hidden_size],
                        encoder_hidden_states.to_vec(),
                    ))
                    .map_err(|err| format!("Could not build decoder encoder_hidden_states tensor: {err}"))?
                    .upcast(),
                );
                continue;
            }
            if name == "use_cache_branch" {
                inputs.insert(
                    name.into(),
                    Tensor::from_array(([1_usize], vec![use_cache]))
                        .map_err(|err| format!("Could not build decoder use_cache_branch tensor: {err}"))?
                        .upcast(),
                );
                continue;
            }
            if name.contains("past_key_values") {
                let tensor = if let Some(tensor) = cache.as_ref().and_then(|cache| cache.get(name)) {
                    cache_tensor_value(tensor)?
                } else {
                    initial_cache_tensor_for_outlet(outlet)?
                };
                inputs.insert(name.into(), tensor);
            }
        }

        let outputs = self
            .decoder
            .run(inputs)
            .map_err(|err| format!("ONNX decoder inference failed: {err}"))?;
        let best_id = {
            let first_output = outputs.values().next();
            let logits = if let Some(logits) = outputs.get("logits") {
                logits
            } else {
                first_output
                    .as_deref()
                    .ok_or_else(|| "ONNX decoder returned no logits".to_string())?
            };
            let logits = logits
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
            let mut best_id = 0_i64;
            let mut best_score = f32::NEG_INFINITY;
            for (idx, score) in logits.iter().skip(start).enumerate() {
                if *score > best_score {
                    best_score = *score;
                    best_id = idx as i64;
                }
            }
            best_id
        };
        update_cache(&outputs, cache)?;
        Ok(best_id)
    }
}

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

fn infer_hidden_size(inputs: &[ort::value::Outlet], hidden_len: usize, source_len: usize) -> Result<usize, String> {
    let outlet = inputs
        .iter()
        .find(|outlet| outlet.name() == "encoder_hidden_states")
        .ok_or_else(|| "Decoder model is missing encoder_hidden_states input".to_string())?;
    if let ValueType::Tensor { shape, .. } = outlet.dtype() {
        if let Some(dim) = shape.as_ref().last().copied() {
            if dim > 0 {
                return Ok(dim as usize);
            }
        }
    }
    if source_len == 0 || hidden_len % source_len != 0 {
        return Err("Could not infer encoder hidden size for decoder input".into());
    }
    Ok(hidden_len / source_len)
}

fn initial_cache_tensor_for_outlet(outlet: &ort::value::Outlet) -> Result<DynTensor, String> {
    let ValueType::Tensor { ty, shape, .. } = outlet.dtype() else {
        return Err(format!("Unsupported non-tensor decoder input '{}'", outlet.name()));
    };
    let dims = concrete_initial_cache_dims(shape.as_ref());
    let element_count = element_count(&dims);

    match ty {
        TensorElementType::Float32 => Tensor::from_array((dims, vec![0_f32; element_count]))
            .map(|tensor| tensor.upcast())
            .map_err(|err| format!("Could not build zero tensor for {}: {err}", outlet.name())),
        TensorElementType::Float16 => Err(format!(
            "Unsupported float16 cache input '{}' for ONNX decoder",
            outlet.name()
        )),
        TensorElementType::Int64 => Tensor::from_array((dims, vec![0_i64; element_count]))
            .map(|tensor| tensor.upcast())
            .map_err(|err| format!("Could not build zero tensor for {}: {err}", outlet.name())),
        TensorElementType::Bool => Tensor::from_array((dims, vec![false; element_count.max(1)]))
            .map(|tensor| tensor.upcast())
            .map_err(|err| format!("Could not build zero tensor for {}: {err}", outlet.name())),
        other => Err(format!(
            "Unsupported decoder cache tensor type {:?} for '{}'",
            other,
            outlet.name()
        )),
    }
}

fn cache_tensor_value(cache: &CacheTensor) -> Result<DynTensor, String> {
    Tensor::from_array((cache.shape.clone(), cache.data.clone()))
        .map(|tensor| tensor.upcast())
        .map_err(|err| format!("Could not build decoder cache tensor: {err}"))
}

fn update_cache(
    outputs: &ort::session::SessionOutputs,
    cache: &mut Option<HashMap<String, CacheTensor>>,
) -> Result<(), String> {
    let previous = cache.take().unwrap_or_default();
    let mut next = HashMap::new();
    for (output_name, value) in outputs.iter() {
        let Some(indexed_name) = output_name.strip_prefix("present.") else {
            continue;
        };
        let input_name = format!("past_key_values.{indexed_name}");
        let array = value
            .try_extract_array::<f32>()
            .map_err(|err| format!("Could not read decoder cache output {output_name}: {err}"))?;
        let shape = array.shape().to_vec();
        let data = array.iter().copied().collect::<Vec<_>>();
        if data.is_empty() && input_name.contains(".encoder.") {
            if let Some(previous) = previous.get(&input_name) {
                next.insert(
                    input_name,
                    CacheTensor {
                        shape: previous.shape.clone(),
                        data: previous.data.clone(),
                    },
                );
            }
            continue;
        }
        next.insert(input_name, CacheTensor { shape, data });
    }
    *cache = Some(next);
    Ok(())
}

fn concrete_initial_cache_dims(shape: &[i64]) -> Vec<usize> {
    shape
        .iter()
        .map(|dim| {
            if *dim > 0 {
                *dim as usize
            } else {
                1
            }
        })
        .collect()
}

fn element_count(dims: &[usize]) -> usize {
    if dims.is_empty() {
        1
    } else {
        dims.iter().product()
    }
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
        let entry = crate::models::model_catalog()
            .into_iter()
            .find(|entry| entry.id == "nllb-200-distilled-600m-onnx")
            .expect("nllb entry exists");
        let model_dir = paths.models_dir.join(&entry.id);
        let mut model = LoadedOnnxModel::load(&entry, &model_dir).expect("model should load");

        eprintln!("encoder inputs:");
        for input in model.encoder.inputs() {
            eprintln!("  {} {:?}", input.name(), input.dtype());
        }
        eprintln!("decoder inputs:");
        for input in model.decoder.inputs() {
            eprintln!("  {} {:?}", input.name(), input.dtype());
        }
        eprintln!("decoder outputs:");
        for output in model.decoder.outputs() {
            eprintln!("  {} {:?}", output.name(), output.dtype());
        }

        let translated = model
            .translate("Hello world", "eng_Latn", "rus_Cyrl")
            .expect("translation should succeed");
        assert!(!translated.trim().is_empty());
        eprintln!("translation: {translated}");
    }
}
