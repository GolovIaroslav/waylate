use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Language {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EngineKind {
    OnnxEncoderDecoder,
    ManagedLlamaCpp,
    OpenAiCompatible,
    NetworkApi,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Audience {
    Beginner,
    HighQuality,
    Advanced,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PromptStyle {
    Chat,
    Completion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageCode {
    pub ui_code: String,
    pub label: String,
    pub nllb_code: Option<String>,
    pub onnx_marian_pair: Option<String>,
    pub llm_language_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub repo: String,
    pub path: String,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub destination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogEntry {
    pub id: String,
    pub name: String,
    pub engine: EngineKind,
    pub audience: Audience,
    pub license: String,
    pub license_url: String,
    pub homepage: String,
    pub description: String,
    pub languages: Vec<LanguageCode>,
    pub files: Vec<ModelFile>,
    pub prompt_style: Option<PromptStyle>,
    pub prompt_template: Option<String>,
    pub estimated_download_bytes: u64,
    pub estimated_disk_bytes: u64,
    pub min_ram_bytes: Option<u64>,
    pub downloadable: bool,
}

impl ModelCatalogEntry {
    pub fn language_codes_for_translate(&self) -> Vec<Language> {
        let wants_nllb =
            self.engine == EngineKind::OnnxEncoderDecoder && self.id.starts_with("nllb-");
        let mut languages = vec![Language {
            code: "auto".into(),
            name: "Auto detect".into(),
        }];
        languages.extend(self.languages.iter().filter_map(|language| {
            let code = if wants_nllb {
                language
                    .nllb_code
                    .as_deref()
                    .unwrap_or(language.ui_code.as_str())
            } else {
                language.ui_code.as_str()
            };
            if code == "auto" {
                return None;
            }
            Some(Language {
                code: code.to_string(),
                name: language.label.clone(),
            })
        }));
        languages
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallState {
    NotInstalled,
    Downloading {
        progress: f32,
        bytes_done: u64,
        bytes_total: Option<u64>,
    },
    Verifying,
    Ready,
    Failed {
        message: String,
    },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfile {
    pub id: String,
    pub name: String,
    pub provider: ProviderKind,
    pub description: String,
    pub quantization: String,
    pub size: String,
    pub homepage: String,
    pub engine_hint: String,
    pub default_endpoint: Option<String>,
    pub hf_repo: Option<String>,
    pub download_filenames: Vec<String>,
    pub managed_prompt_style: Option<String>,
    pub managed_prompt_template: Option<String>,
    pub managed_context_size: Option<u32>,
    pub install_check_files: Vec<String>,
    pub languages: Vec<Language>,
    pub downloadable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    OpenAiCompatible,
    DeepL,
    Google,
    Yandex,
    Custom,
}

impl From<ModelCatalogEntry> for ModelProfile {
    fn from(entry: ModelCatalogEntry) -> Self {
        let languages = entry.language_codes_for_translate();
        let quantization = match entry.id.as_str() {
            "nllb-200-distilled-600m-onnx" => "INT8 ONNX",
            "nllb-200-distilled-1.3b-onnx" => "INT8 ONNX",
            "opus-mt-marian-onnx" => "Pair-specific ONNX",
            "tencent-hy-mt2-1.8b-gguf" => "Q4_K_M GGUF",
            "translategemma-4b-gguf" => "Q4_K_M GGUF",
            "milmmt-46-1b-gguf" => "GGUF prep pending",
            _ => "Built-in",
        };
        let size = human_size(entry.estimated_download_bytes);
        let engine_hint = match entry.engine {
            EngineKind::OnnxEncoderDecoder => {
                "Built-in ONNX engine managed by Waylate.".to_string()
            }
            EngineKind::ManagedLlamaCpp => {
                "Waylate manages llama-server automatically for this model.".to_string()
            }
            EngineKind::OpenAiCompatible => {
                "Uses an external OpenAI-compatible endpoint.".to_string()
            }
            EngineKind::NetworkApi => "Uses a network translation API.".to_string(),
        };
        let provider = match entry.engine {
            EngineKind::OnnxEncoderDecoder => ProviderKind::Custom,
            EngineKind::ManagedLlamaCpp => ProviderKind::Custom,
            EngineKind::OpenAiCompatible => ProviderKind::OpenAiCompatible,
            EngineKind::NetworkApi => ProviderKind::Custom,
        };

        Self {
            id: entry.id,
            name: entry.name,
            provider,
            description: entry.description,
            quantization: quantization.into(),
            size,
            homepage: entry.homepage,
            engine_hint,
            default_endpoint: None,
            hf_repo: None,
            download_filenames: entry
                .files
                .iter()
                .map(|file| file.destination.clone())
                .collect(),
            managed_prompt_style: entry.prompt_style.map(prompt_style_key),
            managed_prompt_template: entry.prompt_template,
            managed_context_size: Some(4096),
            install_check_files: entry
                .files
                .iter()
                .map(|file| file.destination.clone())
                .collect(),
            languages,
            downloadable: entry.downloadable,
        }
    }
}

pub fn model_catalog() -> Vec<ModelCatalogEntry> {
    let languages = language_codes();
    vec![
        ModelCatalogEntry {
            id: "nllb-200-distilled-600m-onnx".into(),
            name: "NLLB-200 (Meta)".into(),
            engine: EngineKind::OnnxEncoderDecoder,
            audience: Audience::Beginner,
            license: "CC-BY-NC-4.0".into(),
            license_url: "https://creativecommons.org/licenses/by-nc/4.0/".into(),
            homepage: "https://huggingface.co/facebook/nllb-200-distilled-600M".into(),
            description: "Recommended broad-coverage local model. Downloads once and then works offline.".into(),
            languages: languages.clone(),
            files: vec![
                model_file(
                    "Xenova/nllb-200-distilled-600M",
                    "onnx/encoder_model_quantized.onnx",
                    "encoder_model_quantized.onnx",
                )
                .with_size(419_120_483)
                .with_sha256("2d291e975e27a76ab0a786e49148ca46b8177ffee21429941309f92016c300e3"),
                model_file(
                    "Xenova/nllb-200-distilled-600M",
                    "onnx/decoder_model_merged_quantized.onnx",
                    "decoder_model_merged_quantized.onnx",
                )
                .with_size(475_505_771)
                .with_sha256("3656bd87027534fc2a906966c02ab6fd08ba3d9b75cf87b18d1bb77d22799a54"),
                model_file("Xenova/nllb-200-distilled-600M", "tokenizer.json", "tokenizer.json")
                    .with_size(17_331_224)
                    .with_sha256("18761a875b5fe0e2091fe1af33c9d084902f95f0a38d4cdb1d2fa411850a95dd"),
            ],
            prompt_style: None,
            prompt_template: None,
            estimated_download_bytes: 911_957_478,
            estimated_disk_bytes: 911_957_478,
            min_ram_bytes: Some(2 * 1024 * 1024 * 1024),
            downloadable: true,
        },
        ModelCatalogEntry {
            id: "nllb-200-distilled-1.3b-onnx".into(),
            name: "NLLB-200 1.3B (Meta)".into(),
            engine: EngineKind::OnnxEncoderDecoder,
            audience: Audience::HighQuality,
            license: "CC-BY-NC-4.0".into(),
            license_url: "https://creativecommons.org/licenses/by-nc/4.0/".into(),
            homepage: "https://huggingface.co/facebook/nllb-200-distilled-1.3B".into(),
            description: "Higher-quality NLLB variant. Same 200 languages, better translation. Needs ~4 GB RAM.".into(),
            languages: languages.clone(),
            files: vec![
                model_file(
                    "Xenova/nllb-200-distilled-1.3B",
                    "onnx/encoder_model_quantized.onnx",
                    "encoder_model_quantized.onnx",
                ),
                model_file(
                    "Xenova/nllb-200-distilled-1.3B",
                    "onnx/decoder_model_merged_quantized.onnx",
                    "decoder_model_merged_quantized.onnx",
                ),
                model_file("Xenova/nllb-200-distilled-1.3B", "tokenizer.json", "tokenizer.json"),
            ],
            prompt_style: None,
            prompt_template: None,
            estimated_download_bytes: 1_950_000_000,
            estimated_disk_bytes: 1_950_000_000,
            min_ram_bytes: Some(4 * 1024 * 1024 * 1024),
            downloadable: true,
        },
        ModelCatalogEntry {
            id: "opus-mt-marian-onnx".into(),
            name: "OPUS-MT / Marian".into(),
            engine: EngineKind::OnnxEncoderDecoder,
            audience: Audience::Beginner,
            license: "Varies per language pair".into(),
            license_url: "https://huggingface.co/Helsinki-NLP".into(),
            homepage: "https://huggingface.co/Helsinki-NLP".into(),
            description: "Lightweight models for specific popular language pairs.".into(),
            languages: languages.clone(),
            files: Vec::new(),
            prompt_style: None,
            prompt_template: None,
            estimated_download_bytes: 300 * 1024 * 1024,
            estimated_disk_bytes: 350 * 1024 * 1024,
            min_ram_bytes: Some(1024 * 1024 * 1024),
            downloadable: false,
        },
        ModelCatalogEntry {
            id: "tencent-hy-mt2-1.8b-gguf".into(),
            name: "Tencent Hy-MT2 (compact)".into(),
            engine: EngineKind::ManagedLlamaCpp,
            audience: Audience::HighQuality,
            license: "Apache-2.0".into(),
            license_url: "https://raw.githubusercontent.com/Tencent-Hunyuan/Hy-MT2/main/LICENSE.txt".into(),
            homepage: "https://huggingface.co/tencent/Hy-MT2-1.8B-1.25Bit-GGUF".into(),
            description: "Compact high-quality local GGUF translation model.".into(),
            languages: languages.clone(),
            files: vec![model_file(
                "tencent/Hy-MT2-1.8B-GGUF",
                "Hy-MT2-1.8B-Q4_K_M.gguf",
                "Hy-MT2-1.8B-Q4_K_M.gguf",
            )
            .with_size(1_133_080_448)
            .with_sha256("a7725d3b0b25dd12b87709a4ef9a4faa70e80d23de3d190661f3d01439b11e0c")],
            prompt_style: Some(PromptStyle::Chat),
            prompt_template: Some("Translate the following text from {source} to {target}. Return only the translation.\n\n{text}".into()),
            estimated_download_bytes: 1_133_080_448,
            estimated_disk_bytes: 1_133_080_448,
            min_ram_bytes: Some(4 * 1024 * 1024 * 1024),
            downloadable: true,
        },
        ModelCatalogEntry {
            id: "translategemma-4b-gguf".into(),
            name: "TranslateGemma (Google)".into(),
            engine: EngineKind::ManagedLlamaCpp,
            audience: Audience::HighQuality,
            license: "Gemma license".into(),
            license_url: "https://ai.google.dev/gemma/terms".into(),
            homepage: "https://huggingface.co/bullerwins/translategemma-4b-it-GGUF".into(),
            description: "High-quality model for machines with 8 GB+ RAM.".into(),
            languages: languages.clone(),
            files: vec![model_file(
                "bullerwins/translategemma-4b-it-GGUF",
                "translategemma-4b-it-Q4_K_M.gguf",
                "translategemma-4b-it-Q4_K_M.gguf",
            )
            .with_size(2_489_909_312)
            .with_sha256("a1db9212fbef2d3ce43f4752eeb0f8eb86a911999e1cc11419eb5ffde6e72f67")],
            prompt_style: Some(PromptStyle::Chat),
            prompt_template: Some("Translate this from {source} to {target}. Return only the translation.\n\n{text}".into()),
            estimated_download_bytes: 3 * 1024 * 1024 * 1024,
            estimated_disk_bytes: 3 * 1024 * 1024 * 1024,
            min_ram_bytes: Some(8 * 1024 * 1024 * 1024),
            downloadable: false,
        },
        ModelCatalogEntry {
            id: "milmmt-46-1b-gguf".into(),
            name: "MiLMMT-46 (Xiaomi)".into(),
            engine: EngineKind::ManagedLlamaCpp,
            audience: Audience::HighQuality,
            license: "Gemma license".into(),
            license_url: "https://ai.google.dev/gemma/terms".into(),
            homepage: "https://huggingface.co/xiaomi-research/MiLMMT-46-1B-v0.1".into(),
            description: "Small multilingual translation model. Needs GGUF conversion before download is enabled.".into(),
            languages,
            files: Vec::new(),
            prompt_style: Some(PromptStyle::Completion),
            prompt_template: Some("Translate this from {source} to {target}.\n{source}: {text}\n{target}:".into()),
            estimated_download_bytes: 1024 * 1024 * 1024,
            estimated_disk_bytes: 1200 * 1024 * 1024,
            min_ram_bytes: Some(4 * 1024 * 1024 * 1024),
            downloadable: false,
        },
    ]
}

pub fn catalog() -> Vec<ModelProfile> {
    vec![
        ModelProfile {
            id: "tencent-hy-mt2-gguf".into(),
            name: "Tencent Hy-MT2 1.8B".into(),
            provider: ProviderKind::Custom,
            description: "Legacy entry. Use tencent-hy-mt2-1.8b-gguf from the new catalog instead.".into(),
            quantization: "Q4_K_M GGUF".into(),
            size: "~1.1 GB".into(),
            homepage: "https://huggingface.co/tencent/Hy-MT2-1.8B-GGUF".into(),
            engine_hint: "Legacy path. See the new model catalog.".into(),
            default_endpoint: None,
            hf_repo: Some("tencent/Hy-MT2-1.8B-GGUF".into()),
            download_filenames: vec!["Hy-MT2-1.8B-Q4_K_M.gguf".into()],
            managed_prompt_style: Some("chat".into()),
            managed_prompt_template: Some("Translate the following text from {source} to {target}. Return only the translation.\n\n{text}".into()),
            managed_context_size: Some(4096),
            install_check_files: vec!["Hy-MT2-1.8B-Q4_K_M.gguf".into()],
            languages: tencent_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "translategemma-4b-gguf".into(),
            name: "TranslateGemma 4B".into(),
            provider: ProviderKind::Custom,
            description: "Legacy entry. Use translategemma-4b-gguf from the new catalog instead.".into(),
            quantization: "Q4_K_M GGUF".into(),
            size: "~3.0 GB".into(),
            homepage: "https://huggingface.co/bullerwins/translategemma-4b-it-GGUF".into(),
            engine_hint: "Legacy path. See the new model catalog.".into(),
            default_endpoint: None,
            hf_repo: Some("bullerwins/translategemma-4b-it-GGUF".into()),
            download_filenames: vec!["translategemma-4b-it-Q4_K_M.gguf".into()],
            managed_prompt_style: Some("chat".into()),
            managed_prompt_template: Some("Translate the following text from {source} to {target}. Return only the translation.\n\n{text}".into()),
            managed_context_size: Some(4096),
            install_check_files: vec!["translategemma-4b-it-Q4_K_M.gguf".into()],
            languages: popular_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "milmmt-46-1b-gguf".into(),
            name: "MiLMMT-46 1B".into(),
            provider: ProviderKind::Custom,
            description: "Legacy entry: awaiting GGUF conversion from the new catalog.".into(),
            quantization: "Q4_K_M GGUF".into(),
            size: "~1.1 GB".into(),
            homepage: "https://huggingface.co/mradermacher/MiLMMT-46-1B-v0.1-GGUF".into(),
            engine_hint: "Legacy path. See the new model catalog.".into(),
            default_endpoint: None,
            hf_repo: Some("mradermacher/MiLMMT-46-1B-v0.1-GGUF".into()),
            download_filenames: vec!["MiLMMT-46-1B-v0.1.Q4_K_M.gguf".into()],
            managed_prompt_style: Some("completion".into()),
            managed_prompt_template: Some("Translate this from {source} to {target}.\n{source}: {text}\n{target}:".into()),
            managed_context_size: Some(4096),
            install_check_files: vec!["MiLMMT-46-1B-v0.1.Q4_K_M.gguf".into()],
            languages: popular_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "deepl-api".into(),
            name: "DeepL API".into(),
            provider: ProviderKind::DeepL,
            description: "Network translation provider. Disabled by default; needs your own API key.".into(),
            quantization: "Cloud API".into(),
            size: "remote".into(),
            homepage: "https://www.deepl.com/docs-api".into(),
            engine_hint: "Save a DeepL API key in settings. Text is sent to DeepL only when this profile is selected.".into(),
            default_endpoint: Some("https://api-free.deepl.com/v2/translate".into()),
            hf_repo: None,
            download_filenames: Vec::new(),
            managed_prompt_style: None,
            managed_prompt_template: None,
            managed_context_size: None,
            install_check_files: Vec::new(),
            languages: popular_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "google-api".into(),
            name: "Google Cloud Translate".into(),
            provider: ProviderKind::Google,
            description: "Network translation provider. Disabled by default; needs your own API key.".into(),
            quantization: "Cloud API".into(),
            size: "remote".into(),
            homepage: "https://cloud.google.com/translate/docs".into(),
            engine_hint: "Save a Google Cloud Translation API key in settings. Text is sent to Google only when this profile is selected.".into(),
            default_endpoint: Some("https://translation.googleapis.com/language/translate/v2".into()),
            hf_repo: None,
            download_filenames: Vec::new(),
            managed_prompt_style: None,
            managed_prompt_template: None,
            managed_context_size: None,
            install_check_files: Vec::new(),
            languages: popular_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "yandex-api".into(),
            name: "Yandex Cloud Translate".into(),
            provider: ProviderKind::Yandex,
            description: "Network translation provider. Disabled by default; needs your own API key and folder ID.".into(),
            quantization: "Cloud API".into(),
            size: "remote".into(),
            homepage: "https://aistudio.yandex.ru/docs/en/translate/operations/translate".into(),
            engine_hint: "Save a Yandex Cloud API key and Folder ID in Settings. Text is sent to Yandex only when this profile is selected.".into(),
            default_endpoint: Some("https://translate.api.cloud.yandex.net/translate/v2/translate".into()),
            hf_repo: None,
            download_filenames: Vec::new(),
            managed_prompt_style: None,
            managed_prompt_template: None,
            managed_context_size: None,
            install_check_files: Vec::new(),
            languages: popular_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "custom-local".into(),
            name: "Custom local model".into(),
            provider: ProviderKind::Custom,
            description: "Advanced profile for your own GGUF model or OpenAI-compatible local endpoint.".into(),
            quantization: "User supplied".into(),
            size: "custom".into(),
            homepage: "".into(),
            engine_hint: "Managed GGUF starts a hidden llama-server for a local GGUF file. External mode keeps support for your own endpoint.".into(),
            default_endpoint: Some("http://127.0.0.1:8080/v1/chat/completions".into()),
            hf_repo: None,
            download_filenames: Vec::new(),
            managed_prompt_style: None,
            managed_prompt_template: None,
            managed_context_size: None,
            install_check_files: Vec::new(),
            languages: popular_languages(),
            downloadable: false,
        },
    ]
}

fn popular_languages() -> Vec<Language> {
    [
        ("auto", "Auto detect"),
        ("en", "English"),
        ("ru", "Russian"),
        ("sk", "Slovak"),
        ("cs", "Czech"),
        ("de", "German"),
        ("uk", "Ukrainian"),
        ("pl", "Polish"),
        ("fr", "French"),
        ("es", "Spanish"),
        ("it", "Italian"),
        ("pt", "Portuguese"),
        ("tr", "Turkish"),
        ("zh", "Chinese"),
        ("ja", "Japanese"),
        ("ko", "Korean"),
    ]
    .into_iter()
    .map(lang)
    .collect()
}

fn language_codes() -> Vec<LanguageCode> {
    [
        ("auto", "Auto detect", None, None),
        ("en", "English", Some("eng_Latn"), Some("English")),
        ("ru", "Russian", Some("rus_Cyrl"), Some("Russian")),
        ("sk", "Slovak", Some("slk_Latn"), Some("Slovak")),
        ("cs", "Czech", Some("ces_Latn"), Some("Czech")),
        ("de", "German", Some("deu_Latn"), Some("German")),
        ("uk", "Ukrainian", Some("ukr_Cyrl"), Some("Ukrainian")),
        ("pl", "Polish", Some("pol_Latn"), Some("Polish")),
        ("fr", "French", Some("fra_Latn"), Some("French")),
        ("es", "Spanish", Some("spa_Latn"), Some("Spanish")),
        ("it", "Italian", Some("ita_Latn"), Some("Italian")),
        ("pt", "Portuguese", Some("por_Latn"), Some("Portuguese")),
        ("tr", "Turkish", Some("tur_Latn"), Some("Turkish")),
        ("zh", "Chinese", Some("zho_Hans"), Some("Chinese")),
        ("ja", "Japanese", Some("jpn_Jpan"), Some("Japanese")),
        ("ko", "Korean", Some("kor_Hang"), Some("Korean")),
    ]
    .into_iter()
    .map(
        |(ui_code, label, nllb_code, llm_language_name)| LanguageCode {
            ui_code: ui_code.into(),
            label: label.into(),
            nllb_code: nllb_code.map(str::to_string),
            onnx_marian_pair: None,
            llm_language_name: llm_language_name.map(str::to_string),
        },
    )
    .collect()
}

fn model_file(repo: &str, path: &str, destination: &str) -> ModelFile {
    ModelFile {
        repo: repo.into(),
        path: path.into(),
        sha256: None,
        size_bytes: None,
        destination: destination.into(),
    }
}

impl ModelFile {
    fn with_size(mut self, size_bytes: u64) -> Self {
        self.size_bytes = Some(size_bytes);
        self
    }

    fn with_sha256(mut self, sha256: &str) -> Self {
        self.sha256 = Some(sha256.into());
        self
    }
}

fn prompt_style_key(style: PromptStyle) -> String {
    match style {
        PromptStyle::Chat => "chat".into(),
        PromptStyle::Completion => "completion".into(),
    }
}

fn human_size(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        format!("~{:.1} GB", bytes as f64 / GIB as f64)
    } else {
        format!("~{} MB", (bytes / MIB).max(1))
    }
}

fn tencent_languages() -> Vec<Language> {
    [
        ("auto", "Auto detect"),
        ("en", "English"),
        ("ru", "Russian"),
        ("sk", "Slovak"),
        ("cs", "Czech"),
        ("de", "German"),
        ("uk", "Ukrainian"),
        ("pl", "Polish"),
        ("fr", "French"),
        ("es", "Spanish"),
        ("it", "Italian"),
        ("pt", "Portuguese"),
        ("tr", "Turkish"),
        ("zh", "Chinese"),
        ("ja", "Japanese"),
        ("ko", "Korean"),
        ("ar", "Arabic"),
        ("th", "Thai"),
        ("vi", "Vietnamese"),
        ("hi", "Hindi"),
    ]
    .into_iter()
    .map(lang)
    .collect()
}

fn lang((code, name): (&str, &str)) -> Language {
    Language {
        code: code.into(),
        name: name.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_legacy_profiles() {
        let catalog = catalog();
        assert!(catalog.iter().any(|profile| profile.id == "custom-local"));
        assert!(catalog.iter().any(|profile| profile.id == "deepl-api"));
        assert!(!catalog.iter().any(|profile| profile.id.contains("ct2")));
    }

    #[test]
    fn spec_catalog_contains_only_agreed_local_engines() {
        let catalog = model_catalog();
        assert_eq!(catalog.len(), 6);
        assert_eq!(catalog[0].id, "nllb-200-distilled-600m-onnx");
        assert!(catalog
            .iter()
            .any(|profile| profile.engine == EngineKind::OnnxEncoderDecoder));
        assert!(catalog
            .iter()
            .any(|profile| profile.engine == EngineKind::ManagedLlamaCpp));
        assert!(!catalog.iter().any(|profile| profile.id.contains("ct2")));
    }

    #[test]
    fn milmmt_waits_for_gguf_prep_before_download() {
        let profile = model_catalog()
            .into_iter()
            .find(|profile| profile.id == "milmmt-46-1b-gguf")
            .expect("milmmt entry exists");
        assert!(!profile.downloadable);
        assert!(profile.files.is_empty());
    }
}
