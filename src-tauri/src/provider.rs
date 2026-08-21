use crate::{
    error::{KfResult, LocalizedError},
    runtime::RuntimeEventSink,
    state::{ActiveTurn, AppState},
    types::{ConfiguredModel, ProviderModel, ProviderProfile, ProviderTemplate, RuntimeEvent},
};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::Duration,
};

/// Headless compatibility defaults. The desktop application no longer installs
/// these as built-in user models.
pub const PROVIDER_ID: &str = "opencode";
pub const MODEL_ID: &str = "nemotron-3-ultra-free";
pub const AUXILIARY_MODEL_ID: &str = "nemotron-3.5-lightning-free";
pub const DEFAULT_BASE_URL: &str = "https://opencode.ai/zen/v1";
const MAX_TOOL_ARGUMENT_BYTES: usize = 64 * 1024;
const MAX_TOOL_CALLS: usize = 16;
const REQUIREMENT_REDUCER_MAX_TOKENS: u64 = 256;
const REQUIREMENT_REDUCER_TIMEOUT: Duration = Duration::from_secs(25);

fn endpoint_url(endpoint: &str, path: &str) -> String {
    format!("{}/{}", endpoint.trim_end_matches('/'), path)
}

pub const PROVIDER_TEMPLATES: &[ProviderTemplate] = &[
    ProviderTemplate {
        id: "openai",
        name: "OpenAI",
        adapter: "openai-responses",
        base_url: "https://api.openai.com/v1",
        api_key_env: "OPENAI_API_KEY",
    },
    ProviderTemplate {
        id: "anthropic",
        name: "Anthropic",
        adapter: "anthropic",
        base_url: "https://api.anthropic.com/v1",
        api_key_env: "ANTHROPIC_API_KEY",
    },
    ProviderTemplate {
        id: "gemini",
        name: "Google Gemini",
        adapter: "gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta",
        api_key_env: "GEMINI_API_KEY",
    },
    ProviderTemplate {
        id: "deepseek",
        name: "DeepSeek",
        adapter: "openai",
        base_url: "https://api.deepseek.com/v1",
        api_key_env: "DEEPSEEK_API_KEY",
    },
    ProviderTemplate {
        id: "openrouter",
        name: "OpenRouter",
        adapter: "openai",
        base_url: "https://openrouter.ai/api/v1",
        api_key_env: "OPENROUTER_API_KEY",
    },
    ProviderTemplate {
        id: "xai",
        name: "xAI",
        adapter: "openai",
        base_url: "https://api.x.ai/v1",
        api_key_env: "XAI_API_KEY",
    },
    ProviderTemplate {
        id: "mistral",
        name: "Mistral AI",
        adapter: "openai",
        base_url: "https://api.mistral.ai/v1",
        api_key_env: "MISTRAL_API_KEY",
    },
    ProviderTemplate {
        id: "groq",
        name: "Groq",
        adapter: "openai",
        base_url: "https://api.groq.com/openai/v1",
        api_key_env: "GROQ_API_KEY",
    },
    ProviderTemplate {
        id: "siliconflow",
        name: "SiliconFlow",
        adapter: "openai",
        base_url: "https://api.siliconflow.cn/v1",
        api_key_env: "SILICONFLOW_API_KEY",
    },
    ProviderTemplate {
        id: "moonshot",
        name: "Moonshot / Kimi",
        adapter: "openai",
        base_url: "https://api.moonshot.cn/v1",
        api_key_env: "MOONSHOT_API_KEY",
    },
    ProviderTemplate {
        id: "ollama",
        name: "Ollama",
        adapter: "openai",
        base_url: "http://127.0.0.1:11434/v1",
        api_key_env: "",
    },
    ProviderTemplate {
        id: "lmstudio",
        name: "LM Studio",
        adapter: "openai",
        base_url: "http://127.0.0.1:1234/v1",
        api_key_env: "",
    },
    ProviderTemplate {
        id: "custom",
        name: "Custom",
        adapter: "openai",
        base_url: "",
        api_key_env: "",
    },
];

fn humanize_model_id(id: &str) -> String {
    let name = id.strip_suffix("-free").unwrap_or(id);
    let humanized = name
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if humanized.is_empty() {
        id.into()
    } else {
        humanized
    }
}

fn normalized_thinking_effort(effort: &str) -> &str {
    match effort {
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" => effort,
        _ => "medium",
    }
}

fn inferred_model_adapter(profile: &ProviderProfile, model_id: &str) -> Option<String> {
    let base = profile.base_url.to_ascii_lowercase();
    // This gateway intentionally exposes response-native models and
    // OpenAI-compatible chat models under one /models catalog.
    if base.contains("opencode.ai/zen") {
        let id = model_id.to_ascii_lowercase();
        if id.starts_with("gpt-") || id.contains("codex") || id.starts_with("muse-spark") {
            return Some("openai-responses".into());
        }
        if id.starts_with("claude-") || id.starts_with("minimax-") || id.starts_with("qwen3.6-") {
            return Some("anthropic".into());
        }
        if id.starts_with("gemini-") {
            return Some("gemini".into());
        }
        return Some("openai".into());
    }
    None
}

pub fn model_adapter(profile: &ProviderProfile, model_id: &str) -> String {
    profile
        .models
        .iter()
        .find(|model| model.id == model_id)
        .and_then(|model| model.adapter.as_deref())
        .filter(|adapter| !adapter.is_empty())
        .map(str::to_owned)
        .or_else(|| inferred_model_adapter(profile, model_id))
        .unwrap_or_else(|| profile.adapter.clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThinkingOptions<'a> {
    pub enabled: bool,
    pub effort: &'a str,
}

impl<'a> ThinkingOptions<'a> {
    fn effort(self) -> &'a str {
        normalized_thinking_effort(self.effort)
    }

    fn active(self) -> bool {
        self.enabled && self.effort() != "none"
    }
}

fn provider_model(
    profile: &ProviderProfile,
    model: &ConfiguredModel,
    available: bool,
) -> ProviderModel {
    ProviderModel {
        provider_id: profile.id.clone(),
        provider_name: profile.name.clone(),
        model_id: model.id.clone(),
        model_name: if model.name.trim().is_empty() {
            humanize_model_id(&model.id)
        } else {
            model.name.clone()
        },
        available,
        capabilities: model.capabilities.clone(),
        context_limit: model.context_limit,
        thinking_enabled: model.thinking_enabled,
        thinking_effort: normalized_thinking_effort(&model.thinking_effort).to_owned(),
        thinking_toggle: model.thinking_toggle,
        thinking_efforts: model
            .thinking_efforts
            .iter()
            .filter(|effort| normalized_thinking_effort(effort) == effort.as_str())
            .cloned()
            .collect(),
        adapter: model_adapter(profile, &model.id),
    }
}

pub fn configured_catalog(profiles: &[ProviderProfile]) -> Vec<ProviderModel> {
    profiles
        .iter()
        .flat_map(|profile| {
            profile
                .models
                .iter()
                .map(|model| provider_model(profile, model, true))
        })
        .collect()
}

fn catalog_from_ids(ids: impl IntoIterator<Item = String>) -> Vec<ProviderModel> {
    let profile = ProviderProfile {
        id: PROVIDER_ID.into(),
        name: "Compatibility endpoint".into(),
        adapter: "openai".into(),
        base_url: DEFAULT_BASE_URL.into(),
        user_agent: String::new(),
        api_key: String::new(),
        credential_ref: String::new(),
        models: Vec::new(),
    };
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| id.ends_with("-free") && seen.insert(id.clone()))
        .map(|id| {
            let model = ConfiguredModel {
                name: humanize_model_id(&id),
                id,
                adapter: Some("openai".into()),
                capabilities: vec!["streaming".into(), "reasoning".into(), "toolCalls".into()],
                context_limit: None,
                thinking_enabled: false,
                thinking_effort: "medium".into(),
                thinking_toggle: false,
                thinking_efforts: Vec::new(),
                catalog_synced: false,
            };
            provider_model(&profile, &model, true)
        })
        .collect()
}

pub fn fallback_models() -> Vec<ProviderModel> {
    Vec::new()
}

pub fn install_catalog(state: &AppState, models: &[ProviderModel]) {
    *state.available_models.write() = models
        .iter()
        .filter(|model| model.available)
        .map(|model| model_key(&model.provider_id, &model.model_id))
        .collect();
}

pub fn clear_catalog(state: &AppState) {
    state.available_models.write().clear();
}

pub(crate) fn validate_available_model(
    provider_id: &str,
    model_id: &str,
    available_models: &HashSet<String>,
) -> KfResult<()> {
    let prefix = format!("{provider_id}\u{0}");
    if !available_models.iter().any(|key| key.starts_with(&prefix)) {
        return Err(LocalizedError::new("error.provider_unsupported").arg("provider", provider_id));
    }
    if !available_models.contains(&model_key(provider_id, model_id)) {
        return Err(LocalizedError::new("error.model_unsupported").arg("model", model_id));
    }
    Ok(())
}

fn model_key(provider_id: &str, model_id: &str) -> String {
    format!("{provider_id}\u{0}{model_id}")
}
pub fn available_model_keys(profiles: &[ProviderProfile]) -> HashSet<String> {
    profiles
        .iter()
        .flat_map(|profile| {
            profile
                .models
                .iter()
                .map(|model| model_key(&profile.id, &model.id))
        })
        .collect()
}

pub fn validate_profiles(profiles: &[ProviderProfile]) -> KfResult<()> {
    let mut ids = HashSet::new();
    for profile in profiles {
        if profile.id.trim().is_empty()
            || profile.name.trim().is_empty()
            || !ids.insert(profile.id.as_str())
        {
            return Err(LocalizedError::new("error.provider_profile"));
        }
        if !matches!(
            profile.adapter.as_str(),
            "openai" | "openai-responses" | "anthropic" | "gemini"
        ) {
            return Err(
                LocalizedError::new("error.provider_adapter").arg("adapter", &profile.adapter)
            );
        }
        let url = url::Url::parse(&profile.base_url)
            .map_err(|_| LocalizedError::new("error.provider_url"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(LocalizedError::new("error.provider_url"));
        }
        if profile.user_agent.len() > 256
            || (!profile.user_agent.trim().is_empty()
                && reqwest::header::HeaderValue::from_str(profile.user_agent.trim()).is_err())
        {
            return Err(LocalizedError::new("error.provider_user_agent"));
        }
        let mut models = HashSet::new();
        for model in &profile.models {
            if model.id.trim().is_empty() || !models.insert(model.id.as_str()) {
                return Err(LocalizedError::new("error.provider_model"));
            }
            if !matches!(
                model.thinking_effort.as_str(),
                "" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            ) {
                return Err(LocalizedError::new("error.provider_model"));
            }
            if model.thinking_efforts.iter().any(|effort| {
                !matches!(
                    effort.as_str(),
                    "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                )
            }) {
                return Err(LocalizedError::new("error.provider_model"));
            }
            if model.adapter.as_deref().is_some_and(|adapter| {
                !matches!(
                    adapter,
                    "" | "openai" | "openai-responses" | "anthropic" | "gemini"
                )
            }) {
                return Err(LocalizedError::new("error.provider_adapter"));
            }
        }
    }
    Ok(())
}

pub fn resolve_profile<'a>(
    profiles: &'a [ProviderProfile],
    id: &str,
) -> KfResult<&'a ProviderProfile> {
    profiles
        .iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| LocalizedError::new("error.provider_unsupported").arg("provider", id))
}

pub fn resolved_api_key(profile: &ProviderProfile) -> KfResult<String> {
    if !profile.api_key.is_empty() {
        return Ok(profile.api_key.clone());
    }
    if profile.credential_ref.is_empty() {
        return Ok(String::new());
    }
    crate::credentials::read(&profile.credential_ref)
}

fn effective_api_key<'a>(endpoint: &str, configured: &'a str) -> &'a str {
    if !configured.is_empty() {
        return configured;
    }
    // OpenCode's anonymous/free mode is still authenticated with the literal
    // public key. Omitting the header is a different request class.
    if endpoint.to_ascii_lowercase().contains("opencode.ai/zen") {
        return "public";
    }
    ""
}

const DEFAULT_USER_AGENT: &str = concat!("KnightFrame/", env!("CARGO_PKG_VERSION"));

fn effective_user_agent(configured: &str) -> &str {
    let configured = configured.trim();
    if configured.is_empty() {
        DEFAULT_USER_AGENT
    } else {
        configured
    }
}

fn apply_user_agent(request: reqwest::RequestBuilder, configured: &str) -> reqwest::RequestBuilder {
    request.header(
        reqwest::header::USER_AGENT,
        effective_user_agent(configured),
    )
}

pub fn validate_selection(state: &AppState, provider_id: &str, model_id: &str) -> KfResult<()> {
    validate_available_model(provider_id, model_id, &state.available_models.read())
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}
#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

fn inferred_capabilities(adapter: &str, model: &str) -> Vec<String> {
    let id = model.to_ascii_lowercase();
    let mut capabilities = vec!["streaming".into(), "toolCalls".into()];
    let vision = adapter == "gemini"
        || (adapter == "anthropic" && id.contains("claude"))
        || [
            "gpt-4o",
            "gpt-4.1",
            "gpt-5",
            "o1",
            "o3",
            "o4",
            "vision",
            "pixtral",
            "qwen-vl",
            "qwen2.5-vl",
            "gemma-3",
            "grok-4",
        ]
        .iter()
        .any(|needle| id.contains(needle));
    if vision {
        capabilities.push("imageInput".into());
    }
    capabilities
}

#[derive(Debug, Deserialize)]
struct GenericModelsResponse {
    #[serde(default)]
    data: Vec<Value>,
    #[serde(default)]
    models: Vec<Value>,
}

const MODELS_DEV_API: &str = "https://models.dev/api.json";

async fn fetch_models_dev(client: &reqwest::Client) -> Option<Value> {
    client
        .get(MODELS_DEV_API)
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<Value>()
        .await
        .ok()
}

fn normalized_api_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn models_dev_provider<'a>(catalog: &'a Value, profile: &ProviderProfile) -> Option<&'a Value> {
    let root = catalog.as_object()?;
    let base = normalized_api_url(&profile.base_url);
    if base.contains("opencode.ai/zen") {
        return root.get("opencode");
    }
    root.values().find(|provider| {
        provider
            .get("api")
            .and_then(Value::as_str)
            .is_some_and(|api| normalized_api_url(api) == base)
    })
}

fn adapter_from_models_dev(provider: &Value, model: &Value) -> Option<String> {
    let npm = model
        .pointer("/provider/npm")
        .and_then(Value::as_str)
        .or_else(|| provider.get("npm").and_then(Value::as_str))?;
    Some(
        match npm {
            "@ai-sdk/openai" | "@ai-sdk/azure" => "openai-responses",
            "@ai-sdk/anthropic" | "@ai-sdk/google-vertex/anthropic" => "anthropic",
            "@ai-sdk/google" | "@ai-sdk/google-vertex" => "gemini",
            _ => "openai",
        }
        .into(),
    )
}

fn models_dev_reasoning_options(model: &Value) -> (bool, Vec<String>) {
    let Some(options) = model.get("reasoning_options").and_then(Value::as_array) else {
        return (false, Vec::new());
    };
    let mut toggle = false;
    let mut efforts = Vec::new();
    for option in options {
        match option.get("type").and_then(Value::as_str) {
            Some("toggle") => toggle = true,
            Some("effort") => {
                if let Some(values) = option.get("values").and_then(Value::as_array) {
                    for value in values {
                        let effort = value
                            .as_str()
                            .map(str::to_owned)
                            .or_else(|| value.is_null().then(|| "none".into()));
                        if effort.as_deref() == Some("none") {
                            toggle = true;
                            continue;
                        }
                        if let Some(effort) = effort.filter(|effort| {
                            matches!(
                                effort.as_str(),
                                "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                            )
                        }) && !efforts.contains(&effort)
                        {
                            efforts.push(effort);
                        }
                    }
                }
            }
            Some("budget_tokens") => {
                for effort in ["high", "max"] {
                    if !efforts.iter().any(|candidate| candidate == effort) {
                        efforts.push(effort.into());
                    }
                }
            }
            _ => {}
        }
    }
    (toggle, efforts)
}

fn enrich_from_models_dev(
    profile: &ProviderProfile,
    provider: &Value,
    mut discovered: ConfiguredModel,
) -> ConfiguredModel {
    let Some(metadata) = provider
        .get("models")
        .and_then(Value::as_object)
        .and_then(|models| models.get(&discovered.id))
    else {
        return discovered;
    };
    if let Some(name) = metadata.get("name").and_then(Value::as_str) {
        discovered.name = name.to_owned();
    }
    discovered.adapter = adapter_from_models_dev(provider, metadata)
        .or_else(|| inferred_model_adapter(profile, &discovered.id));
    discovered.context_limit = metadata
        .pointer("/limit/context")
        .and_then(Value::as_u64)
        .or(discovered.context_limit);
    let reasoning = metadata
        .get("reasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let image_input = metadata
        .get("attachment")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || metadata
            .pointer("/modalities/input")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some("image")));
    let tool_calls = metadata
        .get("tool_call")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    discovered.capabilities = vec!["streaming".into()];
    if tool_calls {
        discovered.capabilities.push("toolCalls".into());
    }
    if reasoning {
        discovered.capabilities.push("reasoning".into());
    }
    if image_input {
        discovered.capabilities.push("imageInput".into());
    }
    (discovered.thinking_toggle, discovered.thinking_efforts) =
        models_dev_reasoning_options(metadata);
    discovered.catalog_synced = true;
    if !discovered
        .thinking_efforts
        .iter()
        .any(|effort| effort == &discovered.thinking_effort)
    {
        discovered.thinking_effort = discovered
            .thinking_efforts
            .iter()
            .find(|effort| effort.as_str() == "medium")
            .or_else(|| discovered.thinking_efforts.first())
            .cloned()
            .unwrap_or_else(|| "medium".into());
    }
    discovered
}

/// Resolve old/manual model entries once, then persist the compact result.
/// Startup never depends on this network catalog and already-synced entries do
/// not download it again.
pub async fn sync_catalog_metadata(
    client: &reqwest::Client,
    profiles: &[ProviderProfile],
) -> Option<Vec<ProviderProfile>> {
    if !profiles
        .iter()
        .flat_map(|profile| &profile.models)
        .any(|model| !model.catalog_synced)
    {
        return None;
    }
    let catalog = fetch_models_dev(client).await?;
    let mut next = profiles.to_vec();
    let mut changed = false;
    for profile in &mut next {
        let profile_snapshot = profile.clone();
        let Some(metadata_provider) = models_dev_provider(&catalog, &profile_snapshot) else {
            continue;
        };
        for model in &mut profile.models {
            if model.catalog_synced {
                continue;
            }
            let original = model.clone();
            let mut enriched =
                enrich_from_models_dev(&profile_snapshot, metadata_provider, original.clone());
            if !enriched.catalog_synced {
                continue;
            }
            if !original.name.trim().is_empty()
                && original.name != original.id
                && original.name != humanize_model_id(&original.id)
            {
                enriched.name = original.name;
            }
            if original.adapter.is_some() {
                enriched.adapter = original.adapter;
            }
            enriched.thinking_enabled = original.thinking_enabled
                && enriched
                    .thinking_efforts
                    .iter()
                    .any(|effort| effort == &original.thinking_effort);
            if enriched.thinking_enabled {
                enriched.thinking_effort = original.thinking_effort;
            }
            *model = enriched;
            changed = true;
        }
    }
    changed.then_some(next)
}

#[tauri::command]
pub async fn kf_provider_probe(
    client_state: tauri::State<'_, Arc<AppState>>,
    profile: ProviderProfile,
) -> KfResult<Vec<ConfiguredModel>> {
    validate_profiles(std::slice::from_ref(&profile))?;
    let configured_api_key = resolved_api_key(&profile)?;
    let api_key = effective_api_key(&profile.base_url, &configured_api_key);
    let mut request = client_state
        .client
        .get(endpoint_url(&profile.base_url, "models"));
    request = apply_user_agent(request, &profile.user_agent);
    request = match profile.adapter.as_str() {
        "anthropic" => request
            .header("anthropic-version", "2023-06-01")
            .header("x-api-key", api_key),
        "gemini" => request.header("x-goog-api-key", api_key),
        _ if !api_key.is_empty() => request.bearer_auth(api_key),
        _ => request,
    };
    let response = request
        .send()
        .await
        .map_err(|e| LocalizedError::new("error.provider_probe").arg("detail", e))?;
    if !response.status().is_success() {
        return Err(
            LocalizedError::new("error.provider_probe_status").arg("status", response.status())
        );
    }
    let payload: GenericModelsResponse = response
        .json()
        .await
        .map_err(|e| LocalizedError::new("error.provider_models_decode").arg("detail", e))?;
    let items = if payload.models.is_empty() {
        payload.data
    } else {
        payload.models
    };
    // A provider's /models response usually omits protocol, multimodal and
    // reasoning-variant metadata. models.dev is the same public catalog used
    // by OpenCode; failure here never makes provider discovery fail.
    let models_dev = fetch_models_dev(&client_state.client).await;
    let metadata_provider = models_dev
        .as_ref()
        .and_then(|catalog| models_dev_provider(catalog, &profile));

    let mut models = items
        .into_iter()
        .filter_map(|item| {
            let raw = item
                .get("id")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)?;
            let id = raw.strip_prefix("models/").unwrap_or(raw).to_owned();
            if profile.adapter == "gemini" {
                let supported = item
                    .get("supportedGenerationMethods")
                    .and_then(Value::as_array)
                    .is_some_and(|methods| {
                        methods
                            .iter()
                            .any(|method| method.as_str() == Some("generateContent"))
                    });
                if !supported {
                    return None;
                }
            }
            Some(ConfiguredModel {
                name: item
                    .get("displayName")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| humanize_model_id(&id)),
                capabilities: inferred_capabilities(&profile.adapter, &id),
                adapter: inferred_model_adapter(&profile, &id),
                context_limit: item
                    .get("inputTokenLimit")
                    .or_else(|| item.get("context_window"))
                    .and_then(Value::as_u64),
                thinking_enabled: false,
                thinking_effort: "medium".into(),
                thinking_toggle: false,
                thinking_efforts: Vec::new(),
                catalog_synced: false,
                id,
            })
        })
        .map(|model| {
            metadata_provider
                .map(|provider| enrich_from_models_dev(&profile, provider, model.clone()))
                .unwrap_or(model)
        })
        .collect::<Vec<_>>();
    models.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}

pub async fn probe(client: &reqwest::Client) -> KfResult<Vec<ProviderModel>> {
    probe_at(client, DEFAULT_BASE_URL).await
}

pub async fn probe_at(client: &reqwest::Client, endpoint: &str) -> KfResult<Vec<ProviderModel>> {
    let response = client
        .get(endpoint_url(endpoint, "models"))
        .header(reqwest::header::USER_AGENT, DEFAULT_USER_AGENT)
        .bearer_auth("public")
        .send()
        .await
        .map_err(|e| LocalizedError::new("error.provider_probe").arg("detail", e))?;
    if !response.status().is_success() {
        return Err(
            LocalizedError::new("error.provider_probe_status").arg("status", response.status())
        );
    }
    let models: ModelsResponse = response
        .json()
        .await
        .map_err(|e| LocalizedError::new("error.provider_models_decode").arg("detail", e))?;
    Ok(catalog_from_ids(
        models.data.into_iter().map(|model| model.id),
    ))
}

pub async fn summarize_title(
    client: &reqwest::Client,
    model: &str,
    request: &str,
) -> KfResult<(String, TokenUsage)> {
    let request: String = request.trim().chars().take(500).collect();
    let response = client
        .post(endpoint_url(DEFAULT_BASE_URL, "chat/completions"))
        .header(reqwest::header::USER_AGENT, DEFAULT_USER_AGENT)
        .bearer_auth("public")
        .json(&json!({
            "model": model,
            "stream": false,
            "temperature": 0,
            "max_tokens": 32,
            "messages": [
                {"role":"system","content":"Write a 2-6 word conversation title. Match the user's language. Output the title only."},
                {"role":"user","content":request}
            ]
        }))
        .send()
        .await
        .map_err(|error| LocalizedError::new("error.provider_request").arg("detail", error))?;
    if !response.status().is_success() {
        return Err(LocalizedError::new("error.provider_status").arg("status", response.status()));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|error| LocalizedError::new("error.provider_sse_decode").arg("detail", error))?;
    let title = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .trim_matches(['\'', '"', '`', '#', '*', ' '])
        .lines()
        .next()
        .unwrap_or_default()
        .trim();
    let title: String = title.chars().take(64).collect();
    if title.is_empty() {
        return Err(LocalizedError::new("error.provider_response_empty"));
    }
    Ok((title, value.get("usage").map(usage).unwrap_or_default()))
}

async fn complete_text_profile(
    client: &reqwest::Client,
    profile: &ProviderProfile,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u64,
) -> KfResult<(String, TokenUsage)> {
    let api_key = resolved_api_key(profile)?;
    let (url, payload) = match profile.adapter.as_str() {
        "openai-responses" => (
            endpoint_url(&profile.base_url, "responses"),
            json!({"model":model,"instructions":system,"input":user,"max_output_tokens":max_tokens}),
        ),
        "anthropic" => (
            endpoint_url(&profile.base_url, "messages"),
            json!({"model":model,"system":system,"messages":[{"role":"user","content":user}],"max_tokens":max_tokens}),
        ),
        "gemini" => (
            endpoint_url(
                &profile.base_url,
                &format!("models/{model}:generateContent"),
            ),
            json!({"systemInstruction":{"parts":[{"text":system}]},"contents":[{"role":"user","parts":[{"text":user}]}],"generationConfig":{"maxOutputTokens":max_tokens,"temperature":0}}),
        ),
        _ => (
            endpoint_url(&profile.base_url, "chat/completions"),
            json!({"model":model,"stream":false,"temperature":0,"max_tokens":max_tokens,"messages":[{"role":"system","content":system},{"role":"user","content":user}]}),
        ),
    };
    let mut request = client.post(url).json(&payload);
    request = apply_user_agent(request, &profile.user_agent);
    request = match profile.adapter.as_str() {
        "anthropic" => request
            .header("anthropic-version", "2023-06-01")
            .header("x-api-key", &api_key),
        "gemini" => request.header("x-goog-api-key", &api_key),
        _ if !api_key.is_empty() => request.bearer_auth(&api_key),
        _ => request,
    };
    let response = request
        .send()
        .await
        .map_err(|error| LocalizedError::new("error.provider_request").arg("detail", error))?;
    if !response.status().is_success() {
        return Err(LocalizedError::new("error.provider_status").arg("status", response.status()));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|error| LocalizedError::new("error.provider_sse_decode").arg("detail", error))?;
    let text = match profile.adapter.as_str() {
        "openai-responses" => value
            .get("output")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|item| {
                item.get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .find_map(|part| part.get("text").and_then(Value::as_str)),
        "anthropic" => value
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find_map(|part| part.get("text").and_then(Value::as_str)),
        "gemini" => value
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(Value::as_str),
        _ => value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str),
    }
    .unwrap_or_default()
    .trim()
    .to_owned();
    let token_usage = if profile.adapter == "gemini" {
        value
            .get("usageMetadata")
            .map(|u| TokenUsage {
                input_tokens: u
                    .get("promptTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                cached_input_tokens: u
                    .get("cachedContentTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                output_tokens: u
                    .get("candidatesTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                reasoning_tokens: u
                    .get("thoughtsTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                reported: true,
            })
            .unwrap_or_default()
    } else {
        value.get("usage").map(usage).unwrap_or_default()
    };
    if text.is_empty() {
        return Err(LocalizedError::new("error.provider_response_empty"));
    }
    Ok((text, token_usage))
}

pub async fn summarize_title_profile(
    client: &reqwest::Client,
    profile: &ProviderProfile,
    model: &str,
    request: &str,
) -> KfResult<(String, TokenUsage)> {
    let request: String = request.trim().chars().take(500).collect();
    let (title, usage) = complete_text_profile(
        client,
        profile,
        model,
        "Write a 2-6 word conversation title. Match the user's language. Output the title only.",
        &request,
        32,
    )
    .await?;
    let title = title
        .trim_matches(['\'', '"', '`', '#', '*', ' '])
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(64)
        .collect::<String>();
    if title.is_empty() {
        return Err(LocalizedError::new("error.provider_response_empty"));
    }
    Ok((title, usage))
}

#[derive(Debug)]
pub struct RequirementReduction {
    pub brief: String,
    pub usage: TokenUsage,
}

fn requirement_reducer_payload(model: &str, request: &str) -> Value {
    json!({
        "model": model,
        "stream": false,
        "temperature": 0,
        "max_tokens": REQUIREMENT_REDUCER_MAX_TOKENS,
        "messages": [
            {
                "role": "system",
                "content": "Compress the user's request in its original language. Preserve every requested action, constraint, acceptance criterion, path, identifier, number, exception, and verification requirement. Remove only repetition, filler, and pleasantries. Output only the compact requirement brief."
            },
            {"role": "user", "content": request}
        ]
    })
}

pub async fn reduce_requirement(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    request: &str,
    cancellation: &tokio_util::sync::CancellationToken,
) -> KfResult<RequirementReduction> {
    if cancellation.is_cancelled() {
        return Err(LocalizedError::new("error.session_cancelled"));
    }
    let operation = async {
        let response = client
            .post(endpoint_url(endpoint, "chat/completions"))
            .bearer_auth("public")
            .json(&requirement_reducer_payload(model, request))
            .send()
            .await
            .map_err(|error| LocalizedError::new("error.provider_request").arg("detail", error))?;
        if !response.status().is_success() {
            let status = response.status();
            let detail: String = response
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(2_048)
                .collect();
            return Err(provider_http_error(status, &detail));
        }
        let value: Value = response.json().await.map_err(|error| {
            LocalizedError::new("error.provider_sse_decode").arg("detail", error)
        })?;
        let brief = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        Ok(RequirementReduction {
            brief,
            usage: value.get("usage").map(usage).unwrap_or_default(),
        })
    };

    tokio::select! {
        _ = cancellation.cancelled() => Err(LocalizedError::new("error.session_cancelled")),
        result = tokio::time::timeout(REQUIREMENT_REDUCER_TIMEOUT, operation) => {
            result.unwrap_or_else(|_| Err(
                LocalizedError::new("error.provider_idle_timeout")
                    .arg("seconds", REQUIREMENT_REDUCER_TIMEOUT.as_secs())
            ))
        }
    }
}

pub async fn reduce_requirement_profile(
    client: &reqwest::Client,
    profile: &ProviderProfile,
    model: &str,
    request: &str,
    cancellation: &tokio_util::sync::CancellationToken,
) -> KfResult<RequirementReduction> {
    if cancellation.is_cancelled() {
        return Err(LocalizedError::new("error.session_cancelled"));
    }
    let operation = complete_text_profile(
        client,
        profile,
        model,
        "Compress the user's request in its original language. Preserve every requested action, constraint, acceptance criterion, path, identifier, number, exception, and verification requirement. Remove only repetition, filler, and pleasantries. Output only the compact requirement brief.",
        request,
        REQUIREMENT_REDUCER_MAX_TOKENS,
    );
    tokio::select! {
        _ = cancellation.cancelled() => Err(LocalizedError::new("error.session_cancelled")),
        result = tokio::time::timeout(REQUIREMENT_REDUCER_TIMEOUT, operation) => result.map_err(|_| LocalizedError::new("error.provider_idle_timeout").arg("seconds", REQUIREMENT_REDUCER_TIMEOUT.as_secs())).and_then(|result| result.map(|(brief, usage)| RequirementReduction { brief, usage })),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    #[serde(skip)]
    pub reported: bool,
}

impl TokenUsage {
    pub fn add(&mut self, other: &Self) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
        self.reported |= other.reported;
    }
    pub fn fresh_input_tokens(&self) -> u64 {
        self.input_tokens.saturating_sub(self.cached_input_tokens)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Clone, Debug, Default)]
pub struct ToolAccumulator(BTreeMap<usize, PartialToolCall>);

impl ToolAccumulator {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn apply(&mut self, value: &Value) -> KfResult<()> {
        let index = value
            .get("index")
            .and_then(Value::as_u64)
            .ok_or_else(|| LocalizedError::new("error.provider_tool_index"))?
            as usize;
        if index >= MAX_TOOL_CALLS {
            return Err(
                LocalizedError::new("error.provider_tool_index_limit").arg("max", MAX_TOOL_CALLS)
            );
        }
        let call = self.0.entry(index).or_default();
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            if !call.id.is_empty() && call.id != id {
                return Err(LocalizedError::new("error.provider_tool_id_changed"));
            }
            call.id = id.into();
        }
        if let Some(function) = value.get("function") {
            if let Some(name) = function.get("name").and_then(Value::as_str) {
                if call.name.is_empty() {
                    call.name = name.into();
                } else if call.name != name {
                    return Err(LocalizedError::new("error.provider_tool_name_changed"));
                }
            }
            let arguments = function
                .get("arguments")
                .and_then(|arguments| match arguments {
                    Value::String(arguments) => Some(arguments.clone()),
                    Value::Object(_) | Value::Array(_) => Some(arguments.to_string()),
                    _ => None,
                });
            if let Some(arguments) = arguments {
                if call.arguments.len().saturating_add(arguments.len()) > MAX_TOOL_ARGUMENT_BYTES {
                    return Err(LocalizedError::new("error.provider_tool_arguments_limit")
                        .arg("maxBytes", MAX_TOOL_ARGUMENT_BYTES));
                }
                call.arguments.push_str(&arguments);
            }
        }
        Ok(())
    }

    fn finish(self) -> KfResult<Vec<ToolCall>> {
        self.0
            .into_iter()
            .map(|(index, call)| {
                if call.id.is_empty() || call.name.is_empty() {
                    return Err(
                        LocalizedError::new("error.provider_tool_incomplete").arg("index", index)
                    );
                }
                let arguments = serde_json::from_str(&call.arguments).map_err(|e| {
                    LocalizedError::new("error.provider_tool_arguments")
                        .arg("tool", &call.name)
                        .arg("detail", e)
                })?;
                Ok(ToolCall {
                    index,
                    id: call.id,
                    name: call.name,
                    arguments,
                })
            })
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct ParsedDelta {
    pub text: Option<String>,
    pub reasoning: Option<String>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
    pub done: bool,
    tool_deltas: Vec<Value>,
}

fn usage(value: &Value) -> TokenUsage {
    let prompt_total = value.get("prompt_tokens").and_then(Value::as_u64);
    let generic_input = value.get("input_tokens").and_then(Value::as_u64);
    let prompt_detail_cached = value
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_u64);
    let input_detail_cached = value
        .pointer("/input_tokens_details/cached_tokens")
        .and_then(Value::as_u64);
    let cache_hit = value.get("prompt_cache_hit_tokens").and_then(Value::as_u64);
    let cache_miss = value
        .get("prompt_cache_miss_tokens")
        .and_then(Value::as_u64);
    let cache_read = value.get("cache_read_input_tokens").and_then(Value::as_u64);
    let cached_reported = prompt_detail_cached
        .or(input_detail_cached)
        .or(cache_hit)
        .or(cache_read);
    let input = prompt_total
        .or_else(|| input_detail_cached.and(generic_input))
        .or_else(|| match (cache_hit, cache_miss) {
            (Some(hit), Some(miss)) => Some(hit.saturating_add(miss)),
            _ => None,
        })
        .or_else(|| generic_input.map(|fresh| fresh.saturating_add(cache_read.unwrap_or(0))))
        .unwrap_or(0);
    let cached = cached_reported.unwrap_or(0).min(input);
    TokenUsage {
        input_tokens: input,
        cached_input_tokens: cached,
        output_tokens: value
            .get("completion_tokens")
            .or_else(|| value.get("output_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        reasoning_tokens: value
            .pointer("/completion_tokens_details/reasoning_tokens")
            .or_else(|| value.pointer("/output_tokens_details/reasoning_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        reported: true,
    }
}

fn stream_request_payload(
    model: &str,
    messages: &[Value],
    tools: &[Value],
    thinking: ThinkingOptions<'_>,
) -> Value {
    let mut payload = json!({
        "model": model,
        "stream": true,
        "stream_options": {"include_usage": true},
        "messages": messages,
        "tools": tools,
        "tool_choice": "auto"
    });
    if thinking.active() {
        payload["reasoning_effort"] = Value::String(thinking.effort().to_owned());
    }
    payload
}

fn openai_responses_payload(
    model: &str,
    messages: &[Value],
    tools: &[Value],
    thinking: ThinkingOptions<'_>,
) -> Value {
    let instructions = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        .filter_map(|message| message.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let mut input = Vec::new();
    for message in messages {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        if role == "system" {
            continue;
        }
        if role == "tool" {
            input.push(json!({"type":"function_call_output","call_id":message.get("tool_call_id").and_then(Value::as_str).unwrap_or("tool"),"output":message.get("content").and_then(Value::as_str).unwrap_or("")}));
            continue;
        }
        if role == "assistant"
            && let Some(calls) = message.get("tool_calls").and_then(Value::as_array)
        {
            input.extend(calls.iter().map(|call| json!({"type":"function_call","call_id":call.get("id").and_then(Value::as_str).unwrap_or("tool"),"name":call.pointer("/function/name").and_then(Value::as_str).unwrap_or("tool"),"arguments":call.pointer("/function/arguments").and_then(Value::as_str).unwrap_or("{}") })));
            continue;
        }
        let raw = message.get("content").cloned().unwrap_or_else(|| json!(""));
        let content = if let Some(parts) = raw.as_array() {
            Value::Array(parts.iter().filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("text") => Some(json!({"type":if role == "assistant" {"output_text"} else {"input_text"},"text":part.get("text").and_then(Value::as_str).unwrap_or("")})),
                Some("image_url") if role != "assistant" => Some(json!({"type":"input_image","image_url":part.pointer("/image_url/url").and_then(Value::as_str).unwrap_or("")})),
                _ => None,
            }).collect())
        } else {
            Value::Array(vec![
                json!({"type":if role == "assistant" {"output_text"} else {"input_text"},"text":raw.as_str().unwrap_or("")}),
            ])
        };
        input.push(json!({"type":"message","role":role,"content":content}));
    }
    let tools = tools.iter().filter_map(|tool| tool.get("function")).map(|function| json!({"type":"function","name":function.get("name").cloned().unwrap_or(Value::Null),"description":function.get("description").cloned().unwrap_or(Value::Null),"parameters":function.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object"}))})).collect::<Vec<_>>();
    let mut payload = json!({"model":model,"stream":true,"instructions":instructions,"input":input,"tools":tools,"tool_choice":"auto","parallel_tool_calls":true});
    if thinking.active() {
        payload["reasoning"] = json!({"effort":thinking.effort(),"summary":"auto"});
    }
    payload
}

fn anthropic_payload(
    model: &str,
    messages: &[Value],
    tools: &[Value],
    thinking: ThinkingOptions<'_>,
) -> Value {
    let system = messages
        .iter()
        .filter(|m| m.get("role").and_then(Value::as_str) == Some("system"))
        .filter_map(|m| m.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let messages = messages.iter().filter_map(|message| {
        let role = message.get("role")?.as_str()?;
        if role == "system" { return None; }
        if role == "tool" {
            return Some(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":message.get("tool_call_id").and_then(Value::as_str).unwrap_or("tool"),"content":message.get("content").and_then(Value::as_str).unwrap_or("")}]}));
        }
        if role == "assistant" && message.get("tool_calls").is_some() {
            let blocks = message.get("tool_calls").and_then(Value::as_array).into_iter().flatten().map(|call| json!({
                "type":"tool_use",
                "id":call.get("id").and_then(Value::as_str).unwrap_or("tool"),
                "name":call.pointer("/function/name").and_then(Value::as_str).unwrap_or("tool"),
                "input":call.pointer("/function/arguments").and_then(Value::as_str).and_then(|v| serde_json::from_str::<Value>(v).ok()).unwrap_or_else(|| json!({}))
            })).collect::<Vec<_>>();
            return Some(json!({"role":"assistant","content":blocks}));
        }
        let content = message.get("content").cloned().unwrap_or_else(|| json!(""));
        let content = if let Some(parts) = content.as_array() {
            Value::Array(parts.iter().filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("text") => Some(json!({"type":"text","text":part.get("text").and_then(Value::as_str).unwrap_or("")})),
                Some("image_url") => {
                    let url = part.pointer("/image_url/url").and_then(Value::as_str)?;
                    let (meta, data) = url.split_once(',')?;
                    Some(json!({"type":"image","source":{"type":"base64","media_type":meta.trim_start_matches("data:").trim_end_matches(";base64"),"data":data}}))
                }
                _ => None,
            }).collect())
        } else { content };
        Some(json!({"role":role,"content":content}))
    }).collect::<Vec<_>>();
    let tools = tools.iter().filter_map(|tool| tool.get("function")).map(|function| json!({
        "name":function.get("name").cloned().unwrap_or(Value::Null),
        "description":function.get("description").cloned().unwrap_or(Value::Null),
        "input_schema":function.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object"}))
    })).collect::<Vec<_>>();
    let mut payload = json!({"model":model,"stream":true,"max_tokens":16384,"system":system,"messages":messages,"tools":tools});
    if thinking.active() {
        let id = model.to_ascii_lowercase();
        let adaptive = id.contains("fable-5")
            || id.contains("sonnet-5")
            || [
                "opus-4-6",
                "opus-4.6",
                "opus-4-7",
                "opus-4.7",
                "opus-4-8",
                "opus-4.8",
                "sonnet-4-6",
                "sonnet-4.6",
            ]
            .iter()
            .any(|version| id.contains(version));
        let summarized = id.contains("fable-5")
            || id.contains("sonnet-5")
            || ["opus-4-7", "opus-4.7", "opus-4-8", "opus-4.8"]
                .iter()
                .any(|version| id.contains(version));
        if adaptive {
            payload["thinking"] = if summarized {
                json!({"type":"adaptive","display":"summarized"})
            } else {
                json!({"type":"adaptive"})
            };
            payload["output_config"] = json!({"effort":thinking.effort()});
        } else if id.contains("opus-4-5") || id.contains("opus-4.5") {
            payload["output_config"] = json!({"effort":thinking.effort()});
        } else {
            let budget = match thinking.effort() {
                "minimal" => 1024,
                "low" => 2048,
                "high" => 8192,
                "xhigh" => 12288,
                "max" => 16384,
                _ => 4096,
            };
            payload["thinking"] = json!({"type":"enabled","budget_tokens":budget});
        }
    }
    payload
}

fn gemini_payload(
    model: &str,
    messages: &[Value],
    tools: &[Value],
    thinking: ThinkingOptions<'_>,
) -> Value {
    let system = messages
        .iter()
        .filter(|m| m.get("role").and_then(Value::as_str) == Some("system"))
        .filter_map(|m| m.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let tool_names = messages
        .iter()
        .filter_map(|message| message.get("tool_calls").and_then(Value::as_array))
        .flatten()
        .filter_map(|call| {
            Some((
                call.get("id")?.as_str()?.to_owned(),
                call.pointer("/function/name")?.as_str()?.to_owned(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let contents = messages.iter().filter_map(|message| {
        let role = message.get("role")?.as_str()?;
        if role == "system" { return None; }
        if role == "tool" {
            let response = message.get("content").and_then(Value::as_str).and_then(|v| serde_json::from_str::<Value>(v).ok()).unwrap_or_else(|| json!({"result":message.get("content").cloned().unwrap_or(Value::Null)}));
            let name = message.get("tool_call_id").and_then(Value::as_str).and_then(|id| tool_names.get(id)).map(String::as_str).unwrap_or("tool");
            return Some(json!({"role":"user","parts":[{"functionResponse":{"name":name,"response":response}}]}));
        }
        if role == "assistant" && message.get("tool_calls").is_some() {
            let parts = message.get("tool_calls").and_then(Value::as_array).into_iter().flatten().map(|call| json!({"functionCall":{
                "name":call.pointer("/function/name").and_then(Value::as_str).unwrap_or("tool"),
                "args":call.pointer("/function/arguments").and_then(Value::as_str).and_then(|v| serde_json::from_str::<Value>(v).ok()).unwrap_or_else(|| json!({}))
            }})).collect::<Vec<_>>();
            return Some(json!({"role":"model","parts":parts}));
        }
        let raw = message.get("content").cloned().unwrap_or_else(|| json!(""));
        let parts = if let Some(items) = raw.as_array() {
            items.iter().filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("text") => Some(json!({"text":part.get("text").and_then(Value::as_str).unwrap_or("")})),
                Some("image_url") => {
                    let url = part.pointer("/image_url/url").and_then(Value::as_str)?;
                    let (meta, data) = url.split_once(',')?;
                    Some(json!({"inlineData":{"mimeType":meta.trim_start_matches("data:").trim_end_matches(";base64"),"data":data}}))
                }
                _ => None,
            }).collect::<Vec<_>>()
        } else { vec![json!({"text":raw.as_str().unwrap_or("")})] };
        Some(json!({"role":if role == "assistant" {"model"} else {"user"},"parts":parts}))
    }).collect::<Vec<_>>();
    let declarations = tools.iter().filter_map(|tool| tool.get("function")).map(|function| json!({
        "name":function.get("name").cloned().unwrap_or(Value::Null),
        "description":function.get("description").cloned().unwrap_or(Value::Null),
        "parameters":function.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object"}))
    })).collect::<Vec<_>>();
    let mut payload = json!({"systemInstruction":{"parts":[{"text":system}]},"contents":contents,"tools":[{"functionDeclarations":declarations}]});
    if thinking.active() {
        let config = if model.to_ascii_lowercase().contains("2.5") {
            let budget = match thinking.effort() {
                "minimal" => 512,
                "low" => 2048,
                "high" => 8192,
                "xhigh" => 12288,
                "max" => 16384,
                _ => 4096,
            };
            json!({"thinkingBudget":budget,"includeThoughts":true})
        } else {
            let level = match thinking.effort() {
                "xhigh" | "max" => "high",
                effort => effort,
            };
            json!({"thinkingLevel":level,"includeThoughts":true})
        };
        payload["generationConfig"] = json!({"thinkingConfig":config});
    }
    payload
}

fn adapter_payload(
    adapter: &str,
    model: &str,
    messages: &[Value],
    tools: &[Value],
    thinking: ThinkingOptions<'_>,
) -> Value {
    match adapter {
        "openai-responses" => openai_responses_payload(model, messages, tools, thinking),
        "anthropic" => anthropic_payload(model, messages, tools, thinking),
        "gemini" => gemini_payload(model, messages, tools, thinking),
        _ => stream_request_payload(model, messages, tools, thinking),
    }
}

fn adapter_endpoint(adapter: &str, endpoint: &str, model: &str) -> String {
    match adapter {
        "anthropic" => endpoint_url(endpoint, "messages"),
        "gemini" => endpoint_url(
            endpoint,
            &format!("models/{model}:streamGenerateContent?alt=sse"),
        ),
        "openai-responses" => endpoint_url(endpoint, "responses"),
        _ => endpoint_url(endpoint, "chat/completions"),
    }
}

fn parse_native_sse(adapter: &str, data: &str) -> KfResult<ParsedDelta> {
    if adapter == "openai" {
        return parse_sse_data(data);
    }
    let value: Value = serde_json::from_str(data.trim())
        .map_err(|e| LocalizedError::new("error.provider_sse_decode").arg("detail", e))?;
    if adapter == "openai-responses" {
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        let mut parsed = ParsedDelta::default();
        match kind {
            "response.output_text.delta" => parsed.text = value.get("delta").and_then(Value::as_str).map(str::to_owned),
            "response.reasoning_summary_text.delta" => parsed.reasoning = value.get("delta").and_then(Value::as_str).map(str::to_owned),
            "response.output_item.added" if value.pointer("/item/type").and_then(Value::as_str) == Some("function_call") => parsed.tool_deltas.push(json!({"index":value.get("output_index").and_then(Value::as_u64).unwrap_or(0),"id":value.pointer("/item/call_id").or_else(|| value.pointer("/item/id")).and_then(Value::as_str).unwrap_or("tool"),"function":{"name":value.pointer("/item/name").and_then(Value::as_str).unwrap_or("tool"),"arguments":""}})),
            "response.function_call_arguments.delta" => parsed.tool_deltas.push(json!({"index":value.get("output_index").and_then(Value::as_u64).unwrap_or(0),"function":{"arguments":value.get("delta").and_then(Value::as_str).unwrap_or("")}})),
            "response.completed" => { parsed.done = true; parsed.finish_reason = Some("stop".into()); parsed.usage = value.pointer("/response/usage").map(usage); },
            "response.incomplete" | "response.failed" => parsed.finish_reason = Some("incomplete".into()),
            _ => {}
        }
        return Ok(parsed);
    }
    if adapter == "anthropic" {
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        let mut parsed = ParsedDelta::default();
        match kind {
            "content_block_start" if value.pointer("/content_block/type").and_then(Value::as_str) == Some("tool_use") => parsed.tool_deltas.push(json!({"index":value.get("index").and_then(Value::as_u64).unwrap_or(0),"id":value.pointer("/content_block/id").and_then(Value::as_str).unwrap_or("tool"),"function":{"name":value.pointer("/content_block/name").and_then(Value::as_str).unwrap_or("tool"),"arguments":""}})),
            "content_block_delta" => match value.pointer("/delta/type").and_then(Value::as_str) {
                Some("text_delta") => parsed.text = value.pointer("/delta/text").and_then(Value::as_str).map(str::to_owned),
                Some("thinking_delta") => parsed.reasoning = value.pointer("/delta/thinking").and_then(Value::as_str).map(str::to_owned),
                Some("input_json_delta") => parsed.tool_deltas.push(json!({"index":value.get("index").and_then(Value::as_u64).unwrap_or(0),"function":{"arguments":value.pointer("/delta/partial_json").and_then(Value::as_str).unwrap_or("")}})),
                _ => {}
            },
            "message_delta" => { parsed.finish_reason = value.pointer("/delta/stop_reason").and_then(Value::as_str).map(|v| if v == "end_turn" {"stop"} else if v == "tool_use" {"tool_calls"} else if v == "max_tokens" {"length"} else {v}.to_owned()); parsed.usage = value.get("usage").map(usage); }
            "message_start" => parsed.usage = value.pointer("/message/usage").map(usage),
            "message_stop" => parsed.done = true,
            _ => {}
        }
        return Ok(parsed);
    }
    let mut parsed = ParsedDelta::default();
    if let Some(parts) = value
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
    {
        for (index, part) in parts.iter().enumerate() {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                if part.get("thought").and_then(Value::as_bool) == Some(true) {
                    parsed
                        .reasoning
                        .get_or_insert_with(String::new)
                        .push_str(text);
                } else {
                    parsed.text.get_or_insert_with(String::new).push_str(text);
                }
            }
            if let Some(call) = part.get("functionCall") {
                parsed.tool_deltas.push(json!({"index":index,"id":format!("gemini-{index}"),"function":{"name":call.get("name").and_then(Value::as_str).unwrap_or("tool"),"arguments":call.get("args").cloned().unwrap_or_else(|| json!({})).to_string()}}));
            }
        }
    }
    parsed.usage = value.get("usageMetadata").map(|u| TokenUsage {
        input_tokens: u
            .get("promptTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cached_input_tokens: u
            .get("cachedContentTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: u
            .get("candidatesTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        reasoning_tokens: u
            .get("thoughtsTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        reported: true,
    });
    parsed.finish_reason = value
        .pointer("/candidates/0/finishReason")
        .and_then(Value::as_str)
        .map(|v| {
            if v == "STOP" {
                "stop"
            } else if v == "MAX_TOKENS" {
                "length"
            } else {
                v
            }
            .to_owned()
        });
    Ok(parsed)
}

pub fn parse_sse_data(data: &str) -> KfResult<ParsedDelta> {
    let data = data.trim();
    if data.is_empty() {
        return Ok(ParsedDelta::default());
    }
    if data == "[DONE]" {
        return Ok(ParsedDelta {
            done: true,
            ..Default::default()
        });
    }
    let value: Value = serde_json::from_str(data)
        .map_err(|e| LocalizedError::new("error.provider_sse_decode").arg("detail", e))?;
    if let Some(error) = value.get("error") {
        let detail = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_else(|| error.as_str().unwrap_or("provider stream error"));
        return Err(LocalizedError::new("error.provider_stream").arg("detail", detail));
    }
    // Compatible gateways use both streaming `delta` frames and terminal
    // `message` frames. Accept both without tying behavior to model names.
    let choice = value.pointer("/choices/0");
    let delta = choice
        .and_then(|item| item.get("delta"))
        .or_else(|| choice.and_then(|item| item.get("message")));
    let mut tool_deltas = delta
        .and_then(|item| item.get("tool_calls"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(call) = delta.and_then(|item| item.get("tool_call")) {
        tool_deltas.push(call.clone());
    }
    for (index, call) in tool_deltas.iter_mut().enumerate() {
        if let Some(object) = call.as_object_mut() {
            object.entry("index").or_insert_with(|| json!(index));
            if !object.contains_key("id")
                && object
                    .get("function")
                    .and_then(|function| function.get("name"))
                    .is_some()
            {
                object.insert("id".into(), json!(format!("tool-{index}")));
            }
        }
    }
    if let Some(function) = delta.and_then(|item| item.get("function_call")) {
        tool_deltas.push(json!({
            "index": 0,
            "id": choice.and_then(|item| item.get("id")).and_then(Value::as_str).unwrap_or("legacy-tool"),
            "function": function
        }));
    }
    let text = compatible_text(delta.and_then(|item| item.get("content")));
    let reasoning = ["reasoning_content", "reasoning", "thinking", "analysis"]
        .iter()
        .find_map(|key| compatible_text(delta.and_then(|item| item.get(*key))))
        .or_else(|| {
            delta
                .and_then(|item| item.get("reasoning_details"))
                .and_then(Value::as_array)
                .map(|details| {
                    details
                        .iter()
                        .filter_map(|detail| {
                            compatible_text(
                                detail
                                    .get("text")
                                    .or_else(|| detail.get("content"))
                                    .or_else(|| detail.get("summary")),
                            )
                        })
                        .collect::<String>()
                })
                .filter(|text| !text.is_empty())
        });
    Ok(ParsedDelta {
        text,
        reasoning,
        usage: value
            .get("usage")
            .filter(|value| !value.is_null())
            .map(usage),
        finish_reason: value
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
            .map(normalize_finish_reason),
        done: value.get("done").and_then(Value::as_bool) == Some(true),
        tool_deltas,
    })
}

fn compatible_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(parts) => {
            let joined = parts
                .iter()
                .filter_map(|part| {
                    part.as_str().or_else(|| {
                        part.get("text")
                            .or_else(|| part.get("content"))
                            .or_else(|| part.get("output_text"))
                            .and_then(Value::as_str)
                    })
                })
                .collect::<String>();
            (!joined.is_empty()).then_some(joined)
        }
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        _ => None,
    }
}

fn normalize_finish_reason(reason: &str) -> String {
    match reason {
        "end_turn" | "completed" | "complete" => "stop",
        "tool_use" | "function_call" => "tool_calls",
        "max_tokens" | "max_output_tokens" => "length",
        other => other,
    }
    .to_owned()
}

fn stream_completed(
    adapter: &str,
    got_done: bool,
    finish_reason: &Option<String>,
    has_payload: bool,
) -> bool {
    got_done
        || finish_reason.is_some()
        // A number of OpenAI-compatible gateways (including Muse Spark) close a
        // valid SSE response cleanly without a final [DONE] frame or
        // finish_reason. A transport truncation still arrives as a stream error,
        // so accepting a normal EOF after material output does not hide broken IO.
        || (adapter == "openai" && has_payload)
}

fn compatible_stream_has_material(
    adapter: &str,
    text: &str,
    reasoning: &str,
    calls: &ToolAccumulator,
) -> bool {
    adapter == "openai" && (!text.is_empty() || !reasoning.is_empty() || !calls.is_empty())
}

#[derive(Debug)]
pub struct ProviderTurn {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub finish_reason: Option<String>,
    pub interrupted_by_guidance: bool,
}

#[derive(Debug)]
pub struct ProviderFailure {
    pub error: LocalizedError,
    pub partial: ProviderTurn,
}

pub struct StreamTurnRequest<'a> {
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub model: &'a str,
    pub messages: &'a [Value],
    pub tools: &'a [Value],
    pub active_turn: &'a ActiveTurn,
    pub endpoint: &'a str,
    pub adapter: &'a str,
    pub api_key: &'a str,
    pub user_agent: &'a str,
    pub thinking: ThinkingOptions<'a>,
}

fn stream_failure(
    error: LocalizedError,
    text: &str,
    reasoning: &str,
    usage: &TokenUsage,
    finish_reason: &Option<String>,
) -> ProviderFailure {
    ProviderFailure {
        error,
        partial: ProviderTurn {
            text: text.to_owned(),
            reasoning: reasoning.to_owned(),
            tool_calls: Vec::new(),
            usage: usage.clone(),
            finish_reason: finish_reason.clone(),
            interrupted_by_guidance: false,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderHttpErrorKind {
    FreeUsageLimit,
    RateLimit,
    ModelUnavailable,
    Other,
}

fn provider_error_signal(detail: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(detail) else {
        return detail.to_ascii_lowercase();
    };
    let error = value.get("error").unwrap_or(&value);
    ["type", "code", "message"]
        .into_iter()
        .filter_map(|field| error.get(field).and_then(Value::as_str))
        .chain(
            ["type", "code", "message"]
                .into_iter()
                .filter_map(|field| value.get(field).and_then(Value::as_str)),
        )
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn classify_provider_http_error(
    status: reqwest::StatusCode,
    detail: &str,
) -> ProviderHttpErrorKind {
    let signal = provider_error_signal(detail);

    if signal.contains("freeusagelimiterror") {
        return ProviderHttpErrorKind::FreeUsageLimit;
    }
    // Stable limit types are stronger evidence than a generic capacity phrase
    // that a downstream provider may include in its message.
    if contains_any(&signal, &["gousagelimiterror", "ratelimiterror"]) {
        return ProviderHttpErrorKind::RateLimit;
    }
    if contains_any(
        &signal,
        &[
            "modelunavailableerror",
            "model_not_found",
            "model_unavailable",
            "provider_unavailable",
            "server_is_overloaded",
            "service_unavailable",
            "no available provider",
            "no available endpoint",
            "temporarily unavailable",
            "provider is overloaded",
            "capacity",
        ],
    ) || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
    {
        return ProviderHttpErrorKind::ModelUnavailable;
    }
    if contains_any(
        &signal,
        &[
            "rate_limit",
            "rate limit",
            "too_many_requests",
            "too many requests",
            "insufficient_quota",
            "quota exceeded",
        ],
    ) || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return ProviderHttpErrorKind::RateLimit;
    }
    ProviderHttpErrorKind::Other
}

fn provider_http_error(status: reqwest::StatusCode, detail: &str) -> LocalizedError {
    let key = match classify_provider_http_error(status, detail) {
        ProviderHttpErrorKind::FreeUsageLimit => "error.provider_free_usage_limit",
        ProviderHttpErrorKind::RateLimit => "error.provider_rate_limit",
        ProviderHttpErrorKind::ModelUnavailable => "error.provider_free_model_unavailable",
        ProviderHttpErrorKind::Other => "error.provider_status",
    };
    LocalizedError::new(key)
        .arg("status", status)
        .arg("detail", detail)
}

fn request_error_detail(error: &reqwest::Error) -> String {
    let mut detail = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        detail.push_str(": ");
        detail.push_str(&cause.to_string());
        source = cause.source();
    }
    detail
}

fn transient_transport_signal(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    [
        "connection closed",
        "connection reset",
        "broken pipe",
        "unexpected eof",
        "incomplete message",
        "channel closed",
        "http2 error",
        "stream error",
    ]
    .iter()
    .any(|signal| detail.contains(signal))
}

pub(crate) async fn stream_turn(
    sink: &dyn RuntimeEventSink,
    state: &Arc<AppState>,
    request: StreamTurnRequest<'_>,
) -> Result<ProviderTurn, ProviderFailure> {
    let StreamTurnRequest {
        session_id,
        turn_id,
        model,
        messages,
        tools,
        active_turn,
        endpoint,
        adapter,
        api_key,
        user_agent,
        thinking,
    } = request;
    let api_key = effective_api_key(endpoint, api_key);
    let cancellation = &active_turn.cancellation;
    let guidance_signal = &active_turn.guidance_signal;
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut token_usage = TokenUsage::default();
    let mut finish_reason = None;
    let request = adapter_payload(adapter, model, messages, tools, thinking);
    let mut retry = 0_u32;
    let response = loop {
        let mut builder = state
            .client
            .post(adapter_endpoint(adapter, endpoint, model))
            .json(&request);
        builder = apply_user_agent(builder, user_agent);
        if endpoint.to_ascii_lowercase().contains("opencode.ai/zen") {
            builder = builder
                .header("x-opencode-session", session_id)
                .header("x-opencode-request", turn_id)
                .header("x-opencode-project", "knightframe")
                .header("x-opencode-client", "knightframe");
        }
        builder = match adapter {
            "anthropic" => builder
                .header("anthropic-version", "2023-06-01")
                .header("x-api-key", api_key),
            "gemini" => builder.header("x-goog-api-key", api_key),
            _ if !api_key.is_empty() => builder.bearer_auth(api_key),
            _ => builder,
        };
        let send = builder.send();
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(stream_failure(LocalizedError::new("error.session_cancelled"), &text, &reasoning, &token_usage, &finish_reason)),
            _ = guidance_signal.notified() => return Ok(ProviderTurn { text, reasoning, tool_calls: Vec::new(), usage: token_usage, finish_reason, interrupted_by_guidance: true }),
            response = send => response,
        };
        let response = match response {
            Ok(response) => response,
            Err(error)
                if retry
                    < if error.is_connect() || error.is_timeout() {
                        3
                    } else {
                        1
                    }
                    && (error.is_connect()
                        || error.is_timeout()
                        || transient_transport_signal(&request_error_detail(&error))) =>
            {
                let wait = Duration::from_secs(1_u64 << retry);
                retry += 1;
                tokio::select! {
                    _ = cancellation.cancelled() => return Err(stream_failure(LocalizedError::new("error.session_cancelled"), &text, &reasoning, &token_usage, &finish_reason)),
                    _ = guidance_signal.notified() => return Ok(ProviderTurn { text, reasoning, tool_calls: Vec::new(), usage: token_usage, finish_reason, interrupted_by_guidance: true }),
                    _ = tokio::time::sleep(wait) => {}
                }
                continue;
            }
            Err(error) => {
                return Err(stream_failure(
                    LocalizedError::new("error.provider_request")
                        .arg("detail", request_error_detail(&error)),
                    &text,
                    &reasoning,
                    &token_usage,
                    &finish_reason,
                ));
            }
        };
        if response.status().is_success() {
            break response;
        }
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|seconds| seconds.min(30));
        let detail: String = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(512)
            .collect();
        let retryable = status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status.is_server_error();
        if !retryable || retry >= 3 {
            return Err(stream_failure(
                provider_http_error(status, &detail),
                &text,
                &reasoning,
                &token_usage,
                &finish_reason,
            ));
        }
        let wait = Duration::from_secs(retry_after.unwrap_or(1_u64 << retry));
        retry += 1;
        tokio::select! {
            _ = cancellation.cancelled() => return Err(stream_failure(LocalizedError::new("error.session_cancelled"), &text, &reasoning, &token_usage, &finish_reason)),
            _ = guidance_signal.notified() => return Ok(ProviderTurn { text, reasoning, tool_calls: Vec::new(), usage: token_usage, finish_reason, interrupted_by_guidance: true }),
            _ = tokio::time::sleep(wait) => {}
        }
    };
    let mut stream = response.bytes_stream().eventsource();
    let mut got_done = false;
    let mut calls = ToolAccumulator::default();
    loop {
        let event = tokio::select! {
            _ = cancellation.cancelled() => return Err(stream_failure(LocalizedError::new("error.session_cancelled"), &text, &reasoning, &token_usage, &finish_reason)),
            _ = guidance_signal.notified() => return Ok(ProviderTurn { text, reasoning, tool_calls: Vec::new(), usage: token_usage, finish_reason, interrupted_by_guidance: true }),
            event = stream.next() => event,
        };
        let Some(event) = event else { break };
        let event = match event {
            Ok(event) => event,
            Err(_error) if compatible_stream_has_material(adapter, &text, &reasoning, &calls) => {
                // Several compatible servers close the socket after their last
                // material frame instead of sending a final SSE terminator.
                // Preserve that output; a reasoning-only turn is continued by
                // the agent loop so it cannot disappear as an empty answer.
                break;
            }
            Err(error) => {
                return Err(stream_failure(
                    LocalizedError::new("error.provider_stream").arg("detail", error),
                    &text,
                    &reasoning,
                    &token_usage,
                    &finish_reason,
                ));
            }
        };
        let parsed = match parse_native_sse(adapter, &event.data) {
            Ok(parsed) => parsed,
            Err(error)
                if error.key == "error.provider_sse_decode"
                    && compatible_stream_has_material(adapter, &text, &reasoning, &calls) =>
            {
                // Preserve valid output when a compatibility server appends a
                // malformed tail. Incomplete tool JSON is still rejected by
                // ToolAccumulator::finish; reasoning-only output is finalized
                // by the agent loop.
                break;
            }
            Err(error) => {
                return Err(stream_failure(
                    error,
                    &text,
                    &reasoning,
                    &token_usage,
                    &finish_reason,
                ));
            }
        };
        let frame_done = parsed.done;
        if let Some(delta) = parsed.reasoning {
            reasoning.push_str(&delta);
            sink.emit(
                RuntimeEvent::new(
                    "assistant.reasoning_delta",
                    json!({"turnId": turn_id, "delta": delta}),
                )
                .session(session_id),
            );
        }
        if let Some(delta) = parsed.text {
            text.push_str(&delta);
            sink.emit(
                RuntimeEvent::new(
                    "assistant.text_delta",
                    json!({"turnId": turn_id, "delta": delta}),
                )
                .session(session_id),
            );
        }
        if let Some(value) = parsed.usage {
            if adapter == "anthropic" {
                token_usage.input_tokens = token_usage.input_tokens.max(value.input_tokens);
                token_usage.cached_input_tokens = token_usage
                    .cached_input_tokens
                    .max(value.cached_input_tokens);
                token_usage.output_tokens = token_usage.output_tokens.max(value.output_tokens);
                token_usage.reasoning_tokens =
                    token_usage.reasoning_tokens.max(value.reasoning_tokens);
                token_usage.reported |= value.reported;
            } else {
                token_usage = value;
            }
        }
        if parsed.finish_reason.is_some() {
            finish_reason = parsed.finish_reason;
        }
        for delta in &parsed.tool_deltas {
            calls.apply(delta).map_err(|error| {
                stream_failure(error, &text, &reasoning, &token_usage, &finish_reason)
            })?;
        }
        if frame_done {
            got_done = true;
            break;
        }
    }
    let has_payload = !text.is_empty() || !reasoning.is_empty() || !calls.is_empty();
    if !stream_completed(adapter, got_done, &finish_reason, has_payload) {
        return Err(stream_failure(
            LocalizedError::new("error.provider_stream_incomplete"),
            &text,
            &reasoning,
            &token_usage,
            &finish_reason,
        ));
    }
    match finish_reason.as_deref() {
        Some("stop" | "tool_calls" | "length") | None => {}
        Some(reason) => {
            return Err(stream_failure(
                LocalizedError::new("error.provider_finish_reason").arg("reason", reason),
                &text,
                &reasoning,
                &token_usage,
                &finish_reason,
            ));
        }
    }
    let tool_calls = calls
        .finish()
        .map_err(|error| stream_failure(error, &text, &reasoning, &token_usage, &finish_reason))?;
    if finish_reason.as_deref() == Some("tool_calls") && tool_calls.is_empty() {
        return Err(stream_failure(
            LocalizedError::new("error.provider_tool_capability"),
            &text,
            &reasoning,
            &token_usage,
            &finish_reason,
        ));
    }
    Ok(ProviderTurn {
        text,
        reasoning,
        tool_calls,
        usage: token_usage,
        finish_reason,
        interrupted_by_guidance: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const THINKING_OFF: ThinkingOptions<'static> = ThinkingOptions {
        enabled: false,
        effort: "medium",
    };

    fn profile(adapter: &str) -> ProviderProfile {
        ProviderProfile {
            id: "test".into(),
            name: "Test".into(),
            adapter: adapter.into(),
            base_url: "https://opencode.ai/zen/v1".into(),
            user_agent: String::new(),
            api_key: String::new(),
            credential_ref: String::new(),
            models: Vec::new(),
        }
    }

    fn discovered(id: &str) -> ConfiguredModel {
        ConfiguredModel {
            id: id.into(),
            name: id.into(),
            adapter: None,
            capabilities: vec!["streaming".into()],
            context_limit: None,
            thinking_enabled: false,
            thinking_effort: "medium".into(),
            thinking_toggle: false,
            thinking_efforts: Vec::new(),
            catalog_synced: false,
        }
    }

    #[test]
    fn models_dev_metadata_selects_protocol_and_exact_reasoning_variants() {
        let metadata = json!({
            "npm":"@ai-sdk/openai-compatible",
            "models":{
                "mimo-v2.5-free":{
                    "name":"MiMo V2.5 Free","attachment":true,"reasoning":true,
                    "reasoning_options":[],"tool_call":true,
                    "limit":{"context":200000},"modalities":{"input":["text","image"]}
                },
                "hy3-free":{
                    "name":"Hy3 Free","reasoning":true,"tool_call":true,
                    "reasoning_options":[{"type":"toggle"},{"type":"effort","values":["low","medium","high"]}],
                    "limit":{"context":190000}
                },
                "muse-spark-free":{
                    "name":"Muse Spark","reasoning":true,"tool_call":true,
                    "reasoning_options":[{"type":"effort","values":["minimal","xhigh"]}],
                    "provider":{"npm":"@ai-sdk/openai"}
                }
            }
        });
        let profile = profile("openai-responses");
        let mimo = enrich_from_models_dev(&profile, &metadata, discovered("mimo-v2.5-free"));
        assert_eq!(mimo.adapter.as_deref(), Some("openai"));
        assert_eq!(mimo.context_limit, Some(200_000));
        assert!(mimo.capabilities.contains(&"imageInput".into()));
        assert!(mimo.capabilities.contains(&"reasoning".into()));
        assert!(
            mimo.thinking_efforts.is_empty(),
            "MiMo reasoning is model-managed"
        );

        let hy3 = enrich_from_models_dev(&profile, &metadata, discovered("hy3-free"));
        assert!(hy3.thinking_toggle);
        assert_eq!(hy3.thinking_efforts, ["low", "medium", "high"]);

        let muse = enrich_from_models_dev(&profile, &metadata, discovered("muse-spark-free"));
        assert_eq!(muse.adapter.as_deref(), Some("openai-responses"));
        assert_eq!(muse.thinking_efforts, ["minimal", "xhigh"]);
    }

    #[test]
    fn mixed_gateway_adapter_is_per_model_and_explicit_override_wins() {
        let mut profile = profile("openai-responses");
        assert_eq!(model_adapter(&profile, "mimo-v2.5-free"), "openai");
        assert_eq!(model_adapter(&profile, "gpt-5.4"), "openai-responses");
        assert_eq!(model_adapter(&profile, "claude-sonnet-5"), "anthropic");
        assert_eq!(model_adapter(&profile, "gemini-3-flash"), "gemini");
        let mut model = discovered("mimo-v2.5-free");
        model.adapter = Some("gemini".into());
        profile.models.push(model);
        assert_eq!(model_adapter(&profile, "mimo-v2.5-free"), "gemini");
    }

    #[test]
    fn anonymous_zen_requests_use_the_same_public_key_contract() {
        assert_eq!(
            effective_api_key("https://opencode.ai/zen/v1", ""),
            "public"
        );
        assert_eq!(effective_api_key("https://example.com/v1", ""), "");
        assert_eq!(
            effective_api_key("https://opencode.ai/zen/v1", "user-key"),
            "user-key"
        );
    }

    #[test]
    fn configured_user_agent_is_applied_exactly_and_blank_is_truthful() {
        assert_eq!(effective_user_agent(""), DEFAULT_USER_AGENT);
        assert_eq!(
            effective_user_agent("  claude-code/2.1.113 (external, cli)  "),
            "claude-code/2.1.113 (external, cli)"
        );

        let request = apply_user_agent(
            reqwest::Client::new().get("https://example.com/v1/models"),
            "coding-plan-client/7",
        )
        .build()
        .expect("valid request");
        assert_eq!(
            request
                .headers()
                .get(reqwest::header::USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some("coding-plan-client/7")
        );
    }

    #[test]
    fn invalid_user_agent_is_rejected_before_network_access() {
        let mut profile = profile("openai");
        profile.user_agent = "invalid\r\nheader: value".into();
        let error = validate_profiles(&[profile]).expect_err("header injection must be rejected");
        assert_eq!(error.key, "error.provider_user_agent");
    }

    #[test]
    fn only_transient_stream_transport_failures_are_retryable_by_signal() {
        for detail in [
            "connection closed before message completed",
            "connection reset by peer",
            "unexpected EOF while sending request",
            "HTTP2 error: stream error received",
        ] {
            assert!(transient_transport_signal(detail), "{detail}");
        }
        assert!(!transient_transport_signal("invalid URL"));
        assert!(!transient_transport_signal("certificate expired"));
    }

    #[test]
    fn malformed_compatible_tail_is_recoverable_only_after_material() {
        let calls = ToolAccumulator::default();
        assert!(!compatible_stream_has_material("openai", "", "", &calls));
        assert!(compatible_stream_has_material(
            "openai", "answer", "", &calls
        ));
        assert!(compatible_stream_has_material(
            "openai", "", "thought", &calls
        ));
        assert!(!compatible_stream_has_material(
            "anthropic",
            "answer",
            "",
            &calls
        ));
    }

    #[test]
    fn live_catalog_includes_future_free_models_and_excludes_paid_models() {
        let models = catalog_from_ids(["future-code-9-free".to_owned(), "paid-code-9".to_owned()]);

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model_id, "future-code-9-free");
        assert_eq!(models[0].model_name, "Future Code 9");
        assert!(models[0].available);
        assert!(models[0].capabilities.contains(&"toolCalls".to_owned()));
        assert_eq!(models[0].context_limit, None);
    }

    #[test]
    fn live_catalog_does_not_guess_context_limits() {
        let models = catalog_from_ids([
            "nemotron-3-ultra-free".to_owned(),
            "hy3-free".to_owned(),
            "laguna-s-2.1-free".to_owned(),
        ]);
        let limits: BTreeMap<_, _> = models
            .iter()
            .map(|model| (model.model_id.as_str(), model.context_limit))
            .collect();

        assert!(limits.values().all(Option::is_none));
    }

    #[test]
    fn endpoint_paths_do_not_depend_on_trailing_slashes() {
        assert_eq!(
            endpoint_url("http://127.0.0.1:8080/v1/", "chat/completions"),
            "http://127.0.0.1:8080/v1/chat/completions"
        );
    }

    #[test]
    fn provider_native_payloads_preserve_images_and_tools() {
        let messages = vec![
            json!({"role":"system","content":"system"}),
            json!({"role":"user","content":[{"type":"text","text":"inspect"},{"type":"image_url","image_url":{"url":"data:image/png;base64,AA=="}}]}),
        ];
        let tools = vec![
            json!({"type":"function","function":{"name":"read","description":"Read","parameters":{"type":"object"}}}),
        ];
        let responses = openai_responses_payload("model", &messages, &tools, THINKING_OFF);
        assert_eq!(responses["input"][0]["content"][1]["type"], "input_image");
        assert_eq!(responses["tools"][0]["name"], "read");
        let anthropic = anthropic_payload("model", &messages, &tools, THINKING_OFF);
        assert_eq!(
            anthropic["messages"][0]["content"][1]["source"]["media_type"],
            "image/png"
        );
        assert_eq!(anthropic["tools"][0]["input_schema"]["type"], "object");
        let gemini = gemini_payload("gemini-3-flash", &messages, &tools, THINKING_OFF);
        assert_eq!(
            gemini["contents"][0]["parts"][1]["inlineData"]["mimeType"],
            "image/png"
        );
        assert_eq!(
            gemini["tools"][0]["functionDeclarations"][0]["name"],
            "read"
        );
    }

    #[test]
    fn native_stream_parsers_keep_text_reasoning_tools_and_usage_separate() {
        let text = parse_native_sse(
            "openai-responses",
            r#"{"type":"response.output_text.delta","delta":"ok"}"#,
        )
        .unwrap();
        assert_eq!(text.text.as_deref(), Some("ok"));
        let reasoning = parse_native_sse("anthropic", r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"why"}}"#).unwrap();
        assert_eq!(reasoning.reasoning.as_deref(), Some("why"));
        let gemini = parse_native_sse("gemini", r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"read","args":{"path":"x"}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":9,"cachedContentTokenCount":4,"candidatesTokenCount":2}}"#).unwrap();
        assert_eq!(gemini.finish_reason.as_deref(), Some("stop"));
        assert_eq!(gemini.tool_deltas.len(), 1);
        assert_eq!(gemini.usage.unwrap().cached_input_tokens, 4);
    }

    #[test]
    fn reasoning_controls_translate_to_each_wire_protocol() {
        let messages = vec![json!({"role":"user","content":"solve"})];
        let tools = vec![
            json!({"type":"function","function":{"name":"read","parameters":{"type":"object"}}}),
        ];
        let thinking = ThinkingOptions {
            enabled: true,
            effort: "high",
        };
        let chat = stream_request_payload("model", &messages, &tools, thinking);
        assert_eq!(chat["reasoning_effort"], "high");
        let responses = openai_responses_payload("model", &messages, &tools, thinking);
        assert_eq!(responses["reasoning"]["effort"], "high");
        assert_eq!(responses["reasoning"]["summary"], "auto");
        let anthropic = anthropic_payload("model", &messages, &tools, thinking);
        assert_eq!(anthropic["thinking"]["type"], "enabled");
        assert_eq!(anthropic["thinking"]["budget_tokens"], 8192);
        let gemini_25 = gemini_payload("gemini-2.5-flash", &messages, &tools, thinking);
        assert_eq!(
            gemini_25["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            8192
        );
        let gemini_3 = gemini_payload("gemini-3-flash", &messages, &tools, thinking);
        assert_eq!(
            gemini_3["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "high"
        );
    }

    #[test]
    fn compatible_protocol_accepts_mimo_and_gateway_variants() {
        let mimo = parse_sse_data(
            r#"{"choices":[{"delta":{"content":"","reasoning":"plan","reasoning_details":[{"type":"reasoning.text","text":"plan"}]},"finish_reason":null}]}"#,
        )
        .unwrap();
        assert_eq!(mimo.reasoning.as_deref(), Some("plan"));
        assert!(mimo.text.is_none());

        let terminal = parse_sse_data(
            r#"{"choices":[{"message":{"content":[{"type":"text","text":"done"}],"tool_calls":[{"function":{"name":"read","arguments":{"path":"a.rs"}}}]},"finish_reason":"tool_use"}]}"#,
        )
        .unwrap();
        assert_eq!(terminal.text.as_deref(), Some("done"));
        assert_eq!(terminal.finish_reason.as_deref(), Some("tool_calls"));
        let mut calls = ToolAccumulator::default();
        calls.apply(&terminal.tool_deltas[0]).unwrap();
        assert_eq!(calls.finish().unwrap()[0].arguments["path"], "a.rs");
    }

    #[test]
    fn legacy_model_settings_get_safe_thinking_defaults() {
        let model: ConfiguredModel = serde_json::from_value(json!({
            "id":"legacy",
            "name":"Legacy",
            "capabilities":["streaming"],
            "contextLimit":null
        }))
        .unwrap();
        assert!(!model.thinking_enabled);
        assert_eq!(model.thinking_effort, "medium");
        assert!(!model.thinking_toggle);
        assert!(model.thinking_efforts.is_empty());
        assert!(!model.catalog_synced);
    }

    #[test]
    fn desktop_defaults_have_provider_templates_but_no_models() {
        assert!(fallback_models().is_empty());
        assert!(
            PROVIDER_TEMPLATES
                .iter()
                .any(|template| template.adapter == "openai-responses")
        );
        assert!(
            PROVIDER_TEMPLATES
                .iter()
                .any(|template| template.adapter == "anthropic")
        );
        assert!(
            PROVIDER_TEMPLATES
                .iter()
                .any(|template| template.adapter == "gemini")
        );
    }

    #[test]
    fn live_catalog_deduplicates_model_ids() {
        let models = catalog_from_ids([
            "future-code-9-free".to_owned(),
            "future-code-9-free".to_owned(),
        ]);

        assert_eq!(models.len(), 1);
    }

    #[test]
    fn offline_fallback_is_never_selectable() {
        let models = fallback_models();

        assert!(models.is_empty());
    }

    #[test]
    fn parses_usage_without_counting_cache_as_fresh() {
        let parsed = parse_sse_data(r#"{"choices":[],"usage":{"prompt_tokens":100,"prompt_tokens_details":{"cached_tokens":80},"completion_tokens":5}}"#).unwrap();
        let usage = parsed.usage.unwrap();
        assert_eq!(usage.cached_input_tokens, 80);
        assert_eq!(usage.fresh_input_tokens(), 20);
        assert!(usage.reported);
    }

    #[test]
    fn normalizes_only_provider_reported_cache_aliases() {
        for (raw, input, cached, output, reasoning) in [
            (
                r#"{"prompt_tokens":100,"prompt_cache_hit_tokens":80,"prompt_cache_miss_tokens":20,"completion_tokens":5}"#,
                100,
                80,
                5,
                0,
            ),
            (
                r#"{"prompt_cache_hit_tokens":80,"prompt_cache_miss_tokens":20,"completion_tokens":5}"#,
                100,
                80,
                5,
                0,
            ),
            (
                r#"{"input_tokens":100,"input_tokens_details":{"cached_tokens":80},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":4}}"#,
                100,
                80,
                5,
                4,
            ),
            (
                r#"{"input_tokens":20,"cache_read_input_tokens":80,"output_tokens":5}"#,
                100,
                80,
                5,
                0,
            ),
        ] {
            let value: Value = serde_json::from_str(raw).unwrap();
            let usage = usage(&value);
            assert_eq!(usage.input_tokens, input, "{raw}");
            assert_eq!(usage.cached_input_tokens, cached, "{raw}");
            assert_eq!(usage.output_tokens, output, "{raw}");
            assert_eq!(usage.reasoning_tokens, reasoning, "{raw}");
            assert!(usage.reported);
        }

        let incomplete: Value =
            serde_json::from_str(r#"{"prompt_cache_hit_tokens":80,"completion_tokens":1}"#)
                .unwrap();
        let usage = usage(&incomplete);
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.cached_input_tokens, 0);
    }

    #[test]
    fn stream_payload_is_deterministic_and_excludes_runtime_metadata() {
        let messages = vec![json!({"role":"user","content":"hello"})];
        let tools = vec![json!({"type":"function","function":{"name":"read"}})];
        let first = stream_request_payload("hy3-free", &messages, &tools, THINKING_OFF);
        let second = stream_request_payload("hy3-free", &messages, &tools, THINKING_OFF);
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
        for key in ["turnId", "sessionId", "eventId", "timestamp"] {
            assert!(first.get(key).is_none(), "runtime key leaked: {key}");
        }
    }

    #[test]
    fn tool_arguments_interleave_by_index() {
        let mut calls = ToolAccumulator::default();
        calls
            .apply(&json!({"index":1,"id":"b","function":{"name":"read","arguments":"{\"path\":"}}))
            .unwrap();
        calls.apply(&json!({"index":0,"id":"a","function":{"name":"project","arguments":"{\"query\":\"x\"}"}})).unwrap();
        calls
            .apply(&json!({"index":1,"function":{"arguments":"\"a.rs\"}"}}))
            .unwrap();
        let calls = calls.finish().unwrap();
        assert_eq!(calls[0].id, "a");
        assert_eq!(calls[1].arguments["path"], "a.rs");
    }

    #[tokio::test]
    async fn split_crlf_comment_and_multidata_sse_frames() {
        use futures_util::stream;
        let chunks: Vec<Result<bytes::Bytes, std::io::Error>> = vec![
            Ok(bytes::Bytes::from_static(b": keepalive\r\nda")),
            Ok(bytes::Bytes::from_static(
                b"ta: {\"a\":1}\r\ndata: {\"b\":2}\r\n\r\n",
            )),
        ];
        let events: Vec<_> = stream::iter(chunks).eventsource().collect().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_ref().unwrap().data, "{\"a\":1}\n{\"b\":2}");
    }

    #[test]
    fn empty_usage_and_done_are_valid() {
        assert!(parse_sse_data("").unwrap().text.is_none());
        assert!(parse_sse_data("[DONE]").unwrap().done);
    }

    #[test]
    fn compatible_stream_accepts_clean_eof_after_finish_reason() {
        assert!(stream_completed(
            "openai",
            false,
            &Some("stop".into()),
            false
        ));
        assert!(stream_completed(
            "openai",
            false,
            &Some("tool_calls".into()),
            false
        ));
        assert!(stream_completed("openai", false, &None, true));
        assert!(!stream_completed("openai", false, &None, false));
        assert!(!stream_completed("anthropic", false, &None, true));
    }

    #[test]
    fn classifies_live_opencode_free_usage_limit_fixture() {
        let body = include_str!("../tests/fixtures/provider/free_usage_limit.json");
        assert_eq!(
            classify_provider_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, body),
            ProviderHttpErrorKind::FreeUsageLimit
        );
        assert_eq!(
            provider_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, body).key,
            "error.provider_free_usage_limit"
        );
    }

    #[test]
    fn classic_rate_limit_stays_a_rate_limit() {
        let body = include_str!("../tests/fixtures/provider/rate_limit.json");
        assert_eq!(
            classify_provider_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, body),
            ProviderHttpErrorKind::RateLimit
        );
        assert_eq!(
            provider_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, body).key,
            "error.provider_rate_limit"
        );
    }

    #[test]
    fn requirement_reducer_request_is_non_streaming_tool_free_and_bounded() {
        let payload = requirement_reducer_payload(AUXILIARY_MODEL_ID, "long request");
        assert_eq!(payload["model"], AUXILIARY_MODEL_ID);
        assert_eq!(payload["stream"], false);
        assert_eq!(payload["max_tokens"], REQUIREMENT_REDUCER_MAX_TOKENS);
        assert!(payload.get("tools").is_none());
        assert_eq!(payload["messages"][1]["content"], "long request");
    }

    #[tokio::test]
    async fn requirement_reducer_honors_pre_cancelled_turn_without_network() {
        let cancellation = tokio_util::sync::CancellationToken::new();
        cancellation.cancel();
        let error = reduce_requirement(
            &reqwest::Client::new(),
            "http://127.0.0.1:1/v1",
            AUXILIARY_MODEL_ID,
            "request",
            &cancellation,
        )
        .await
        .expect_err("a cancelled turn must not start the reducer request");
        assert_eq!(error.key, "error.session_cancelled");
    }

    #[test]
    fn capacity_and_model_unavailable_are_not_misreported_as_rate_limits() {
        for body in [
            include_str!("../tests/fixtures/provider/capacity_unavailable.json"),
            include_str!("../tests/fixtures/provider/model_unavailable.json"),
        ] {
            assert_eq!(
                classify_provider_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, body),
                ProviderHttpErrorKind::ModelUnavailable
            );
            assert_eq!(
                provider_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, body).key,
                "error.provider_free_model_unavailable"
            );
        }
    }

    #[test]
    fn unrelated_provider_failure_stays_generic_and_keeps_raw_detail() {
        let body = include_str!("../tests/fixtures/provider/provider_error.json").trim();
        let error = provider_http_error(reqwest::StatusCode::BAD_REQUEST, body);
        assert_eq!(error.key, "error.provider_status");
        assert_eq!(error.args.get("detail").map(String::as_str), Some(body));
        assert_eq!(
            error.args.get("status").map(String::as_str),
            Some("400 Bad Request")
        );
    }
}
