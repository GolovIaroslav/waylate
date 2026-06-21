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
    CTranslate2,
    DeepL,
    Google,
    Yandex,
    Custom,
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
                model_file("niedev/nllb-200-distilled-600M-onnx", "encoder_model.onnx", "encoder_model.onnx"),
                model_file(
                    "niedev/nllb-200-distilled-600M-onnx",
                    "decoder_model_merged.onnx",
                    "decoder_model_merged.onnx",
                ),
                model_file("niedev/nllb-200-distilled-600M-onnx", "tokenizer.json", "tokenizer.json"),
            ],
            prompt_style: None,
            prompt_template: None,
            estimated_download_bytes: 600 * 1024 * 1024,
            estimated_disk_bytes: 700 * 1024 * 1024,
            min_ram_bytes: Some(2 * 1024 * 1024 * 1024),
            downloadable: false,
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
                "tencent/Hy-MT2-1.8B-1.25Bit-GGUF",
                "Hy-MT2-1.8B-1.25Bit.gguf",
                "Hy-MT2-1.8B-1.25Bit.gguf",
            )],
            prompt_style: Some(PromptStyle::Chat),
            prompt_template: Some("Translate the following text from {source} to {target}. Return only the translation.\n\n{text}".into()),
            estimated_download_bytes: 440 * 1024 * 1024,
            estimated_disk_bytes: 480 * 1024 * 1024,
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
            )],
            prompt_style: Some(PromptStyle::Chat),
            // TODO: pull the exact chat template from the model card before enabling.
            prompt_template: None,
            estimated_download_bytes: 3 * 1024 * 1024 * 1024,
            estimated_disk_bytes: 3 * 1024 * 1024 * 1024,
            min_ram_bytes: Some(8 * 1024 * 1024 * 1024),
            downloadable: true,
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
            id: "nllb-200-ct2".into(),
            name: "NLLB-200".into(),
            provider: ProviderKind::CTranslate2,
            description: "Legacy CTranslate2 model entry (kept for existing installs). New users should use the ONNX engine.".into(),
            quantization: "Balanced".into(),
            size: "~2.4 GB".into(),
            homepage: "https://huggingface.co/entai2965/nllb-200-distilled-600M-ctranslate2".into(),
            engine_hint: "Legacy path. Replaced by the ONNX engine in new installs.".into(),
            default_endpoint: None,
            hf_repo: Some("entai2965/nllb-200-distilled-600M-ctranslate2".into()),
            download_filenames: Vec::new(),
            managed_prompt_style: None,
            managed_prompt_template: None,
            managed_context_size: None,
            install_check_files: vec![
                "config.json".into(),
                "model.bin".into(),
                "tokenizer.json".into(),
                "tokenizer_config.json".into(),
            ],
            languages: nllb_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "nllb-200-ct2-int8".into(),
            name: "NLLB-200 INT8".into(),
            provider: ProviderKind::CTranslate2,
            description: "Legacy CTranslate2 INT8 entry (kept for existing installs).".into(),
            quantization: "INT8".into(),
            size: "~1.3 GB".into(),
            homepage: "https://huggingface.co/Tushe/nllb-200-600M-ct2-int8".into(),
            engine_hint: "Legacy path. Replaced by the ONNX engine in new installs.".into(),
            default_endpoint: None,
            hf_repo: Some("Tushe/nllb-200-600M-ct2-int8".into()),
            download_filenames: Vec::new(),
            managed_prompt_style: None,
            managed_prompt_template: None,
            managed_context_size: None,
            install_check_files: vec![
                "config.json".into(),
                "model.bin".into(),
                "tokenizer.json".into(),
                "tokenizer_config.json".into(),
            ],
            languages: nllb_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "opus-mt-en-ru".into(),
            name: "OPUS-MT / Marian".into(),
            provider: ProviderKind::CTranslate2,
            description: "Lightweight model family for specific language pairs such as English to Russian.".into(),
            quantization: "Pair-specific".into(),
            size: "~300 MB".into(),
            homepage: "https://huggingface.co/Helsinki-NLP/opus-mt-en-ru".into(),
            engine_hint: "This family is planned next. It is listed here so the catalog shape is complete, but downloading is not enabled yet.".into(),
            default_endpoint: None,
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
            id: "tencent-hy-mt2-gguf".into(),
            name: "Tencent Hy-MT2 1.8B".into(),
            provider: ProviderKind::Custom,
            description: "Legacy entry. Use tencent-hy-mt2-1.8b-gguf from the new catalog instead.".into(),
            quantization: "1.25-bit GGUF".into(),
            size: "~440 MB".into(),
            homepage: "https://huggingface.co/tencent/Hy-MT2-1.8B-1.25Bit-GGUF".into(),
            engine_hint: "Legacy path. See the new model catalog.".into(),
            default_endpoint: None,
            hf_repo: Some("tencent/Hy-MT2-1.8B-1.25Bit-GGUF".into()),
            download_filenames: vec!["Hy-MT2-1.8B-1.25Bit.gguf".into()],
            managed_prompt_style: Some("chat".into()),
            managed_prompt_template: Some("Translate the following text from {source} to {target}. Return only the translation.\n\n{text}".into()),
            managed_context_size: Some(4096),
            install_check_files: vec!["Hy-MT2-1.8B-1.25Bit.gguf".into()],
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
    .map(|(ui_code, label, nllb_code, llm_language_name)| LanguageCode {
        ui_code: ui_code.into(),
        label: label.into(),
        nllb_code: nllb_code.map(str::to_string),
        onnx_marian_pair: None,
        llm_language_name: llm_language_name.map(str::to_string),
    })
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

fn nllb_languages() -> Vec<Language> {
    [
        ("auto", "Auto detect"),
        ("eng_Latn", "English"),
        ("rus_Cyrl", "Russian"),
        ("slk_Latn", "Slovak"),
        ("ces_Latn", "Czech"),
        ("deu_Latn", "German"),
        ("ukr_Cyrl", "Ukrainian"),
        ("pol_Latn", "Polish"),
        ("fra_Latn", "French"),
        ("spa_Latn", "Spanish"),
        ("ita_Latn", "Italian"),
        ("por_Latn", "Portuguese"),
        ("tur_Latn", "Turkish"),
        ("zho_Hans", "Chinese Simplified"),
        ("jpn_Jpan", "Japanese"),
        ("kor_Hang", "Korean"),
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
    fn catalog_contains_local_profiles_first() {
        let catalog = catalog();
        assert_eq!(catalog[0].id, "nllb-200-ct2");
        assert!(catalog.iter().any(|profile| profile.id == "nllb-200-ct2"));
        assert!(catalog.iter().any(|profile| profile.id == "custom-local"));
    }

    #[test]
    fn nllb_profile_uses_nllb_language_codes() {
        let profile = catalog()
            .into_iter()
            .find(|profile| profile.id == "nllb-200-ct2")
            .expect("nllb profile exists");
        assert!(profile.languages.iter().any(|language| language.code == "eng_Latn"));
        assert!(profile.languages.iter().any(|language| language.code == "rus_Cyrl"));
    }

    #[test]
    fn spec_catalog_contains_only_agreed_local_engines() {
        let catalog = model_catalog();
        assert_eq!(catalog.len(), 5);
        assert_eq!(catalog[0].id, "nllb-200-distilled-600m-onnx");
        assert!(catalog.iter().any(|profile| profile.engine == EngineKind::OnnxEncoderDecoder));
        assert!(catalog.iter().any(|profile| profile.engine == EngineKind::ManagedLlamaCpp));
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
