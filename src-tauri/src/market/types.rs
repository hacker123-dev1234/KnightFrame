//! PA Agent 移植层：K 线数据结构、指标与市场设置模型。
//! 数据语义与 Python 版对齐：bars[0] = 最新（seq=1），bars[N-1] = 最旧（seq=N）；
//! 未收盘棒 seq=0，不进入分析快照。

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const INDICATOR_WARMUP_BARS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlineBar {
    pub seq: u32,
    pub ts_open: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub closed: bool,
}

impl KlineBar {
    /// 与 Python `normalize_kline_bar` 一致：保证毫秒时间戳、high≥low、close 夹在区间内。
    pub fn normalized(mut self) -> Self {
        if self.ts_open < 1e10 {
            self.ts_open *= 1000.0;
        }
        self.high = self.high.max(self.low);
        self.low = self.high.min(self.low);
        self.close = self.close.clamp(self.low, self.high);
        self
    }

    pub fn body_ratio(&self) -> Option<f64> {
        let range = self.high - self.low;
        if range <= 0.0 {
            None
        } else {
            Some((self.close - self.open).abs() / range)
        }
    }

    pub fn close_position(&self) -> Option<f64> {
        let range = self.high - self.low;
        if range <= 0.0 {
            None
        } else {
            Some(((self.close - self.low) / range).clamp(0.0, 1.0))
        }
    }

    pub fn is_bull(&self) -> bool {
        self.close > self.open
    }

    /// 蜡烛方向中文标签（进入 prompt）。
    pub fn candle_label(&self) -> &'static str {
        if self.close > self.open {
            "阳线"
        } else if self.close < self.open {
            "阴线"
        } else {
            "平"
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndicatorBundle {
    pub ema20: Vec<Option<f64>>,
    pub atr14: Vec<Option<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlineFrame {
    pub symbol: String,
    pub timeframe: String,
    pub bars: Vec<KlineBar>,
    pub indicators: IndicatorBundle,
    pub snapshot_ts_local_ms: u64,
}

/// K 线几何特征（16 项），字段名与 Python `KlineGeometryFeature` 对齐。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GeometryFeature {
    pub seq: u32,
    pub bar_type: String,
    pub body_ratio: Option<f64>,
    pub upper_wick_ratio: Option<f64>,
    pub lower_wick_ratio: Option<f64>,
    pub close_position: Option<f64>,
    pub range_atr_ratio: Option<f64>,
    pub ema_relation: String,
    pub overlap_prev_ratio: Option<f64>,
    pub inside_sequence: String,
    pub ioi_pattern: bool,
    pub micro_double: String,
    pub gap_bar: String,
    pub ema_gap_count: Option<u32>,
    pub breakout_prev: String,
    pub follow_through_1_2: String,
}

// ---------------------------------------------------------------------------
// 市场设置（对应 PA settings.json 的四组配置）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderSettings {
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub thinking: bool,
    pub reasoning_effort: String,
    pub context_window: u64,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            model: "nemotron-3-ultra-free".into(),
            base_url: "https://opencode.ai/zen/v1".into(),
            api_key: "public".into(),
            thinking: true,
            reasoning_effort: "max".into(),
            context_window: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralSettings {
    pub analysis_bar_count: u32,
    pub refresh_interval_ms: u64,
    pub context_warning_threshold_pct: f64,
    pub last_data_source: String,
    pub last_tradingview_exchange: String,
    pub last_symbol: String,
    pub last_timeframe: String,
    pub decision_flow_auto_play: bool,
    pub decision_flow_play_seconds: u32,
    pub alert_on_order_opportunity: bool,
    pub incremental_max_new_bars: u32,
    pub decision_stance: String,
    pub keep_analysis: bool,
    pub cancel_keep_analysis_on_retry: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            analysis_bar_count: 100,
            refresh_interval_ms: 1000,
            context_warning_threshold_pct: 80.0,
            // 默认东财（大陆直连零配置；XAUUSD 经别名映射上金所 Au99.99），
            // 其余源失败时 fetch_bars_resolved 会跨源兜底。
            last_data_source: "eastmoney".into(),
            last_tradingview_exchange: "OANDA".into(),
            last_symbol: "XAUUSD".into(),
            last_timeframe: "15m".into(),
            decision_flow_auto_play: true,
            decision_flow_play_seconds: 50,
            alert_on_order_opportunity: true,
            incremental_max_new_bars: 10,
            decision_stance: "balanced".into(),
            keep_analysis: false,
            cancel_keep_analysis_on_retry: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PromptSettings {
    pub stage2_load_full_strategy_library: bool,
    pub experience_max_entries: u32,
    pub experience_max_chars_per_entry: u32,
    pub stage1_inject_pattern_briefs: bool,
}

impl Default for PromptSettings {
    fn default() -> Self {
        Self {
            stage2_load_full_strategy_library: false,
            experience_max_entries: 3,
            experience_max_chars_per_entry: 400,
            stage1_inject_pattern_briefs: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ValidationSettings {
    pub normalization_mode: String,
    pub stage1_coherence_checks: bool,
    pub stage2_coherence_checks: bool,
    pub trace_semantic_checks: bool,
    pub disable_truncation_repair: bool,
    pub retry_enabled: bool,
    pub retry_max: u32,
    pub retry_max_semantic: u32,
    pub retry_stage2: bool,
}

impl Default for ValidationSettings {
    fn default() -> Self {
        Self {
            normalization_mode: "lenient".into(),
            stage1_coherence_checks: false,
            stage2_coherence_checks: false,
            trace_semantic_checks: false,
            disable_truncation_repair: false,
            retry_enabled: true,
            retry_max: 3,
            retry_max_semantic: 1,
            retry_stage2: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MarketSettings {
    pub provider: ProviderSettings,
    pub general: GeneralSettings,
    pub prompt: PromptSettings,
    pub validation: ValidationSettings,
}

impl MarketSettings {
    pub fn stance(&self) -> &str {
        &self.general.decision_stance
    }
}

// ---------------------------------------------------------------------------
// 记录 schema（AnalysisRecord / FollowupTurn）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRecord {
    pub meta: Value,
    pub kline_data: Vec<KlineBar>,
    #[serde(default)]
    pub htf_text: String,
    #[serde(default)]
    pub stage1_messages: Vec<Value>,
    #[serde(default)]
    pub stage1_response: Option<Value>,
    #[serde(default)]
    pub stage1_diagnosis: Option<Value>,
    #[serde(default)]
    pub stage2_messages: Vec<Value>,
    #[serde(default)]
    pub stage2_response: Option<Value>,
    #[serde(default)]
    pub stage2_decision: Option<Value>,
    #[serde(default)]
    pub strategy_files_used: Vec<String>,
    #[serde(default)]
    pub experience_loaded: Vec<Value>,
    #[serde(default)]
    pub exception: Option<Value>,
    #[serde(default)]
    pub usage_total: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _partial_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowupTurn {
    pub turn: u32,
    pub ts_ms: u64,
    pub user: String,
    pub ai_content: String,
    pub ai_reasoning: Option<String>,
    pub usage: Value,
    #[serde(default)]
    pub cancelled: bool,
}

/// 程序侧决策节点填充结果（NodeFill）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeFill {
    pub node_id: String,
    pub answer: &'static str,
    pub reason: String,
    pub bar_range: String,
    pub branch: Option<String>,
    pub section: Option<&'static str>,
}

impl NodeFill {
    pub fn to_trace_value(&self, question: &str) -> Value {
        serde_json::json!({
            "node_id": self.node_id,
            "question": question,
            "answer": self.answer,
            "reason": self.reason,
            "branch": self.branch,
            "bar_range": self.bar_range,
            "skipped": false,
            "section": self.section,
        })
    }
}

/// 市场周期枚举（与 Python CYCLE_ORDER 对齐）。
pub const CYCLE_ORDER: &[&str] = &[
    "spike",
    "micro_channel",
    "tight_channel",
    "normal_channel",
    "broad_channel",
    "trending_tr",
    "trading_range",
    "extreme_tr",
];

pub const CYCLE_ZH: &[(&str, &str)] = &[
    ("spike", "尖峰"),
    ("micro_channel", "微型通道"),
    ("tight_channel", "窄通道"),
    ("normal_channel", "正常通道"),
    ("broad_channel", "宽通道"),
    ("trending_tr", "趋势型交易区间"),
    ("trading_range", "交易区间"),
    ("extreme_tr", "极端交易区间"),
    ("unknown", "未知"),
];

pub fn cycle_zh(cycle: &str) -> &'static str {
    CYCLE_ZH
        .iter()
        .find(|(key, _)| *key == cycle)
        .map(|(_, zh)| *zh)
        .unwrap_or("未知")
}

/// timeframe → 秒。
pub fn timeframe_to_seconds(tf: &str) -> Option<u64> {
    let tf = tf.trim();
    let digits: String = tf.chars().take_while(|c| c.is_ascii_digit()).collect();
    let unit = tf[digits.len()..].trim();
    let value: u64 = digits.parse().ok()?;
    let seconds = match unit {
        "m" | "min" => value * 60,
        "h" => value * 3600,
        "d" => value * 86400,
        "w" => value * 604800,
        "M" | "mo" => value * 2592000,
        _ => return None,
    };
    Some(seconds)
}

/// 距离当前棒收盘剩余秒数（与 Python seconds_until_bar_closes 一致，用取模对时钟漂移鲁棒）。
pub fn seconds_until_bar_closes(ts_open_ms: f64, timeframe: &str, now_ms: u64) -> Option<u64> {
    let duration_s = timeframe_to_seconds(timeframe)?;
    if duration_s == 0 {
        return None;
    }
    let duration_ms = (duration_s * 1000) as f64;
    let elapsed = now_ms as f64 - ts_open_ms;
    if elapsed < 0.0 {
        return Some(duration_s);
    }
    let remainder = elapsed % duration_ms;
    if remainder <= f64::EPSILON {
        return Some(0);
    }
    Some(((duration_ms - remainder) / 1000.0).ceil() as u64)
}

/// 时间帧归一化（数据源各自支持的时间帧集合校验用）。
pub fn normalize_timeframe(tf: &str) -> String {
    tf.trim().to_string()
}
