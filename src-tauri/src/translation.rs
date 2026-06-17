use crate::{
    config::AppConfig,
    models::{catalog, ProviderKind},
    secrets,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;

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

pub fn translate(config: &AppConfig, request: &TranslationRequest) -> Result<TranslationResponse, String> {
    if request.text.trim().is_empty() {
        return Err("Nothing to translate".into());
    }

    let profile = catalog()
        .into_iter()
        .find(|item| item.id == request.model_id)
        .ok_or_else(|| format!("Unknown model profile: {}", request.model_id))?;

    match profile.provider {
        ProviderKind::OpenAiCompatible | ProviderKind::Custom => translate_openai_compatible(config, request),
        ProviderKind::CTranslate2 => translate_ctranslate2(config, request),
        ProviderKind::DeepL => translate_deepl(config, request),
        ProviderKind::Google => translate_google(config, request),
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

fn translate_ctranslate2(config: &AppConfig, request: &TranslationRequest) -> Result<TranslationResponse, String> {
    if config.ct2_model_path.trim().is_empty() {
        return Err("NLLB/CTranslate2 profile needs a converted model directory in Settings.".into());
    }
    if config.ct2_tokenizer_path.trim().is_empty() {
        return Err("NLLB/CTranslate2 profile needs a tokenizer path or Hugging Face tokenizer id in Settings.".into());
    }

    let helper = if config.ct2_helper_command.trim().is_empty() {
        "waylate-ct2-translate"
    } else {
        config.ct2_helper_command.trim()
    };

    let output = Command::new(helper)
        .arg("--model")
        .arg(&config.ct2_model_path)
        .arg("--tokenizer")
        .arg(&config.ct2_tokenizer_path)
        .arg("--device")
        .arg(&config.ct2_device)
        .arg("--source")
        .arg(&request.source_lang)
        .arg("--target")
        .arg(&request.target_lang)
        .arg(&request.text)
        .output()
        .map_err(|err| format!("Could not start CTranslate2 helper: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "CTranslate2 helper failed. Install ctranslate2 and transformers for Python.".into()
        } else {
            stderr
        });
    }

    Ok(TranslationResponse {
        translated_text: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        provider_label: "CTranslate2".into(),
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

fn ensure_network_enabled(config: &AppConfig) -> Result<(), String> {
    if config.api_provider_enabled {
        Ok(())
    } else {
        Err("Network API providers are disabled in Settings.".into())
    }
}
