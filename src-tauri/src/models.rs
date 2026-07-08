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
    pub actual_language_count: Option<u32>,
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
            actual_language_count: Some(200),
            files: vec![
                model_file(
                    "Xenova/nllb-200-distilled-600M",
                    "onnx/encoder_model_quantized.onnx",
                    "encoder_model_quantized.onnx",
                )
                .with_size(419_120_483),
                model_file(
                    "Xenova/nllb-200-distilled-600M",
                    "onnx/decoder_model_merged_quantized.onnx",
                    "decoder_model_merged_quantized.onnx",
                )
                .with_size(475_505_771),
                model_file("Xenova/nllb-200-distilled-600M", "tokenizer.json", "tokenizer.json")
                    .with_size(17_331_224),
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
            description: "Higher-quality NLLB variant. Same 200 languages, better translation but slower and needs ~4 GB RAM. Runs on CPU or GPU like the 600M model — pick it only if you want better quality.".into(),
            languages: languages.clone(),
            actual_language_count: Some(200),
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
            actual_language_count: None,
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
            homepage: "https://huggingface.co/tencent/Hy-MT2-1.8B-GGUF".into(),
            description: "Compact high-quality local GGUF translation model. 131 languages.".into(),
            languages: languages.clone(),
            actual_language_count: Some(131),
            files: vec![model_file(
                "tencent/Hy-MT2-1.8B-GGUF",
                "Hy-MT2-1.8B-Q4_K_M.gguf",
                "Hy-MT2-1.8B-Q4_K_M.gguf",
            )
            .with_size(1_133_080_448)],
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
            actual_language_count: None,
            files: vec![model_file(
                "bullerwins/translategemma-4b-it-GGUF",
                "translategemma-4b-it-Q4_K_M.gguf",
                "translategemma-4b-it-Q4_K_M.gguf",
            )
            .with_size(2_489_909_312)],
            prompt_style: Some(PromptStyle::Chat),
            prompt_template: Some("Translate this from {source} to {target}. Return only the translation.\n\n{text}".into()),
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
            actual_language_count: None,
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
    // Full NLLB-200 language list with correct tokenizer codes
    [
        ("auto",   "Auto detect",           None,             None),
        ("en",     "English",               Some("eng_Latn"), Some("English")),
        ("ru",     "Russian",               Some("rus_Cyrl"), Some("Russian")),
        ("zh",     "Chinese (Simplified)",  Some("zho_Hans"), Some("Chinese")),
        ("zh-TW",  "Chinese (Traditional)", Some("zho_Hant"), Some("Traditional Chinese")),
        ("ar",     "Arabic",                Some("arb_Arab"), Some("Arabic")),
        ("de",     "German",                Some("deu_Latn"), Some("German")),
        ("fr",     "French",                Some("fra_Latn"), Some("French")),
        ("es",     "Spanish",               Some("spa_Latn"), Some("Spanish")),
        ("pt",     "Portuguese",            Some("por_Latn"), Some("Portuguese")),
        ("it",     "Italian",               Some("ita_Latn"), Some("Italian")),
        ("pl",     "Polish",                Some("pol_Latn"), Some("Polish")),
        ("nl",     "Dutch",                 Some("nld_Latn"), Some("Dutch")),
        ("tr",     "Turkish",               Some("tur_Latn"), Some("Turkish")),
        ("ja",     "Japanese",              Some("jpn_Jpan"), Some("Japanese")),
        ("ko",     "Korean",                Some("kor_Hang"), Some("Korean")),
        ("vi",     "Vietnamese",            Some("vie_Latn"), Some("Vietnamese")),
        ("th",     "Thai",                  Some("tha_Thai"), Some("Thai")),
        ("id",     "Indonesian",            Some("ind_Latn"), Some("Indonesian")),
        ("ms",     "Malay",                 Some("zsm_Latn"), Some("Malay")),
        ("hi",     "Hindi",                 Some("hin_Deva"), Some("Hindi")),
        ("bn",     "Bengali",               Some("ben_Beng"), Some("Bengali")),
        ("uk",     "Ukrainian",             Some("ukr_Cyrl"), Some("Ukrainian")),
        ("cs",     "Czech",                 Some("ces_Latn"), Some("Czech")),
        ("sk",     "Slovak",                Some("slk_Latn"), Some("Slovak")),
        ("ro",     "Romanian",              Some("ron_Latn"), Some("Romanian")),
        ("hu",     "Hungarian",             Some("hun_Latn"), Some("Hungarian")),
        ("bg",     "Bulgarian",             Some("bul_Cyrl"), Some("Bulgarian")),
        ("hr",     "Croatian",              Some("hrv_Latn"), Some("Croatian")),
        ("sr",     "Serbian",               Some("srp_Cyrl"), Some("Serbian")),
        ("bs",     "Bosnian",               Some("bos_Latn"), Some("Bosnian")),
        ("sv",     "Swedish",               Some("swe_Latn"), Some("Swedish")),
        ("da",     "Danish",                Some("dan_Latn"), Some("Danish")),
        ("fi",     "Finnish",               Some("fin_Latn"), Some("Finnish")),
        ("nb",     "Norwegian",             Some("nob_Latn"), Some("Norwegian")),
        ("nn",     "Norwegian Nynorsk",     Some("nno_Latn"), Some("Norwegian Nynorsk")),
        ("el",     "Greek",                 Some("ell_Grek"), Some("Greek")),
        ("he",     "Hebrew",                Some("heb_Hebr"), Some("Hebrew")),
        ("fa",     "Persian",               Some("pes_Arab"), Some("Persian")),
        ("ur",     "Urdu",                  Some("urd_Arab"), Some("Urdu")),
        ("af",     "Afrikaans",             Some("afr_Latn"), Some("Afrikaans")),
        ("sq",     "Albanian",              Some("als_Latn"), Some("Albanian")),
        ("am",     "Amharic",               Some("amh_Ethi"), Some("Amharic")),
        ("hy",     "Armenian",              Some("hye_Armn"), Some("Armenian")),
        ("az",     "Azerbaijani",           Some("azj_Latn"), Some("Azerbaijani")),
        ("eu",     "Basque",                Some("eus_Latn"), Some("Basque")),
        ("be",     "Belarusian",            Some("bel_Cyrl"), Some("Belarusian")),
        ("ca",     "Catalan",               Some("cat_Latn"), Some("Catalan")),
        ("cy",     "Welsh",                 Some("cym_Latn"), Some("Welsh")),
        ("et",     "Estonian",              Some("est_Latn"), Some("Estonian")),
        ("gl",     "Galician",              Some("glg_Latn"), Some("Galician")),
        ("ka",     "Georgian",              Some("kat_Geor"), Some("Georgian")),
        ("gu",     "Gujarati",              Some("guj_Gujr"), Some("Gujarati")),
        ("ht",     "Haitian Creole",        Some("hat_Latn"), Some("Haitian Creole")),
        ("ha",     "Hausa",                 Some("hau_Latn"), Some("Hausa")),
        ("ig",     "Igbo",                  Some("ibo_Latn"), Some("Igbo")),
        ("is",     "Icelandic",             Some("isl_Latn"), Some("Icelandic")),
        ("jv",     "Javanese",              Some("jav_Latn"), Some("Javanese")),
        ("kn",     "Kannada",               Some("kan_Knda"), Some("Kannada")),
        ("kk",     "Kazakh",                Some("kaz_Cyrl"), Some("Kazakh")),
        ("km",     "Khmer",                 Some("khm_Khmr"), Some("Khmer")),
        ("ky",     "Kyrgyz",                Some("kir_Cyrl"), Some("Kyrgyz")),
        ("lo",     "Lao",                   Some("lao_Laoo"), Some("Lao")),
        ("lv",     "Latvian",               Some("lvs_Latn"), Some("Latvian")),
        ("lt",     "Lithuanian",            Some("lit_Latn"), Some("Lithuanian")),
        ("lb",     "Luxembourgish",         Some("ltz_Latn"), Some("Luxembourgish")),
        ("mk",     "Macedonian",            Some("mkd_Cyrl"), Some("Macedonian")),
        ("mg",     "Malagasy",              Some("plt_Latn"), Some("Malagasy")),
        ("ml",     "Malayalam",             Some("mal_Mlym"), Some("Malayalam")),
        ("mt",     "Maltese",               Some("mlt_Latn"), Some("Maltese")),
        ("mr",     "Marathi",               Some("mar_Deva"), Some("Marathi")),
        ("mn",     "Mongolian",             Some("khk_Cyrl"), Some("Mongolian")),
        ("my",     "Burmese",               Some("mya_Mymr"), Some("Burmese")),
        ("ne",     "Nepali",                Some("npi_Deva"), Some("Nepali")),
        ("ny",     "Chichewa",              Some("nya_Latn"), Some("Chichewa")),
        ("or",     "Odia",                  Some("ory_Orya"), Some("Odia")),
        ("ps",     "Pashto",                Some("pbt_Arab"), Some("Pashto")),
        ("pa",     "Punjabi",               Some("pan_Guru"), Some("Punjabi")),
        ("si",     "Sinhala",               Some("sin_Sinh"), Some("Sinhala")),
        ("sl",     "Slovenian",             Some("slv_Latn"), Some("Slovenian")),
        ("so",     "Somali",                Some("som_Latn"), Some("Somali")),
        ("sw",     "Swahili",               Some("swh_Latn"), Some("Swahili")),
        ("tl",     "Filipino",              Some("tgl_Latn"), Some("Filipino")),
        ("tg",     "Tajik",                 Some("tgk_Cyrl"), Some("Tajik")),
        ("ta",     "Tamil",                 Some("tam_Taml"), Some("Tamil")),
        ("te",     "Telugu",                Some("tel_Telu"), Some("Telugu")),
        ("bo",     "Tibetan",               Some("bod_Tibt"), Some("Tibetan")),
        ("ti",     "Tigrinya",              Some("tir_Ethi"), Some("Tigrinya")),
        ("tk",     "Turkmen",               Some("tuk_Latn"), Some("Turkmen")),
        ("ug",     "Uyghur",                Some("uig_Arab"), Some("Uyghur")),
        ("uz",     "Uzbek",                 Some("uzn_Latn"), Some("Uzbek")),
        ("yo",     "Yoruba",                Some("yor_Latn"), Some("Yoruba")),
        ("zu",     "Zulu",                  Some("zul_Latn"), Some("Zulu")),
        ("xho",    "Xhosa",                 Some("xho_Latn"), Some("Xhosa")),
        ("sn",     "Shona",                 Some("sna_Latn"), Some("Shona")),
        ("rw",     "Kinyarwanda",           Some("kin_Latn"), Some("Kinyarwanda")),
        ("rn",     "Kirundi",               Some("run_Latn"), Some("Kirundi")),
        ("lg",     "Luganda",               Some("lug_Latn"), Some("Luganda")),
        ("ln",     "Lingala",               Some("lin_Latn"), Some("Lingala")),
        ("tw",     "Twi",                   Some("aka_Latn"), Some("Twi")),
        ("ee",     "Ewe",                   Some("ewe_Latn"), Some("Ewe")),
        ("ceb",    "Cebuano",               Some("ceb_Latn"), Some("Cebuano")),
        ("ilo",    "Ilocano",               Some("ilo_Latn"), Some("Ilocano")),
        ("su",     "Sundanese",             Some("sun_Latn"), Some("Sundanese")),
        ("war",    "Waray",                 Some("war_Latn"), Some("Waray")),
        ("ckb",    "Kurdish (Sorani)",      Some("ckb_Arab"), Some("Kurdish")),
        ("kmr",    "Kurdish (Kurmanji)",    Some("kmr_Latn"), Some("Kurmanji Kurdish")),
        ("eo",     "Esperanto",             Some("epo_Latn"), Some("Esperanto")),
        ("ga",     "Irish",                 Some("gle_Latn"), Some("Irish")),
        ("gd",     "Scottish Gaelic",       Some("gla_Latn"), Some("Scottish Gaelic")),
        ("oc",     "Occitan",               Some("oci_Latn"), Some("Occitan")),
        ("mi",     "Māori",                 Some("mri_Latn"), Some("Māori")),
        ("sm",     "Samoan",                Some("smo_Latn"), Some("Samoan")),
        ("fj",     "Fijian",                Some("fij_Latn"), Some("Fijian")),
        ("tn",     "Tswana",                Some("tsn_Latn"), Some("Tswana")),
        ("st",     "Southern Sotho",        Some("sot_Latn"), Some("Southern Sotho")),
        ("ss",     "Swati",                 Some("ssw_Latn"), Some("Swati")),
        ("ts",     "Tsonga",                Some("tso_Latn"), Some("Tsonga")),
        ("ba",     "Bashkir",               Some("bak_Cyrl"), Some("Bashkir")),
        ("tt",     "Tatar",                 Some("tat_Cyrl"), Some("Tatar")),
        ("bm",     "Bambara",               Some("bam_Latn"), Some("Bambara")),
        ("mos",    "Mossi",                 Some("mos_Latn"), Some("Mossi")),
        ("kab",    "Kabyle",                Some("kab_Latn"), Some("Kabyle")),
        ("umb",    "Umbundu",               Some("umb_Latn"), Some("Umbundu")),
        ("sag",    "Sango",                 Some("sag_Latn"), Some("Sango")),
        ("nso",    "Northern Sotho",        Some("nso_Latn"), Some("Northern Sotho")),
        ("nus",    "Nuer",                  Some("nus_Latn"), Some("Nuer")),
        ("grn",    "Guarani",               Some("grn_Latn"), Some("Guarani")),
        ("que",    "Quechua",               Some("quy_Latn"), Some("Quechua")),
        ("as",     "Assamese",              Some("asm_Beng"), Some("Assamese")),
        ("mai",    "Maithili",              Some("mai_Deva"), Some("Maithili")),
        ("mag",    "Magahi",                Some("mag_Deva"), Some("Magahi")),
        ("bho",    "Bhojpuri",              Some("bho_Deva"), Some("Bhojpuri")),
        ("awa",    "Awadhi",                Some("awa_Deva"), Some("Awadhi")),
        ("mni",    "Meitei",                Some("mni_Beng"), Some("Meitei")),
        ("sat",    "Santali",               Some("sat_Olck"), Some("Santali")),
        ("ary",    "Moroccan Arabic",       Some("ary_Arab"), Some("Moroccan Arabic")),
        ("arz",    "Egyptian Arabic",       Some("arz_Arab"), Some("Egyptian Arabic")),
        ("acm",    "Mesopotamian Arabic",   Some("acm_Arab"), Some("Mesopotamian Arabic")),
        ("prs",    "Dari",                  Some("prs_Arab"), Some("Dari")),
        ("azb",    "South Azerbaijani",     Some("azb_Arab"), Some("South Azerbaijani")),
        ("fur",    "Friulian",              Some("fur_Latn"), Some("Friulian")),
        ("lij",    "Ligurian",              Some("lij_Latn"), Some("Ligurian")),
        ("scn",    "Sicilian",              Some("scn_Latn"), Some("Sicilian")),
        ("vec",    "Venetian",              Some("vec_Latn"), Some("Venetian")),
        ("lmo",    "Lombard",               Some("lmo_Latn"), Some("Lombard")),
        ("ast",    "Asturian",              Some("ast_Latn"), Some("Asturian")),
        ("min",    "Minangkabau",           Some("min_Latn"), Some("Minangkabau")),
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

    #[test]
    fn translate_codes_always_lead_with_a_single_auto() {
        let entry = model_catalog()
            .into_iter()
            .find(|profile| profile.id == "nllb-200-distilled-600m-onnx")
            .expect("nllb entry exists");
        let codes = entry.language_codes_for_translate();
        // "Auto detect" is always offered first, and exactly once — the source list must
        // never inject a second "auto" alongside the synthetic one.
        assert_eq!(codes[0].code, "auto");
        assert_eq!(codes.iter().filter(|lang| lang.code == "auto").count(), 1);
        assert!(codes.len() > 1);
    }

    #[test]
    fn nllb_model_exposes_flores_codes() {
        let entry = model_catalog()
            .into_iter()
            .find(|profile| profile.id == "nllb-200-distilled-600m-onnx")
            .expect("nllb entry exists");
        let codes = entry.language_codes_for_translate();
        // NLLB uses FLORES-style codes (e.g. eng_Latn), not the short UI codes.
        assert!(codes.iter().any(|lang| lang.code.contains('_')));
    }

    #[test]
    fn non_nllb_model_uses_short_ui_codes() {
        let entry = model_catalog()
            .into_iter()
            .find(|profile| profile.engine == EngineKind::ManagedLlamaCpp)
            .expect("a managed llama model exists");
        let codes = entry.language_codes_for_translate();
        assert_eq!(codes[0].code, "auto");
        // A non-NLLB model must not leak FLORES "_Latn" codes; it speaks short UI codes.
        assert!(!codes.iter().any(|lang| lang.code.contains("_Latn")));
    }
}
