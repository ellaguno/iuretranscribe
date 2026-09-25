//! Resumen y minuta vía OpenRouter (API compatible con OpenAI).

use serde::Deserialize;
use iurefficient_connect::{lang, tr};
use serde_json::json;

#[derive(Deserialize)]
struct ChatResponse {
    choices: Option<Vec<Choice>>,
    error: Option<serde_json::Value>,
}
#[derive(Deserialize)]
struct Choice {
    message: Message,
}
#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

pub async fn chat(api_key: &str, model: &str, system: &str, user: &str) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Err(tr!("Set your OpenRouter key in Settings to generate summaries and minutes.", "Configura tu llave de OpenRouter en Ajustes para generar resúmenes y minutas."));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    });
    let resp = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .bearer_auth(api_key.trim())
        .header("HTTP-Referer", "https://github.com/ellaguno/iuretranscribe")
        .header("X-Title", "IureTranscribe")
        .json(&body)
        .send()
        .await
        .map_err(|e| tr!("Could not connect to OpenRouter: {e}", "No se pudo conectar con OpenRouter: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let parsed: ChatResponse = serde_json::from_str(&text)
        .map_err(|_| tr!("OpenRouter responded {status}: {}", "OpenRouter respondió {status}: {}", truncate(&text, 300)))?;
    if let Some(err) = parsed.error {
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or(lang::pick("unknown error", "error desconocido"));
        return Err(format!("OpenRouter: {msg}"));
    }
    parsed
        .choices
        .and_then(|c| c.into_iter().next())
        .and_then(|c| c.message.content)
        .filter(|c| !c.trim().is_empty())
        .ok_or_else(|| tr!("OpenRouter returned no content.", "OpenRouter no devolvió contenido."))
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}
