use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub locale: String,
    pub task_manager: bool,
    pub caveman_mode: String,
    pub usage_panel: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_avatar: Option<String>,
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub providers: Vec<ProviderProfile>,
    #[serde(default)]
    pub auxiliary_enabled: bool,
    #[serde(default)]
    pub auxiliary_provider_id: String,
    #[serde(default)]
    pub auxiliary_model_id: String,
    #[serde(default = "default_true")]
    pub skill_router: bool,
    #[serde(default = "default_true")]
    pub skill_opt: bool,
    /// 界面缩放（0.85–1.30，1.0 = 原生）。字体/控件整体随 zoom 缩放，
    /// 旧配置文件没有此字段时回落 1.0。
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f64,
}

fn default_true() -> bool {
    true
}

fn default_ui_scale() -> f64 {
    1.0
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            locale: "zh-CN".into(),
            task_manager: true,
            caveman_mode: "lite".into(),
            usage_panel: true,
            user_avatar: None,
            provider_id: String::new(),
            model_id: String::new(),
            providers: Vec::new(),
            auxiliary_enabled: false,
            auxiliary_provider_id: String::new(),
            auxiliary_model_id: String::new(),
            skill_router: true,
            skill_opt: true,
            ui_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub locale: Option<String>,
    pub task_manager: Option<bool>,
    pub caveman_mode: Option<String>,
    pub usage_panel: Option<bool>,
    pub user_avatar: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub providers: Option<Vec<ProviderProfile>>,
    pub auxiliary_enabled: Option<bool>,
    pub auxiliary_provider_id: Option<String>,
    pub auxiliary_model_id: Option<String>,
    pub skill_router: Option<bool>,
    pub skill_opt: Option<bool>,
    pub ui_scale: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub adapter: String,
    pub base_url: String,
    /// Optional exact User-Agent override for coding-plan gateways that route
    /// by client identity. Empty keeps KnightFrame's truthful default.
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub credential_ref: String,
    #[serde(default)]
    pub models: Vec<ConfiguredModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredModel {
    pub id: String,
    pub name: String,
    /// Optional per-model override for providers that expose mixed protocols.
    #[serde(default)]
    pub adapter: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub context_limit: Option<u64>,
    /// Normalized model-level reasoning controls. Adapters translate these to
    /// their native wire format; older settings remain valid and default off.
    #[serde(default)]
    pub thinking_enabled: bool,
    #[serde(default = "default_thinking_effort")]
    pub thinking_effort: String,
    /// Whether the upstream exposes an explicit reasoning on/off variant.
    #[serde(default)]
    pub thinking_toggle: bool,
    /// Exact effort values advertised by the upstream model catalog.
    #[serde(default)]
    pub thinking_efforts: Vec<String>,
    /// True after protocol/capability metadata was resolved from the catalog.
    #[serde(default)]
    pub catalog_synced: bool,
}

fn default_thinking_effort() -> String {
    "medium".into()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub adapter: &'static str,
    pub base_url: &'static str,
    pub api_key_env: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    pub provider_id: String,
    pub provider_name: String,
    pub model_id: String,
    pub model_name: String,
    pub available: bool,
    pub capabilities: Vec<String>,
    pub context_limit: Option<u64>,
    pub thinking_enabled: bool,
    pub thinking_effort: String,
    pub thinking_toggle: bool,
    pub thinking_efforts: Vec<String>,
    pub adapter: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSnapshot {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<MessageAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachment {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub data_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub fresh_input_tokens: u64,
    pub cache_read_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub request_count: u64,
    pub current_context_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub id: String,
    pub title: String,
    pub provider_id: String,
    pub model_id: String,
    pub project_root: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<crate::error::LocalizedError>,
    pub messages: Vec<MessageSnapshot>,
    pub task: Option<TaskSnapshot>,
    pub usage: UsageSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub id: String,
    pub title: String,
    pub detail: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub id: String,
    pub status: String,
    pub completed: usize,
    pub total: usize,
    pub current: Option<String>,
    pub items: Vec<TaskItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub root: String,
    pub name: String,
    pub status: String,
    pub stage: String,
    pub completed: usize,
    pub total: usize,
    pub files: usize,
    pub languages: Vec<String>,
    pub failures: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphStats {
    pub files: usize,
    pub directories: usize,
    pub dependencies: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSnapshot {
    pub root: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub stats: GraphStats,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTabSnapshot {
    pub id: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub loading: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshot {
    pub available: bool,
    pub open: bool,
    pub url: Option<String>,
    pub title: Option<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub loading: bool,
    pub active_tab_id: Option<String>,
    pub tabs: Vec<BrowserTabSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPayload {
    pub settings: SettingsSnapshot,
    pub providers: Vec<ProviderModel>,
    pub provider_templates: Vec<ProviderTemplate>,
    pub sessions: Vec<SessionSnapshot>,
    pub active_session_id: Option<String>,
    pub project: Option<ProjectSnapshot>,
    pub browser: BrowserSnapshot,
    pub features: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    pub event_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub kind: String,
    pub data: Value,
}

impl RuntimeEvent {
    pub fn new(kind: impl Into<String>, data: Value) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            session_id: None,
            task_id: None,
            kind: kind.into(),
            data,
        }
    }

    pub fn session(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }
}

#[derive(Debug, Serialize)]
pub struct Ack {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnReceipt {
    pub turn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HistoryItem {
    User {
        turn_id: String,
        content: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<MessageAttachment>,
    },
    Context {
        source: String,
        content: String,
    },
    Assistant {
        turn_id: String,
        content: String,
        reasoning: String,
    },
    ToolCall {
        turn_id: String,
        call_id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        turn_id: String,
        call_id: String,
        projection: Value,
        artifact_id: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_do_not_receive_a_hidden_builtin_model() {
        let settings: SettingsSnapshot = serde_json::from_str(
            r#"{"locale":"zh-CN","taskManager":true,"cavemanMode":"lite","usagePanel":true}"#,
        )
        .expect("legacy settings should remain readable");

        assert!(settings.provider_id.is_empty());
        assert!(settings.model_id.is_empty());
        assert!(settings.providers.is_empty());
        assert!(!settings.auxiliary_enabled);
        assert!(settings.auxiliary_provider_id.is_empty());
        assert!(settings.auxiliary_model_id.is_empty());
        assert!(settings.skill_router);
        assert!(settings.skill_opt);
    }

    #[test]
    fn future_free_model_selection_round_trips_through_settings_json() {
        let settings = SettingsSnapshot {
            provider_id: "custom".into(),
            model_id: "future-code-9-free".into(),
            ..Default::default()
        };

        let encoded = serde_json::to_string(&settings).expect("settings should serialize");
        let decoded: SettingsSnapshot =
            serde_json::from_str(&encoded).expect("settings should deserialize");

        assert_eq!(decoded.provider_id, "custom");
        assert_eq!(decoded.model_id, "future-code-9-free");
    }

    #[test]
    fn legacy_provider_profiles_default_to_knightframe_identity() {
        let profile: ProviderProfile = serde_json::from_str(
            r#"{"id":"custom","name":"Custom","adapter":"openai","baseUrl":"https://example.com/v1","models":[]}"#,
        )
        .expect("legacy provider profiles should remain readable");

        assert!(profile.user_agent.is_empty());
    }
}
