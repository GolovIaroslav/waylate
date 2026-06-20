use crate::{
    config::{AppConfig, AppPaths},
    models::{catalog, ProviderKind},
    runtime::{self, RuntimeManager},
    secrets,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationRequest {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationResponse {
    pub translated_text: String,
    pub provider_label: String,
    pub warning: Option<String>,
}

pub fn translate(
    paths: &AppPaths,
    runtime_manager: &RuntimeManager,
    config: &AppConfig,
    request: &TranslationRequest,
) -> Result<TranslationResponse, String> {
    if request.text.trim().is_empty() {
        return Err("Nothing to translate".into());
    }

    let profile = catalog()
        .into_iter()
        .find(|item| item.id == request.model_id)
        .ok_or_else(|| format!("Unknown model profile: {}", request.model_id))?;

    match profile.provider {
        ProviderKind::OpenAiCompatible => translate_openai_compatible(config, request),
        ProviderKind::Custom => translate_custom_local(paths, runtime_manager, config, request),
        ProviderKind::CTranslate2 => translate_ctranslate2(paths, runtime_manager, config, request),
        ProviderKind::DeepL => translate_deepl(config, request),
        ProviderKind::Google => translate_google(config, request),
        ProviderKind::Yandex => translate_yandex(config, request),
    }
}

fn translate_openai_compatible(config: &AppConfig, request: &TranslationRequest) -> Result<TranslationResponse, String> {
    let endpoint = if config.openai_endpoint.trim().is_empty() {
        "http://127.0.0.1:8080/v1/chat/completions"
    } else {
        config.openai_endpoint.trim()
    };
    let prompt = format!(
        "Translate the following text from {} to {}. Return only the translation.\n\n{}",
        request.source_lang, request.target_lang, request.text
    );
    let body = json!({
        "model": config.openai_model,
        "messages": [
            {"role": "system", "content": "You are a precise translation engine. Do not explain your answer."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "stream": false
    });

    let mut builder = Client::new().post(endpoint).json(&body);
    if let Ok(key) = secrets::get("openai-compatible") {
        builder = builder.bearer_auth(key);
    }

    let value: Value = builder
        .send()
        .map_err(|err| format!("Could not reach local OpenAI-compatible server at {endpoint}: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Local translation server returned an error: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse local server response: {err}"))?;

    let translated = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/choices/0/text").and_then(Value::as_str))
        .ok_or_else(|| "Local server response did not contain a translation".to_string())?;

    Ok(TranslationResponse {
        translated_text: translated.trim().to_string(),
        provider_label: "OpenAI-compatible local server".into(),
        warning: None,
    })
}

fn translate_custom_local(
    paths: &AppPaths,
    runtime_manager: &RuntimeManager,
    config: &AppConfig,
    request: &TranslationRequest,
) -> Result<TranslationResponse, String> {
    if config.custom_backend_mode == "managed-gguf" {
        let (translated, device) = runtime::translate_via_managed_llama(
            runtime_manager,
            paths,
            config,
            &request.model_id,
            &request.source_lang,
            &request.target_lang,
            &request.text,
        )?;
        return Ok(TranslationResponse {
            translated_text: translated,
            provider_label: format!("Managed local GGUF ({device})"),
            warning: None,
        });
    }

    translate_openai_compatible(config, request)
}

fn translate_ctranslate2(
    paths: &AppPaths,
    runtime_manager: &RuntimeManager,
    config: &AppConfig,
    request: &TranslationRequest,
) -> Result<TranslationResponse, String> {
    if config.ct2_model_path.trim().is_empty() {
        return Err("This model is not installed - Download it in Settings.".into());
    }
    if config.ct2_tokenizer_path.trim().is_empty() {
        return Err("This model is not installed - Download it in Settings.".into());
    }

    let (translated, device) = runtime::translate_via_warm_ct2(
        runtime_manager,
        paths,
        config,
        &request.model_id,
        &request.source_lang,
        &request.target_lang,
        &request.text,
    )?;

    Ok(TranslationResponse {
        translated_text: translated,
        provider_label: format!("Warm local NLLB ({device})"),
        warning: None,
    })
}

fn translate_deepl(config: &AppConfig, request: &TranslationRequest) -> Result<TranslationResponse, String> {
    ensure_network_enabled(config)?;
    let key = secrets::get("deepl")?;
    let endpoint = "https://api-free.deepl.com/v2/translate";
    let mut form = vec![
        ("text", request.text.as_str()),
        ("target_lang", request.target_lang.as_str()),
    ];
    if request.source_lang != "auto" {
        form.push(("source_lang", request.source_lang.as_str()));
    }

    let value: Value = Client::new()
        .post(endpoint)
        .header("Authorization", format!("DeepL-Auth-Key {key}"))
        .form(&form)
        .send()
        .map_err(|err| format!("Could not reach DeepL: {err}"))?
        .error_for_status()
        .map_err(|err| format!("DeepL returned an error: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse DeepL response: {err}"))?;

    let translated = value
        .pointer("/translations/0/text")
        .and_then(Value::as_str)
        .ok_or_else(|| "DeepL response did not contain a translation".to_string())?;

    Ok(TranslationResponse {
        translated_text: translated.to_string(),
        provider_label: "DeepL API".into(),
        warning: Some("Text was sent to DeepL because the DeepL profile is selected.".into()),
    })
}

fn translate_google(config: &AppConfig, request: &TranslationRequest) -> Result<TranslationResponse, String> {
    ensure_network_enabled(config)?;
    let key = secrets::get("google")?;
    let value: Value = Client::new()
        .post(format!("https://translation.googleapis.com/language/translate/v2?key={key}"))
        .form(&[
            ("q", request.text.as_str()),
            ("target", request.target_lang.as_str()),
            ("source", request.source_lang.as_str()),
            ("format", "text"),
        ])
        .send()
        .map_err(|err| format!("Could not reach Google Translate: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Google Translate returned an error: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse Google Translate response: {err}"))?;

    let translated = value
        .pointer("/data/translations/0/translatedText")
        .and_then(Value::as_str)
        .ok_or_else(|| "Google response did not contain a translation".to_string())?;

    Ok(TranslationResponse {
        translated_text: translated.to_string(),
        provider_label: "Google Cloud Translate".into(),
        warning: Some("Text was sent to Google because the Google profile is selected.".into()),
    })
}

fn translate_yandex(config: &AppConfig, request: &TranslationRequest) -> Result<TranslationResponse, String> {
    ensure_network_enabled(config)?;
    let key = secrets::get("yandex")?;
    if config.yandex_folder_id.trim().is_empty() {
        return Err("Yandex Cloud Translate needs a Folder ID in Settings.".into());
    }

    let mut body = json!({
        "folderId": config.yandex_folder_id.trim(),
        "targetLanguageCode": request.target_lang,
        "texts": [request.text],
    });
    if request.source_lang != "auto" {
        body["sourceLanguageCode"] = json!(request.source_lang);
    }

    let value: Value = Client::new()
        .post("https://translate.api.cloud.yandex.net/translate/v2/translate")
        .header("Authorization", format!("Api-Key {key}"))
        .json(&body)
        .send()
        .map_err(|err| format!("Could not reach Yandex Cloud Translate: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Yandex Cloud Translate returned an error: {err}"))?
        .json()
        .map_err(|err| format!("Could not parse Yandex Cloud Translate response: {err}"))?;

    let translated = value
        .pointer("/translations/0/text")
        .and_then(Value::as_str)
        .ok_or_else(|| "Yandex Cloud Translate response did not contain a translation".to_string())?;

    Ok(TranslationResponse {
        translated_text: translated.to_string(),
        provider_label: "Yandex Cloud Translate".into(),
        warning: Some("Text was sent to Yandex because the Yandex profile is selected.".into()),
    })
}

fn ensure_network_enabled(config: &AppConfig) -> Result<(), String> {
    if config.api_provider_enabled {
        Ok(())
    } else {
        Err("Network API providers are disabled in Settings.".into())
    }
}
