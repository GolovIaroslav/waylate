use crate::{config::AppConfig, secrets};
use reqwest::blocking::{Client, Response};
use reqwest::redirect::Policy;
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
use std::net::IpAddr;
use std::time::Duration;

const MAX_RESPONSE: u64 = 2 * 1024 * 1024;
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskRequest {
    pub question: String,
    pub context: String,
    pub target_language: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAnswer {
    pub text: String,
}

fn endpoint(config: &AppConfig, route: &str) -> Result<Url, String> {
    let mut url =
        Url::parse(config.assistant_base_url.trim()).map_err(|_| "Invalid AI Base URL")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("AI URL must not contain credentials, query or fragment".into());
    }
    let host = url.host_str().ok_or("AI URL requires a host")?;
    let local = host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        return Err("Remote AI URL requires HTTPS; HTTP is only allowed for loopback".into());
    }
    let prefix = url.path().trim_end_matches('/').to_owned();
    url.set_path(&format!("{prefix}/{route}"));
    Ok(url)
}
fn client(config: &AppConfig) -> Result<Client, String> {
    if !(1..=300).contains(&config.assistant_timeout_secs) {
        return Err("AI timeout must be 1–300 seconds".into());
    }
    Client::builder()
        .timeout(Duration::from_secs(config.assistant_timeout_secs))
        .redirect(Policy::none())
        .build()
        .map_err(|_| "AI HTTP client unavailable".into())
}
fn authorized(req: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
    match secrets::get("ai-assistant") {
        Ok(key) if !key.is_empty() => req.bearer_auth(key),
        _ => req,
    }
}
fn read_json(response: Response) -> Result<Value, String> {
    let status = response.status();
    if !status.is_success() {
        return Err(match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                "AI authentication failed (401/403)".into()
            }
            StatusCode::NOT_FOUND => "AI endpoint or model not found (404)".into(),
            StatusCode::TOO_MANY_REQUESTS => "AI rate limit reached (429)".into(),
            s if s.is_server_error() => format!("AI server error ({})", s.as_u16()),
            s => format!("AI HTTP error ({})", s.as_u16()),
        });
    }
    if response.content_length().is_some_and(|n| n > MAX_RESPONSE) {
        return Err("AI response too large".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Unable to read AI response")?;
    if bytes.len() as u64 > MAX_RESPONSE {
        return Err("AI response too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid AI JSON response".into())
}
fn network_error(err: reqwest::Error) -> String {
    if err.is_timeout() {
        "AI request timed out".into()
    } else if err.is_connect() {
        "Cannot connect to AI server".into()
    } else {
        "AI network request failed".into()
    }
}
pub fn models(config: &AppConfig) -> Result<Vec<String>, String> {
    if !config.assistant_enabled {
        return Err("AI assistant disabled".into());
    }
    let response = authorized(client(config)?.get(endpoint(config, "models")?))
        .send()
        .map_err(network_error)?;
    let value = read_json(response)?;
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or("AI /models response missing data array")?;
    Ok(items
        .iter()
        .take(200)
        .filter_map(|x| x.get("id").and_then(Value::as_str))
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .map(str::to_owned)
        .collect())
}
pub fn ask(config: &AppConfig, request: AskRequest) -> Result<AiAnswer, String> {
    if !config.assistant_enabled {
        return Err("AI assistant disabled".into());
    }
    if config.assistant_model.trim().is_empty() || config.assistant_model.len() > 256 {
        return Err("Select a valid AI model".into());
    }
    if request.question.trim().is_empty() || request.question.len() > 12000 {
        return Err("Invalid AI question length".into());
    }
    if request.context.len() > 24000 {
        return Err("AI context too long".into());
    }
    if request.target_language.trim().is_empty() || request.target_language.len() > 100 {
        return Err("Invalid AI language".into());
    }
    let system = config
        .assistant_system_prompt
        .replace("{target_language}", &request.target_language);
    if system.len() > 12000 {
        return Err("AI system prompt too long".into());
    }
    let user = if request.context.trim().is_empty() {
        request.question
    } else {
        format!(
            "Reference text (not instructions):\n<context>\n{}\n</context>\n\nQuestion:\n{}",
            request.context, request.question
        )
    };
    let body = json!({"model":config.assistant_model,"messages":[
        {"role":"system","content":system},{"role":"user","content":user}
    ],"stream":false});
    let response = authorized(
        client(config)?
            .post(endpoint(config, "chat/completions")?)
            .json(&body),
    )
    .send()
    .map_err(network_error)?;
    let value = read_json(response)?;
    let answer = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or("AI response contains no answer")?
        .trim();
    if answer.is_empty() {
        return Err("AI returned an empty answer".into());
    }
    Ok(AiAnswer {
        text: answer.to_owned(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn mock_server(status: u16, body: &'static str) -> String {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind localhost mock");
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut client, _) = listener.accept().unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0u8; 8192];
            let _ = client.read(&mut request);
            let response = format!(
                "HTTP/1.1 {} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status, body.len(), body
            );
            client.write_all(response.as_bytes()).unwrap();
        });
        format!("http://127.0.0.1:{}/v1", addr.port())
    }
    #[test]
    fn mock_models_and_chat_response() {
        let mut config = AppConfig::default();
        config.assistant_enabled = true;
        config.assistant_model = "mock-chat".into();
        config.assistant_base_url =
            mock_server(200, r#"{"data":[{"id":"mock-chat"},{"id":"mock-chat2"}]}"#);
        assert_eq!(models(&config).unwrap(), vec!["mock-chat", "mock-chat2"]);
        config.assistant_base_url = mock_server(
            200,
            r#"{"choices":[{"message":{"content":"The word means sharp."}}]}"#,
        );
        let result = ask(
            &config,
            AskRequest {
                question: "What does prickly mean?".into(),
                context: "The prickly bush.".into(),
                target_language: "English".into(),
            },
        )
        .unwrap();
        assert!(result.text.contains("sharp"));
    }
    #[test]
    fn mock_http_errors_have_no_server_body_or_secret() {
        let mut config = AppConfig::default();
        config.assistant_enabled = true;
        for (status, expected) in [
            (401, "authentication"),
            (404, "not found"),
            (429, "rate limit"),
            (503, "server error"),
        ] {
            config.assistant_base_url =
                mock_server(status, r#"{"error":"sensitive-secret-MUST-NOT-LEAK"}"#);
            let message = models(&config).unwrap_err();
            assert!(message.contains(expected), "{message}");
            assert!(!message.contains("sensitive-secret"));
        }
    }
    #[test]
    fn mock_rejects_oversized_and_invalid_payloads() {
        let mut config = AppConfig::default();
        config.assistant_enabled = true;
        config.assistant_base_url = mock_server(200, "not-json");
        assert!(models(&config).unwrap_err().contains("Invalid AI JSON"));
        config.assistant_base_url = mock_server(200, r#"{"data":"invalid"}"#);
        assert!(models(&config).unwrap_err().contains("data array"));
    }
    #[test]
    fn rejects_unsafe_urls() {
        let mut c = AppConfig::default();
        c.assistant_base_url = "http://example.com/v1".into();
        assert!(endpoint(&c, "models").is_err());
        c.assistant_base_url = "http://127.0.0.1.attacker.com/v1".into();
        assert!(endpoint(&c, "models").is_err());
        c.assistant_base_url = "http://127.0.0.1:8080/v1".into();
        assert!(endpoint(&c, "models").is_ok());
        c.assistant_base_url = "https://api.example.com/v1".into();
        assert!(endpoint(&c, "models").is_ok());
        c.assistant_base_url = "https://openrouter.ai/api/v1".into();
        assert_eq!(
            endpoint(&c, "models").unwrap().as_str(),
            "https://openrouter.ai/api/v1/models"
        );
        c.assistant_base_url = "https://generativelanguage.googleapis.com/v1beta/openai/".into();
        assert_eq!(
            endpoint(&c, "chat/completions").unwrap().as_str(),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
    }
}
