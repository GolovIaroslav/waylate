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
    pub license: String,
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
    Custom,
}

pub fn catalog() -> Vec<ModelProfile> {
    vec![
        ModelProfile {
            id: "hy-mt-gguf".into(),
            name: "Tencent Hy-MT GGUF".into(),
            provider: ProviderKind::OpenAiCompatible,
            description: "Quality-focused local MT model through llama.cpp or another OpenAI-compatible local server.".into(),
            license: "Check the selected Hugging Face repository before downloading.".into(),
            homepage: "https://huggingface.co/models?search=Hy-MT%20GGUF".into(),
            engine_hint: "Start llama.cpp server with a Hy-MT GGUF model, then keep the endpoint at http://127.0.0.1:8080/v1/chat/completions.".into(),
            default_endpoint: Some("http://127.0.0.1:8080/v1/chat/completions".into()),
            hf_repo: None,
            languages: popular_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "nllb-200-ct2".into(),
            name: "Meta NLLB-200 CTranslate2".into(),
            provider: ProviderKind::CTranslate2,
            description: "Wide-language local translation profile for converted NLLB CTranslate2 models.".into(),
            license: "CC-BY-NC 4.0 for Meta NLLB checkpoints; research/non-commercial constraints apply.".into(),
            homepage: "https://huggingface.co/facebook/nllb-200-distilled-600M".into(),
            engine_hint: "Install python dependencies ctranslate2 and transformers, then point Waylate to a converted CTranslate2 model directory.".into(),
            default_endpoint: None,
            hf_repo: Some("OpenNMT/nllb-200-distilled-1.3B-ct2-int8".into()),
            languages: nllb_languages(),
            downloadable: true,
        },
        ModelProfile {
            id: "deepl-api".into(),
            name: "DeepL API".into(),
            provider: ProviderKind::DeepL,
            description: "Network translation provider. Disabled by default; needs your own API key.".into(),
            license: "DeepL API terms".into(),
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
            license: "Google Cloud terms".into(),
            homepage: "https://cloud.google.com/translate/docs".into(),
            engine_hint: "Save a Google Cloud Translation API key in settings. Text is sent to Google only when this profile is selected.".into(),
            default_endpoint: Some("https://translation.googleapis.com/language/translate/v2".into()),
            hf_repo: None,
            languages: popular_languages(),
            downloadable: false,
        },
        ModelProfile {
            id: "custom-local".into(),
            name: "Custom local model".into(),
            provider: ProviderKind::Custom,
            description: "Manual profile for an already installed local translator or OpenAI-compatible endpoint.".into(),
            license: "User supplied".into(),
            homepage: "".into(),
            engine_hint: "Use this when you already run a local translation server and want Waylate to send prompts to it.".into(),
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
        ("zh", "Chinese"),
        ("ja", "Japanese"),
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
        ("zho_Hans", "Chinese Simplified"),
        ("jpn_Jpan", "Japanese"),
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
        assert_eq!(catalog[0].id, "hy-mt-gguf");
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
