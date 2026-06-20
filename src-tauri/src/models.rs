use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Language {
    pub code: String,
    pub name: String,
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

pub fn catalog() -> Vec<ModelProfile> {
    vec![
        ModelProfile {
            id: "nllb-200-ct2".into(),
            name: "NLLB-200 small".into(),
            provider: ProviderKind::CTranslate2,
            description: "Best first choice. Downloads once and works offline.".into(),
            quantization: "Balanced".into(),
            size: "~2.4 GB".into(),
            homepage: "https://huggingface.co/entai2965/nllb-200-distilled-600M-ctranslate2".into(),
            engine_hint: "Waylate downloads this model and configures local translation automatically.".into(),
            default_endpoint: None,
            hf_repo: Some("entai2965/nllb-200-distilled-600M-ctranslate2".into()),
            languages: nllb_languages(),
            downloadable: true,
        },
        ModelProfile {
            id: "nllb-200-1-3b-ct2".into(),
            name: "NLLB-200 medium".into(),
            provider: ProviderKind::CTranslate2,
            description: "Better quality, slower and heavier.".into(),
            quantization: "Balanced".into(),
            size: "~5.5 GB".into(),
            homepage: "https://huggingface.co/entai2965/nllb-200-distilled-1.3B-ctranslate2".into(),
            engine_hint: "Waylate downloads this model and configures local translation automatically.".into(),
            default_endpoint: None,
            hf_repo: Some("entai2965/nllb-200-distilled-1.3B-ctranslate2".into()),
            languages: nllb_languages(),
            downloadable: true,
        },
        ModelProfile {
            id: "nllb-200-3-3b-ct2".into(),
            name: "NLLB-200 large".into(),
            provider: ProviderKind::CTranslate2,
            description: "Highest quality option. Needs much more disk and memory.".into(),
            quantization: "Balanced".into(),
            size: "~13 GB".into(),
            homepage: "https://huggingface.co/entai2965/nllb-200-3.3B-ctranslate2".into(),
            engine_hint: "Waylate downloads this model and configures local translation automatically.".into(),
            default_endpoint: None,
            hf_repo: Some("entai2965/nllb-200-3.3B-ctranslate2".into()),
            languages: nllb_languages(),
            downloadable: true,
        },
        ModelProfile {
            id: "tencent-hy-mt2-1-25bit-gguf".into(),
            name: "Tencent Hy-MT2 1.8B".into(),
            provider: ProviderKind::OpenAiCompatible,
            description: "Advanced custom model. Needs a built-in runner before it can be one-click.".into(),
            quantization: "1.25-bit GGUF".into(),
            size: "~440 MB".into(),
            homepage: "https://huggingface.co/tencent/Hy-MT2-1.8B-1.25Bit-GGUF".into(),
            engine_hint: "Advanced only until Waylate bundles a GGUF runner.".into(),
            default_endpoint: Some("http://127.0.0.1:8080/v1/chat/completions".into()),
            hf_repo: Some("tencent/Hy-MT2-1.8B-1.25Bit-GGUF".into()),
            languages: tencent_languages(),
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
}
