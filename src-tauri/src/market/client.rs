//! OpenAI 兼容流式客户端（移植自 ai/deepseek_client.py）。
//! 支持 reasoning_content 思考流、thinking/effort 参数适配、usage 统计。

use super::types::ProviderSettings;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Default)]
pub struct AiUsage {
    pub prompt_tokens: u64,
    pub cached_prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

impl AiUsage {
    pub fn cache_hit_rate(&self) -> f64 {
        if self.prompt_tokens == 0 {
            0.0
        } else {
            self.cached_prompt_tokens as f64 / self.prompt_tokens as f64
        }
    }
    pub fn merge(&mut self, other: &AiUsage) {
        self.prompt_tokens += other.prompt_tokens;
        self.cached_prompt_tokens += other.cached_prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
    }
}

#[derive(Debug, Clone, Default)]
pub struct AiReply {
    pub content: String,
    pub reasoning_content: String,
    pub usage: AiUsage,
    pub latency_ms: u64,
}

pub struct StreamCallbacks<'a> {
    pub on_reasoning: &'a (dyn Fn(&str) + Sync),
    pub on_content: &'a (dyn Fn(&str) + Sync),
}

fn resolve_thinking_params(settings: &ProviderSettings) -> (Value, Option<String>) {
    let base = settings.base_url.to_lowercase();
    let model = settings.model.to_lowercase();
    let is_deepseek = base.contains("deepseek.com") || model.contains("deepseek");
    let is_minimax = model.contains("minimax") || base.contains("minimax");
    let is_claude = model.contains("claude");
    let is_packy = base.contains("packy");
    let adaptive_claude =
        model.contains("opus-4") || model.contains("sonnet-4") || model.contains("claude-4");

    let effort = match settings.reasoning_effort.as_str() {
        "none" | "minimal" | "low" => "low",
        "medium" => "medium",
        "high" => "high",
        _ => "max",
    };

    if !settings.thinking {
        let extra = if is_deepseek || is_minimax {
            json!({"thinking": {"type": "disabled"}})
        } else {
            json!({})
        };
        return (extra, None);
    }

    if is_deepseek {
        return (
            json!({"thinking": {"type": "adaptive"}, "output_config": {"effort": effort}}),
            None,
        );
    }
    if is_minimax {
        return (
            json!({"thinking": {"type": "adaptive"}, "reasoning_split": true}),
            None,
        );
    }
    if is_packy && is_claude {
        let budget = 524_287u64.min(128_000u64.saturating_sub(1).max(1024));
        return (
            json!({"thinking": {"type": "enabled", "budget_tokens": budget}}),
            None,
        );
    }
    if adaptive_claude {
        return (
            json!({"thinking": {"type": "adaptive"}, "output_config": {"effort": effort}}),
            Some(effort.to_string()),
        );
    }
    if is_claude {
        let budget = 524_287u64.min(524_288u64.saturating_sub(1).max(1024));
        return (
            json!({"thinking": {"type": "enabled", "budget_tokens": budget}}),
            Some(effort.to_string()),
        );
    }
    (json!({}), Some(effort.to_string()))
}

fn provider_max_output_tokens(settings: &ProviderSettings) -> u64 {
    let base = settings.base_url.to_lowercase();
    let model = settings.model.to_lowercase();
    if base.contains("packy") && model.contains("claude") {
        128_000
    } else if base.contains("deepseek.com") || model.contains("deepseek") {
        393_216
    } else {
        32_768
    }
}

pub fn build_request_body(settings: &ProviderSettings, messages: &[Value]) -> Value {
    let (extra_body, reasoning_effort) = resolve_thinking_params(settings);
    let mut body = json!({
        "model": settings.model,
        "messages": messages,
        "max_tokens": provider_max_output_tokens(settings),
    });
    if let Some(object) = extra_body.as_object() {
        for (key, value) in object {
            body[key] = value.clone();
        }
    }
    if let Some(effort) = reasoning_effort {
        body["reasoning_effort"] = json!(effort);
    } else {
        body["temperature"] = json!(0);
    }
    body
}

#[derive(Debug)]
pub enum ClientError {
    Network(String),
    Status(u16, String),
    Cancelled,
    Timeout,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Network(detail) => write!(formatter, "网络错误：{detail}"),
            ClientError::Status(status, detail) => write!(formatter, "服务返回 {status}：{detail}"),
            ClientError::Cancelled => write!(formatter, "已取消"),
            ClientError::Timeout => write!(formatter, "请求超时"),
        }
    }
}

fn parse_usage(value: &Value) -> AiUsage {
    let prompt = value
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .or_else(|| value.get("input_tokens").and_then(Value::as_u64))
        .unwrap_or(0);
    let cached = value
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_u64)
        .or_else(|| value.get("prompt_cache_hit_tokens").and_then(Value::as_u64))
        .or_else(|| value.get("cache_read_input_tokens").and_then(Value::as_u64))
        .unwrap_or(0)
        .min(prompt);
    let completion = value
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .or_else(|| value.get("output_tokens").and_then(Value::as_u64))
        .unwrap_or(0);
    let total = value
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(prompt + completion);
    AiUsage {
        prompt_tokens: prompt,
        cached_prompt_tokens: cached,
        completion_tokens: completion,
        total_tokens: total,
    }
}

pub async fn stream_chat(
    client: &reqwest::Client,
    settings: &ProviderSettings,
    messages: &[Value],
    cancellation: &CancellationToken,
    callbacks: Option<&StreamCallbacks<'_>>,
) -> Result<AiReply, ClientError> {
    let started = std::time::Instant::now();
    let mut body = build_request_body(settings, messages);
    body["stream"] = json!(true);
    body["stream_options"] = json!({"include_usage": true});

    let url = format!(
        "{}/chat/completions",
        settings.base_url.trim_end_matches('/')
    );
    let mut attempt = 0u32;
    let response = loop {
        if cancellation.is_cancelled() {
            return Err(ClientError::Cancelled);
        }
        let result = tokio::select! {
            _ = cancellation.cancelled() => return Err(ClientError::Cancelled),
            result = client
                .post(&url)
                .bearer_auth(&settings.api_key)
                .json(&body)
                .send() => result,
        };
        let response = match result {
            Ok(response) => response,
            Err(error) => return Err(ClientError::Network(error.to_string())),
        };
        let status = response.status();
        if status.is_success() {
            break response;
        }
        let detail: String = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(512)
            .collect();
        let retryable =
            status.as_u16() == 429 || status.as_u16() == 408 || status.is_server_error();
        if !retryable || attempt >= 3 {
            return Err(ClientError::Status(status.as_u16(), detail));
        }
        let wait = Duration::from_secs(1u64 << attempt);
        attempt += 1;
        tokio::select! {
            _ = cancellation.cancelled() => return Err(ClientError::Cancelled),
            _ = tokio::time::sleep(wait) => {}
        }
    };

    let mut stream = response.bytes_stream().eventsource();
    let mut reply = AiReply::default();
    loop {
        let event = tokio::select! {
            _ = cancellation.cancelled() => return Err(ClientError::Cancelled),
            event = stream.next() => event,
        };
        let Some(event) = event else { break };
        let event = event.map_err(|error| ClientError::Network(error.to_string()))?;
        let data = event.data.trim().to_string();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            break;
        }
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if let Some(usage) = value.get("usage").filter(|v| !v.is_null()) {
            let parsed = parse_usage(usage);
            if parsed.total_tokens > 0 {
                reply.usage = parsed;
            }
        }
        let delta = value.pointer("/choices/0/delta");
        let Some(delta) = delta else { continue };
        // 思考流优先（reasoning_content / reasoning）
        let reasoning = delta
            .get("reasoning_content")
            .and_then(Value::as_str)
            .or_else(|| delta.get("reasoning").and_then(Value::as_str));
        if let Some(chunk) = reasoning.filter(|s| !s.is_empty()) {
            reply.reasoning_content.push_str(chunk);
            if let Some(callbacks) = callbacks {
                (callbacks.on_reasoning)(chunk);
            }
        }
        if let Some(chunk) = delta
            .get("content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            reply.content.push_str(chunk);
            if let Some(callbacks) = callbacks {
                (callbacks.on_content)(chunk);
            }
        }
    }
    reply.latency_ms = started.elapsed().as_millis() as u64;
    Ok(reply)
}

pub async fn chat(
    client: &reqwest::Client,
    settings: &ProviderSettings,
    messages: &[Value],
) -> Result<AiReply, ClientError> {
    let started = std::time::Instant::now();
    let body = build_request_body(settings, messages);
    let url = format!(
        "{}/chat/completions",
        settings.base_url.trim_end_matches('/')
    );
    let response = client
        .post(&url)
        .bearer_auth(&settings.api_key)
        .timeout(Duration::from_secs(600))
        .json(&body)
        .send()
        .await
        .map_err(|error| ClientError::Network(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let detail: String = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(512)
            .collect();
        return Err(ClientError::Status(status.as_u16(), detail));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|error| ClientError::Network(error.to_string()))?;
    let mut reply = AiReply {
        content: value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        reasoning_content: value
            .pointer("/choices/0/message/reasoning_content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        ..Default::default()
    };
    if let Some(usage) = value.get("usage") {
        reply.usage = parse_usage(usage);
    }
    reply.latency_ms = started.elapsed().as_millis() as u64;
    Ok(reply)
}

/// Token 估算（无 tiktoken 时的字符近似：中文 ~1 token/字，ASCII ~1/4 token/字符）。
pub fn estimate_tokens(messages: &[Value]) -> u64 {
    let mut total = 4u64 * messages.len() as u64 + 2;
    for message in messages {
        if let Some(map) = message.as_object() {
            for value in map.values() {
                if let Some(text) = value.as_str() {
                    total += estimate_text_tokens(text);
                }
            }
        }
    }
    total
}

fn estimate_text_tokens(text: &str) -> u64 {
    let mut cjk = 0u64;
    let mut other = 0u64;
    for ch in text.chars() {
        if (ch as u32) >= 0x4E00 && (ch as u32) <= 0x9FFF {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + other / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(base: &str, model: &str, thinking: bool, effort: &str) -> ProviderSettings {
        ProviderSettings {
            model: model.into(),
            base_url: base.into(),
            api_key: String::new(),
            thinking,
            reasoning_effort: effort.into(),
            context_window: 200_000,
        }
    }

    #[test]
    fn deepseek_uses_adaptive_thinking() {
        let (extra, effort) = resolve_thinking_params(&settings(
            "https://api.deepseek.com",
            "deepseek-v4-pro",
            true,
            "max",
        ));
        assert_eq!(extra["thinking"]["type"], "adaptive");
        assert_eq!(extra["output_config"]["effort"], "max");
        assert!(effort.is_none());
    }

    #[test]
    fn disabled_thinking_injects_temperature_zero() {
        let settings = settings("https://api.deepseek.com", "deepseek-v4-pro", false, "max");
        let body = build_request_body(&settings, &[]);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body["temperature"], 0);
    }

    #[test]
    fn generic_openai_passes_reasoning_effort() {
        let settings = settings(
            "https://opencode.ai/zen/v1",
            "nemotron-3-ultra-free",
            true,
            "high",
        );
        let body = build_request_body(&settings, &[]);
        assert_eq!(body["reasoning_effort"], "high");
        assert!(body.get("thinking").is_none() || body["thinking"].as_object().unwrap().is_empty());
    }

    #[test]
    fn token_estimate_counts_cjk_heavier() {
        let messages = vec![json!({"role": "user", "content": "十个中文字符"})];
        let tokens = estimate_tokens(&messages);
        assert!(tokens >= 12);
    }
}
