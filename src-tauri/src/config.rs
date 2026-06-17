use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub model_id: String,
    pub source_lang: String,
    pub target_lang: String,
    pub history_enabled: bool,
    pub autostart: bool,
    pub openai_endpoint: String,
    pub openai_model: String,
    pub custom_model_path: String,
    pub ct2_model_path: String,
    pub ct2_tokenizer_path: String,
    pub ct2_helper_command: String,
    pub ct2_device: String,
    pub api_provider_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model_id: "hy-mt-gguf".into(),
            source_lang: "auto".into(),
            target_lang: "en".into(),
            history_enabled: false,
            autostart: true,
            openai_endpoint: "http://127.0.0.1:8080/v1/chat/completions".into(),
            openai_model: "local-translation-model".into(),
            custom_model_path: String::new(),
            ct2_model_path: String::new(),
            ct2_tokenizer_path: String::new(),
            ct2_helper_command: "waylate-ct2-translate".into(),
            ct2_device: "auto".into(),
            api_provider_enabled: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_file: PathBuf,
    pub history_db: PathBuf,
    pub models_dir: PathBuf,
}

impl AppPaths {
    pub fn new() -> Result<Self, String> {
        let dirs = ProjectDirs::from("dev", "jar", "Waylate")
            .ok_or_else(|| "Could not resolve XDG application directories".to_string())?;
        let config_dir = dirs.config_dir().to_path_buf();
        let data_dir = dirs.data_dir().to_path_buf();
        let cache_dir = dirs.cache_dir().to_path_buf();
        let config_file = config_dir.join("config.json");
        let history_db = data_dir.join("history.sqlite3");
        let models_dir = data_dir.join("models");
        Ok(Self {
            config_dir,
            data_dir,
            cache_dir,
            config_file,
            history_db,
            models_dir,
        })
    }

    pub fn ensure(&self) -> Result<(), String> {
        for path in [&self.config_dir, &self.data_dir, &self.cache_dir, &self.models_dir] {
            fs::create_dir_all(path).map_err(|err| format!("Could not create {}: {err}", path.display()))?;
        }
        Ok(())
    }
}

pub fn load(paths: &AppPaths) -> Result<AppConfig, String> {
    paths.ensure()?;
    if !paths.config_file.exists() {
        let config = AppConfig::default();
        save(paths, &config)?;
        return Ok(config);
    }

    let raw = fs::read_to_string(&paths.config_file)
        .map_err(|err| format!("Could not read {}: {err}", paths.config_file.display()))?;
    serde_json::from_str(&raw).map_err(|err| format!("Could not parse config: {err}"))
}

pub fn save(paths: &AppPaths, config: &AppConfig) -> Result<(), String> {
    paths.ensure()?;
    let raw = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;
    fs::write(&paths.config_file, raw)
        .map_err(|err| format!("Could not write {}: {err}", paths.config_file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_keep_history_and_api_off() {
        let config = AppConfig::default();
        assert_eq!(config.model_id, "hy-mt-gguf");
        assert!(!config.history_enabled);
        assert!(!config.api_provider_enabled);
        assert_eq!(config.ct2_helper_command, "waylate-ct2-translate");
    }

    #[test]
    fn missing_config_fields_fall_back_to_defaults() {
        let raw = r#"{"modelId":"hy-mt-gguf","sourceLang":"auto","targetLang":"ru"}"#;
        let config: AppConfig = serde_json::from_str(raw).expect("partial config should load");
        assert_eq!(config.target_lang, "ru");
        assert_eq!(config.openai_endpoint, AppConfig::default().openai_endpoint);
        assert_eq!(config.ct2_helper_command, "waylate-ct2-translate");
    }
}
