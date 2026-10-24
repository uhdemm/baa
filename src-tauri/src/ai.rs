//! Optional AI pass: clean up a dictation, or rewrite selected text by voice
//! (command mode). Works with Claude, OpenAI, a local Ollama, or any
//! OpenAI-compatible server. API keys live in Windows Credential Manager.

use crate::settings::AiSettings;
use serde_json::{json, Value};
use std::time::Duration;

const KEYRING_SERVICE: &str = "rip.baa.app";

pub fn set_key(provider: &str, key: &str) -> anyhow::Result<()> {
    let e = keyring::Entry::new(KEYRING_SERVICE, provider)?;
    if key.is_empty() {
        let _ = e.delete_credential();
    } else {
        e.set_password(key)?;
    }
    Ok(())
}

pub fn get_key(provider: &str) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, provider).ok()?.get_password().ok()
}

pub fn has_key(provider: &str) -> bool {
    get_key(provider).is_some()
}

/// Hosted services that need a key. Groq, Gemini, Cerebras and OpenRouter all have free tiers.
pub const KEYED: &[&str] = &["anthropic", "openai", "groq", "gemini", "cerebras", "openrouter", "freellmapi", "custom"];

/// Whether the configured provider can run (local servers need no key).
pub fn ready(ai: &AiSettings) -> bool {
    match ai.provider.as_str() {
        "ollama" | "freellmapi" => true,
        "custom" => !ai.base_url.trim().is_empty(),
        p => has_key(p),
    }
}

fn base_url(ai: &AiSettings) -> String {
    let b = ai.base_url.trim().trim_end_matches('/');
    match (ai.provider.as_str(), b.is_empty()) {
        (_, false) => b.to_string(),
        ("anthropic", true) => "https://api.anthropic.com".into(),
        ("openai", true) => "https://api.openai.com/v1".into(),
        ("ollama", true) => "http://localhost:11434/v1".into(),
        ("groq", true) => "https://api.groq.com/openai/v1".into(),
        ("gemini", true) => "https://generativelanguage.googleapis.com/v1beta/openai".into(),
        ("cerebras", true) => "https://api.cerebras.ai/v1".into(),
        ("openrouter", true) => "https://openrouter.ai/api/v1".into(),
        ("freellmapi", true) => "http://localhost:3001/v1".into(),
        _ => String::new(),
    }
}

/// One request/response round with the configured model.
pub fn complete(ai: &AiSettings, system: &str, user: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(ai.timeout_secs.max(2))).build()?;
    let url = base_url(ai);
    let model = ai.model.trim().to_string();
    if model.is_empty() {
        anyhow::bail!("Choose a model in Settings > AI");
    }
    tauri::async_runtime::block_on(async move {
        if ai.provider == "anthropic" {
            let key = get_key("anthropic").ok_or_else(|| anyhow::anyhow!("Add your Anthropic API key in Settings > AI"))?;
            // Fast, short job: low effort keeps latency down. "default" fallbacks reroute a declined request.
            let body = json!({
                "model": model,
                "max_tokens": 4096,
                "system": system,
                "messages": [{"role": "user", "content": user}],
                "output_config": {"effort": "low"},
                "fallbacks": "default",
            });
            let resp = client
                .post(format!("{url}/v1/messages"))
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", "server-side-fallback-2026-07-01")
                .json(&body)
                .send()
                .await?;
            let status = resp.status();
            let v: Value = resp.json().await?;
            if !status.is_success() {
                anyhow::bail!("Claude: {}", v["error"]["message"].as_str().unwrap_or(status.as_str()));
            }
            if v["stop_reason"] == "refusal" {
                anyhow::bail!("Claude declined this one");
            }
            // thinking blocks may come first; join the text blocks
            let text: String = v["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect();
            Ok(text.trim().to_string())
        } else {
            let mut req = client.post(format!("{url}/chat/completions")).json(&json!({
                "model": model,
                "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
            }));
            if let Some(key) = get_key(&ai.provider) {
                req = req.bearer_auth(key);
            }
            let resp = req.send().await?;
            let status = resp.status();
            let v: Value = resp.json().await?;
            if !status.is_success() {
                anyhow::bail!("AI: {}", v["error"]["message"].as_str().unwrap_or(status.as_str()));
            }
            Ok(v["choices"][0]["message"]["content"].as_str().unwrap_or_default().trim().to_string())
        }
    })
}

/// The provider's own model list, so nobody has to guess model names.
pub fn list_models(ai: &AiSettings) -> anyhow::Result<Vec<String>> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;
    let url = base_url(ai);
    if url.is_empty() {
        anyhow::bail!("Enter the server address first");
    }
    let provider = ai.provider.clone();
    tauri::async_runtime::block_on(async move {
        let req = if provider == "anthropic" {
            let key = get_key("anthropic").ok_or_else(|| anyhow::anyhow!("Save your Anthropic key first"))?;
            client.get(format!("{url}/v1/models?limit=100")).header("x-api-key", key).header("anthropic-version", "2023-06-01")
        } else {
            let mut r = client.get(format!("{url}/models"));
            if let Some(key) = get_key(&provider) {
                r = r.bearer_auth(key);
            }
            r
        };
        let resp = req.send().await?;
        let status = resp.status();
        let v: Value = resp.json().await?;
        if !status.is_success() {
            anyhow::bail!("{}", v["error"]["message"].as_str().unwrap_or(status.as_str()));
        }
        let mut ids: Vec<String> = v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["id"].as_str())
            .map(|id| id.trim_start_matches("models/").to_string()) // Gemini prefixes ids
            .collect();
        ids.sort();
        Ok(ids)
    })
}

/// Dictation clean-up. Falls back to the original text on any failure so a slow
/// or offline AI never eats what you said.
/// Where a dictation is going and what is already there.
#[derive(Default)]
pub struct Where<'a> {
    pub app: Option<&'a str>,
    pub title: Option<&'a str>,
    /// text just before the cursor, so the clean-up can continue it
    pub before: Option<&'a str>,
    /// "formal" | "casual" | "very_casual" | "excited"
    pub style: Option<&'a str>,
}

pub fn cleanup(ai: &AiSettings, text: &str, at: &Where, custom_words: &[String]) -> Result<String, String> {
    let mut system = ai.prompt.clone();
    if !custom_words.is_empty() {
        system.push_str(&format!("\n\nNAMES AND TERMS\nSpell these exactly as written here: {}.", custom_words.join(", ")));
    }
    let context = match (at.app, at.title.filter(|t| !t.trim().is_empty())) {
        (Some(a), Some(t)) => format!("{a}: {t}"),
        (Some(a), None) => a.to_string(),
        (None, Some(t)) => t.to_string(),
        (None, None) => "unknown".into(),
    };
    let mut user = format!("<context>{context}</context>\n");
    if let Some(st) = at.style {
        user.push_str(&format!("<style>{}</style>\n", st.replace('_', " ")));
    }
    if let Some(b) = at.before.filter(|b| !b.trim().is_empty()) {
        user.push_str(&format!("<before_cursor>{b}</before_cursor>\n"));
    }
    user.push_str(&format!("<dictation>{text}</dictation>"));
    match complete(ai, &system, &user) {
        Ok(out) if !out.is_empty() => Ok(strip_wrapper(&out)),
        Ok(_) => Err("AI returned nothing".into()),
        Err(e) => Err(e.to_string()),
    }
}

const MEETING_PROMPT: &str = include_str!("meeting_prompt.txt");

/// Turns a meeting transcript into notes. With speaker labels the reply starts
/// with a "SPEAKERS:" line naming them, then the title, then the sections.
pub fn meeting_notes(ai: &AiSettings, transcript: &str, attendees: &str, my_notes: &str, template: &str) -> anyhow::Result<String> {
    // a long meeting needs more time than a dictation
    let mut ai = ai.clone();
    ai.timeout_secs = ai.timeout_secs.max(120);
    let template = if template.is_empty() { "general" } else { template };
    let user = format!(
        "<template>{template}</template>\n<attendees>{}</attendees>\n<my_notes>\n{}\n</my_notes>\n<transcript>\n{transcript}\n</transcript>",
        if attendees.trim().is_empty() { "not given" } else { attendees.trim() },
        my_notes.trim()
    );
    Ok(strip_wrapper(&complete(&ai, MEETING_PROMPT, &user)?))
}

/// Answers a question about one note or meeting, using only what is in it.
pub fn ask_note(ai: &AiSettings, title: &str, body: &str, question: &str) -> anyhow::Result<String> {
    let mut ai = ai.clone();
    ai.timeout_secs = ai.timeout_secs.max(60);
    let system = "You answer questions about one note or meeting. Use only what is in it; if the answer isn't there, \
        say so in one sentence. When asked to write something (an email, a message, a summary), write it ready to send, \
        in the user's voice, with no preamble. Plain text; \"- \" for lists, no other markdown.";
    let user = format!("<note title=\"{title}\">\n{body}\n</note>\n\n{question}");
    Ok(strip_wrapper(&complete(&ai, system, &user)?))
}

/// Command mode: apply a spoken instruction to the selected text (or write new text).
pub fn command(ai: &AiSettings, instruction: &str, selection: Option<&str>) -> anyhow::Result<String> {
    let system = "You edit text for the user, who speaks instructions out loud. Follow the instruction and reply \
        with only the resulting text, ready to paste: no explanations, no quotes, no markdown fences.";
    let user = match selection {
        Some(sel) => format!("Instruction: {instruction}\n\n<text>\n{sel}\n</text>"),
        None => format!("Instruction (no text is selected, so write new text): {instruction}"),
    };
    let out = complete(ai, system, &user)?;
    if out.is_empty() {
        anyhow::bail!("AI returned nothing");
    }
    Ok(strip_wrapper(&out))
}

/// Models sometimes echo the wrapper tags back.
fn strip_wrapper(s: &str) -> String {
    let mut t = s.trim();
    for tag in ["dictation", "text"] {
        let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
        if t.starts_with(&open) && t.ends_with(&close) {
            t = t[open.len()..t.len() - close.len()].trim();
        }
    }
    t.to_string()
}
