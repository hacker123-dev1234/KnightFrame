//! JSON 校验器与归一化器（移植自 ai/json_validator.py + stage1/2_normalizer.py + trade_metrics.py）。
//! 校验分类：a=语法 / b=缺字段 / c=值非法或一致性失败 / d=纯文本。

use super::decision_nodes;
use super::decision_tree::{
    normalize_bar_range, validate_gate_result_consistency, validate_stage2_trace_consistency,
};
use super::features::infer_price_tick;
use super::router;
use super::types::{CYCLE_ORDER, KlineFrame, MarketSettings};
use serde_json::{Map, Value, json};

pub const MIN_RISK_REWARD_RATIO: f64 = 1.0;
pub const MAX_RISK_REWARD_RATIO: f64 = 1.5;

const STAGE1_REQUIRED: &[&str] = &[
    "cycle_position",
    "direction",
    "diagnosis_confidence",
    "market_phase",
    "detected_patterns",
    "key_signals",
    "htf_context",
    "entry_setup",
    "strategy_files_needed",
    "bar_by_bar_summary",
    "gate_trace",
    "gate_result",
];
const STAGE2_REQUIRED: &[&str] = &[
    "decision",
    "diagnosis_summary",
    "decision_trace",
    "terminal",
    "next_bar_prediction",
    "next_cycle_prediction",
];
const CYCLE_ENUMS: &[&str] = &[
    "spike",
    "micro_channel",
    "tight_channel",
    "normal_channel",
    "broad_channel",
    "trending_tr",
    "trading_range",
    "extreme_tr",
    "unknown",
];
const DIRECTIONS: &[&str] = &["bullish", "bearish", "neutral"];
const GATE_RESULTS: &[&str] = &["proceed", "wait", "unknown"];
const ORDER_TYPES: &[&str] = &["限价单", "突破单", "市价单", "不下单"];
const PREDICTION_FEATURES: &[&str] = &[
    "stage1_diagnosis",
    "kline_features",
    "analysis_history",
    "experience_library",
    "stage2_decision",
    "previous_prediction_summary",
];

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub category: char,
    pub message: String,
    pub missing_fields: Vec<String>,
    pub invalid_fields: Vec<String>,
}

impl ValidationError {
    fn syntax(message: impl Into<String>) -> Self {
        Self {
            category: 'a',
            message: message.into(),
            missing_fields: vec![],
            invalid_fields: vec![],
        }
    }
    fn text(message: impl Into<String>) -> Self {
        Self {
            category: 'd',
            message: message.into(),
            missing_fields: vec![],
            invalid_fields: vec![],
        }
    }
    fn missing(fields: Vec<String>) -> Self {
        Self {
            category: 'b',
            message: format!("缺少必填字段：{}", fields.join(", ")),
            missing_fields: fields,
            invalid_fields: vec![],
        }
    }
    fn invalid(fields: Vec<String>, message: impl Into<String>) -> Self {
        Self {
            category: 'c',
            message: message.into(),
            missing_fields: vec![],
            invalid_fields: fields,
        }
    }
}

// ---------------------------------------------------------------------------
// 文本清理与修复
// ---------------------------------------------------------------------------

pub fn strip_fences(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    // 剥离 markdown 围栏
    if text.starts_with("```") {
        text = text
            .trim_start_matches("```json")
            .trim_start_matches("```JSON")
            .trim_start_matches("```")
            .to_string();
        if let Some(end) = text.rfind("```") {
            text.truncate(end);
        }
    }
    // 智能引号 → ASCII
    text = text
        .replace(['“', '”'], "\"")
        .replace(['‘', '’'], "'")
        .replace('，', ", ");
    // 提取首个顶层 {...}
    if let Some(start) = text.find('{')
        && let Some(end) = text.rfind('}')
        && end > start
    {
        text = text[start..=end].to_string();
    }
    text
}

/// 截断修复：补全未闭合的引号/括号。
pub fn repair_truncated(text: &str) -> String {
    let mut repaired = text.trim().to_string();
    let mut in_string = false;
    let mut escape = false;
    let mut stack: Vec<char> = Vec::new();
    for ch in repaired.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '{' | '[' if !in_string => stack.push(ch),
            '}' | ']' if !in_string => {
                let closing = ch;
                if let Some(open) = stack.last() {
                    let matched =
                        (*open == '{' && closing == '}') || (*open == '[' && closing == ']');
                    if matched {
                        stack.pop();
                    }
                }
            }
            _ => {}
        }
    }
    if in_string {
        repaired.push('"');
    }
    // 尾部清理悬挂逗号
    let trimmed = repaired.trim_end();
    if let Some(stripped) = trimmed.strip_suffix(',') {
        repaired = stripped.to_string();
    }
    while let Some(open) = stack.pop() {
        repaired.push(if open == '{' { '}' } else { ']' });
    }
    repaired
}

fn parse_json_object(
    text: &str,
    allow_repair: bool,
) -> Result<Map<String, Value>, ValidationError> {
    if !text.trim_start().starts_with('{') {
        return Err(ValidationError::text("输出不是 JSON 对象（纯文本）"));
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(ValidationError::text("顶层不是 JSON 对象")),
        Err(error) => {
            if allow_repair {
                let repaired = repair_truncated(text);
                if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&repaired) {
                    return Ok(map);
                }
            }
            Err(ValidationError::syntax(format!("JSON 语法错误：{error}")))
        }
    }
}

// ---------------------------------------------------------------------------
// 盈亏比与交易方程
// ---------------------------------------------------------------------------

pub struct RiskReward {
    pub risk: f64,
    pub reward: f64,
    pub ratio: f64,
}

pub fn compute_risk_reward(
    entry: f64,
    take_profit: f64,
    stop_loss: f64,
    direction: Option<&str>,
) -> Option<RiskReward> {
    let long = match direction {
        Some("做多") => true,
        Some("做空") => false,
        _ => {
            if take_profit > entry && stop_loss < entry {
                true
            } else if take_profit < entry && stop_loss > entry {
                false
            } else {
                return None;
            }
        }
    };
    let (risk, reward) = if long {
        (entry - stop_loss, take_profit - entry)
    } else {
        (stop_loss - entry, entry - take_profit)
    };
    if risk <= 0.0 || reward <= 0.0 {
        return None;
    }
    Some(RiskReward {
        risk,
        reward,
        ratio: reward / risk,
    })
}

pub fn passes_trader_equation(win_rate_pct: f64, risk: f64, reward: f64) -> bool {
    let p = (win_rate_pct.clamp(0.0, 100.0)) / 100.0;
    p * reward > (1.0 - p) * risk
}

#[allow(dead_code)] // PA 全量移植保留：铁律 1 校验辅助（与 Python 版对齐）
fn order_is_no_order(decision: &Map<String, Value>) -> bool {
    decision.get("order_type").and_then(Value::as_str) == Some("不下单")
}

fn f64_of(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|v| v.is_finite())
}

// ---------------------------------------------------------------------------
// 预测归一化
// ---------------------------------------------------------------------------

fn normalize_prediction_keys(
    probabilities: &Map<String, Value>,
    keys: &[&str],
) -> Map<String, Value> {
    let mut result = Map::new();
    for key in keys {
        let value = probabilities
            .get(*key)
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 100.0)
            .round();
        result.insert((*key).to_string(), json!(value as u64));
    }
    // 重缩放使和 = 100
    let total: f64 = result.values().filter_map(Value::as_f64).sum();
    if total > 0.0 && (total - 100.0).abs() > 1.5 {
        let scale = 100.0 / total;
        for (_, value) in result.iter_mut() {
            if let Some(number) = value.as_f64() {
                *value = json!((number * scale).round() as u64);
            }
        }
        let new_total: f64 = result.values().filter_map(Value::as_f64).sum();
        let remainder = (100.0 - new_total).round() as i64;
        if remainder != 0 {
            let max_key = result
                .iter()
                .max_by(|a, b| {
                    a.1.as_f64()
                        .unwrap_or(0.0)
                        .total_cmp(&b.1.as_f64().unwrap_or(0.0))
                })
                .map(|(k, _)| k.clone());
            if let Some(max_key) = max_key
                && let Some(current) = result.get(&max_key).and_then(Value::as_i64)
            {
                let adjusted = (current + remainder).max(0);
                result.insert(max_key, json!(adjusted));
            }
        }
    }
    result
}

fn argmax_key(probabilities: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .max_by(|a, b| {
            let pa = probabilities
                .get(**a)
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let pb = probabilities
                .get(**b)
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            // 概率大者胜；并列时键序靠前者胜（max_by 相等取最后，
            // 因此并列比较反转键序，让靠前的键被视为更大）。
            pa.total_cmp(&pb).then(b.cmp(a))
        })
        .map(|key| (*key).to_string())
}

fn normalize_features_used(value: &mut Value) {
    let features = value
        .get("features_used")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut seen = std::collections::BTreeSet::new();
    let filtered: Vec<Value> = features
        .into_iter()
        .filter_map(|feature| feature.as_str().map(str::to_owned))
        .filter(|feature| PREDICTION_FEATURES.contains(&feature.as_str()))
        .filter(|feature| seen.insert(feature.clone()))
        .map(|feature| json!(feature))
        .collect();
    value["features_used"] = json!(if filtered.is_empty() {
        vec![json!("stage1_diagnosis")]
    } else {
        filtered
    });
    if let Some(reasoning) = value
        .get("reasoning")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        let truncated: String = reasoning.chars().take(1500).collect();
        value["reasoning"] = json!(truncated);
    }
}

pub fn normalize_next_bar_prediction(prediction: &mut Value) {
    if prediction.is_null() {
        *prediction = json!({
            "direction": Value::Null, "probabilities": Value::Null,
            "reasoning": "未提供预测。", "unpredictable": true,
            "features_used": ["stage1_diagnosis"],
        });
        return;
    }
    let unpredictable = prediction
        .get("unpredictable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if unpredictable {
        prediction["direction"] = Value::Null;
        prediction["probabilities"] = Value::Null;
        normalize_features_used(prediction);
        return;
    }
    let keys = ["bullish", "bearish", "neutral"];
    if let Some(probabilities) = prediction.get("probabilities").and_then(Value::as_object) {
        let normalized = normalize_prediction_keys(probabilities, &keys);
        prediction["probabilities"] = Value::Object(normalized.clone());
        let direction = argmax_key(&normalized, &keys);
        prediction["direction"] = direction.map(Value::String).unwrap_or(Value::Null);
    } else {
        prediction["probabilities"] = Value::Null;
        prediction["direction"] = Value::Null;
        prediction["unpredictable"] = json!(true);
    }
    normalize_features_used(prediction);
}

pub fn normalize_next_cycle_prediction(prediction: &mut Value) {
    if prediction.is_null() {
        *prediction = json!({
            "cycle": Value::Null, "direction": Value::Null, "probabilities": Value::Null,
            "reasoning": "未提供预测。", "unpredictable": true,
            "features_used": ["stage1_diagnosis"],
        });
        return;
    }
    let unpredictable = prediction
        .get("unpredictable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if unpredictable {
        prediction["cycle"] = Value::Null;
        prediction["direction"] = Value::Null;
        prediction["probabilities"] = Value::Null;
        normalize_features_used(prediction);
        return;
    }
    let keys = CYCLE_ORDER.to_vec();
    if let Some(probabilities) = prediction.get("probabilities").and_then(Value::as_object) {
        let normalized = normalize_prediction_keys(probabilities, &keys);
        prediction["probabilities"] = Value::Object(normalized.clone());
        let cycle = argmax_key(&normalized, &keys);
        prediction["cycle"] = cycle.map(Value::String).unwrap_or(Value::Null);
    } else {
        prediction["probabilities"] = Value::Null;
        prediction["cycle"] = Value::Null;
        prediction["unpredictable"] = json!(true);
    }
    normalize_features_used(prediction);
}

// ---------------------------------------------------------------------------
// 阶段一归一化
// ---------------------------------------------------------------------------

const GENERIC_ANSWER_ALIASES: &[(&str, &str)] = &[
    ("yes", "是"),
    ("no", "否"),
    ("true", "是"),
    ("false", "否"),
    ("多头", "是"),
    ("bullish", "是"),
    ("bearish", "是"),
    ("通过", "是"),
    ("部分", "中性"),
    ("partial", "中性"),
    ("neutral", "中性"),
    ("待定", "等待"),
    ("pending", "等待"),
    ("n/a", "不适用"),
    ("none", "不适用"),
];

fn normalize_answer(answer: &str) -> &str {
    let trimmed = answer.trim();
    for (alias, canonical) in GENERIC_ANSWER_ALIASES {
        if trimmed.eq_ignore_ascii_case(alias) || trimmed == *alias {
            return canonical;
        }
    }
    trimmed
}

pub fn normalize_stage1(mut out: Value, frame: &KlineFrame) -> Value {
    // 解包 {"stage1_diagnosis": {...}}
    if let Some(inner) = out.get("stage1_diagnosis").filter(|v| v.is_object()) {
        let inner = inner.clone();
        out = inner;
    }
    // 程序决策引擎
    decision_nodes::apply_stage1(&mut out, frame);
    // 策略文件缺失时路由补全
    let needs_route = out
        .get("strategy_files_needed")
        .and_then(Value::as_array)
        .is_none_or(|files| files.is_empty());
    if needs_route {
        out["strategy_files_needed"] = json!(router::route_strategy_files(&out));
    }
    // trace 归一化
    if let Some(trace) = out.get_mut("gate_trace").and_then(Value::as_array_mut) {
        let max_seq = frame.bars.len() as u32;
        for item in trace.iter_mut() {
            if let Some(answer) = item.get("answer").and_then(Value::as_str) {
                item["answer"] = json!(normalize_answer(answer));
            }
            let node_id = item
                .get("node_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if node_id == "14" {
                item["node_id"] = json!("14.1");
            }
            normalize_bar_range(item, max_seq);
        }
        trace.sort_by(|a, b| {
            let ka = a
                .get("node_id")
                .and_then(Value::as_str)
                .map(super::decision_tree::node_sort_key);
            let kb = b
                .get("node_id")
                .and_then(Value::as_str)
                .map(super::decision_tree::node_sort_key);
            ka.cmp(&kb)
        });
    }
    // bar_by_bar_summary 至少 1 条
    if out
        .get("bar_by_bar_summary")
        .and_then(Value::as_array)
        .is_none_or(|items| items.is_empty())
    {
        out["bar_by_bar_summary"] = json!([{
            "bar": "K1",
            "role": "structure",
            "bar_type": "other",
            "context_effect": "neutral",
            "follow_through": "pending",
            "trapped_side": "none",
            "reason": "模型未提供逐棒摘要",
        }]);
    }
    out
}

// ---------------------------------------------------------------------------
// 阶段二归一化
// ---------------------------------------------------------------------------

fn alias_map(value: Option<&str>, aliases: &[(&str, &str)]) -> Option<String> {
    let raw = value?;
    for (alias, canonical) in aliases {
        if raw.trim() == *alias || raw.trim().to_lowercase() == alias.to_lowercase() {
            return Some((*canonical).to_string());
        }
    }
    Some(raw.trim().to_string())
}

pub fn normalize_stage2(mut out: Value, frame: &KlineFrame, stage1: &Value) -> Value {
    // 枚举别名
    if let Some(decision) = out.get_mut("decision").and_then(Value::as_object_mut) {
        if let Some(order_type) = decision.get("order_type").and_then(Value::as_str) {
            let mapped = alias_map(
                Some(order_type),
                &[
                    ("limit", "限价单"),
                    ("limit_order", "限价单"),
                    ("限价", "限价单"),
                    ("breakout", "突破单"),
                    ("breakout_order", "突破单"),
                    ("突破", "突破单"),
                    ("market", "市价单"),
                    ("market_order", "市价单"),
                    ("市价", "市价单"),
                    ("none", "不下单"),
                    ("no_order", "不下单"),
                    ("no_trade", "不下单"),
                    ("不交易", "不下单"),
                ],
            );
            if let Some(mapped) = mapped {
                decision.insert("order_type".into(), json!(mapped));
            }
        }
        if let Some(direction) = decision.get("order_direction").and_then(Value::as_str) {
            let mapped = alias_map(
                Some(direction),
                &[
                    ("long", "做多"),
                    ("buy", "做多"),
                    ("多", "做多"),
                    ("bull", "做多"),
                    ("short", "做空"),
                    ("sell", "做空"),
                    ("空", "做空"),
                    ("bear", "做空"),
                ],
            );
            if let Some(mapped) = mapped {
                decision.insert("order_direction".into(), json!(mapped));
            }
        }
    }
    if let Some(terminal) = out.get_mut("terminal").and_then(Value::as_object_mut)
        && let Some(outcome) = terminal.get("outcome").and_then(Value::as_str)
    {
        let mapped = alias_map(
            Some(outcome),
            &[
                ("wait", "wait"),
                ("hold", "wait"),
                ("等待", "wait"),
                ("reject", "reject"),
                ("放弃", "reject"),
                ("trade", "trade"),
                ("下单", "trade"),
                ("proceed", "proceed"),
                ("继续", "proceed"),
            ],
        );
        if let Some(mapped) = mapped {
            terminal.insert("outcome".into(), json!(mapped));
        }
    }

    // 强制不下单情形：§14 触犯 / terminal=wait,reject 且无方案
    let violation_14 = out
        .get("decision_trace")
        .and_then(Value::as_array)
        .map(|trace| {
            trace.iter().any(|item| {
                let id = item.get("node_id").and_then(Value::as_str).unwrap_or("");
                (id == "14" || id == "14.1")
                    && item.get("answer").and_then(Value::as_str) == Some("是")
            })
        })
        .unwrap_or(false);
    let outcome = out
        .pointer("/terminal/outcome")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if violation_14 || outcome == "wait" {
        coerce_no_order(&mut out);
    }

    // 突破单修正：basis_extreme 按方向、entry_price 用极点 ± tick 重算
    let tick = infer_price_tick(frame);
    let order_type = out
        .pointer("/decision/order_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if order_type == "突破单" {
        let direction = out
            .pointer("/decision/order_direction")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let basis_label = out
            .pointer("/decision/entry_basis_bar")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(index) = frame.bars.iter().position(|bar| {
            format!("K{}", bar.seq) == basis_label || basis_label == format!("{}", bar.seq)
        }) {
            let bar = &frame.bars[index];
            let (extreme, entry) = if direction == "做空" {
                ("low", (bar.low - tick).round_to_tick_friendly(tick))
            } else {
                ("high", (bar.high + tick).round_to_tick_friendly(tick))
            };
            if let Some(decision) = out.get_mut("decision").and_then(Value::as_object_mut) {
                decision.insert("entry_basis_extreme".into(), json!(extreme));
                decision.insert("entry_price".into(), json!(entry));
            }
        }
    }

    // 程序决策引擎（§9.1-9.5、§11 路由）
    decision_nodes::apply_stage2(&mut out, frame, stage1);

    // 盈亏比不达标 → 强制不下单
    if let Some(decision) = out.get("decision").and_then(Value::as_object) {
        let order_type = decision
            .get("order_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if order_type != "不下单" {
            let entry = f64_of(decision.get("entry_price"));
            let tp = f64_of(decision.get("take_profit_price"));
            let sl = f64_of(decision.get("stop_loss_price"));
            let direction = decision.get("order_direction").and_then(Value::as_str);
            let win_rate = f64_of(decision.get("estimated_win_rate"));
            let metrics_ok = match (entry, tp, sl) {
                (Some(entry), Some(tp), Some(sl)) => {
                    match compute_risk_reward(entry, tp, sl, direction) {
                        Some(metrics) => {
                            let equation = win_rate
                                .map(|rate| {
                                    passes_trader_equation(rate, metrics.risk, metrics.reward)
                                })
                                .unwrap_or(false);
                            metrics.ratio >= MIN_RISK_REWARD_RATIO
                                && metrics.ratio <= MAX_RISK_REWARD_RATIO
                                && equation
                        }
                        None => false,
                    }
                }
                _ => false,
            };
            if !metrics_ok {
                coerce_no_order(&mut out);
                if let Some(trace) = out.get_mut("decision_trace").and_then(Value::as_array_mut) {
                    for item in trace.iter_mut() {
                        if item.get("node_id").and_then(Value::as_str) == Some("10.3") {
                            item["answer"] = json!("否");
                            item["reason"] = json!("盈亏比或交易者方程不达标，程序强制不下单");
                        }
                    }
                }
            }
        }
    }

    // 不下单时清空价格字段
    if out.pointer("/decision/order_type").and_then(Value::as_str) == Some("不下单")
        && let Some(decision) = out.get_mut("decision").and_then(Value::as_object_mut)
    {
        for field in [
            "order_direction",
            "entry_price",
            "entry_basis_bar",
            "entry_basis_extreme",
            "entry_rule",
            "take_profit_price",
            "stop_loss_price",
            "estimated_win_rate",
        ] {
            decision.insert(field.into(), Value::Null);
        }
        if decision.get("trade_confidence").is_none() {
            decision.insert("trade_confidence".into(), json!(0));
        }
    }

    // 信号棒/入场棒链：signal seq 必须 > entry seq
    if let Some(bar_analysis) = out.get_mut("bar_analysis").and_then(Value::as_object_mut) {
        let parse_seq = |value: Option<&Value>| -> Option<u32> {
            value
                .and_then(Value::as_str)
                .and_then(|label| label.replace(['K', 'k'], "").trim().parse().ok())
        };
        let signal_seq = parse_seq(bar_analysis.get("signal_bar"));
        let entry_seq = parse_seq(bar_analysis.get("entry_bar"));
        if let (Some(signal), Some(entry)) = (signal_seq, entry_seq)
            && signal <= entry
            && let Some(entry_label) = entry.checked_sub(1).filter(|v| *v >= 1)
        {
            bar_analysis.insert("signal_bar".into(), json!(format!("K{entry_label}")));
        }
    }

    // trace 归一化
    if let Some(trace) = out.get_mut("decision_trace").and_then(Value::as_array_mut) {
        let max_seq = frame.bars.len() as u32;
        for item in trace.iter_mut() {
            if let Some(answer) = item.get("answer").and_then(Value::as_str) {
                item["answer"] = json!(normalize_answer(answer));
            }
            normalize_bar_range(item, max_seq);
        }
        trace.sort_by(|a, b| {
            let ka = a
                .get("node_id")
                .and_then(Value::as_str)
                .map(super::decision_tree::node_sort_key);
            let kb = b
                .get("node_id")
                .and_then(Value::as_str)
                .map(super::decision_tree::node_sort_key);
            ka.cmp(&kb)
        });
    }

    // 预测归一化
    if let Some(prediction) = out.get_mut("next_bar_prediction") {
        normalize_next_bar_prediction(prediction);
    }
    if let Some(prediction) = out.get_mut("next_cycle_prediction") {
        normalize_next_cycle_prediction(prediction);
    }
    out
}

trait RoundTick {
    fn round_to_tick_friendly(self, tick: f64) -> f64;
}

impl RoundTick for f64 {
    fn round_to_tick_friendly(self, tick: f64) -> f64 {
        if tick <= 0.0 {
            return self;
        }
        (self / tick).round() * tick
    }
}

fn coerce_no_order(out: &mut Value) {
    if let Some(decision) = out.get_mut("decision").and_then(Value::as_object_mut) {
        decision.insert("order_type".into(), json!("不下单"));
    }
}

// ---------------------------------------------------------------------------
// 校验入口
// ---------------------------------------------------------------------------

fn check_enum(errors: &mut Vec<String>, pointer: &str, value: Option<&str>, allowed: &[&str]) {
    match value {
        Some(value) if allowed.contains(&value) => {}
        Some(value) => errors.push(format!("s1:{pointer} 非法值 {value:?}（允许 {allowed:?}）")),
        None => errors.push(format!("s1:{pointer} 缺失")),
    }
}

pub fn validate_stage1(
    raw: &str,
    frame: &KlineFrame,
    settings: &MarketSettings,
) -> Result<Value, ValidationError> {
    let cleaned = strip_fences(raw);
    let allow_repair = !settings.validation.disable_truncation_repair;
    let map = parse_json_object(&cleaned, allow_repair)?;
    let mut value = Value::Object(map);
    value = normalize_stage1(value, frame);

    let mut missing = Vec::new();
    for field in STAGE1_REQUIRED {
        if value.get(*field).is_none() {
            missing.push((*field).to_string());
        }
    }
    if !missing.is_empty() {
        return Err(ValidationError::missing(missing));
    }
    let mut invalid = Vec::new();
    check_enum(
        &mut invalid,
        "cycle_position",
        value.get("cycle_position").and_then(Value::as_str),
        CYCLE_ENUMS,
    );
    check_enum(
        &mut invalid,
        "direction",
        value.get("direction").and_then(Value::as_str),
        DIRECTIONS,
    );
    check_enum(
        &mut invalid,
        "gate_result",
        value.get("gate_result").and_then(Value::as_str),
        GATE_RESULTS,
    );
    if let Some(confidence) = value.get("diagnosis_confidence") {
        let ok = confidence
            .as_f64()
            .or_else(|| confidence.as_i64().map(|v| v as f64))
            .is_some_and(|v| (0.0..=100.0).contains(&v));
        if !ok {
            invalid.push("s1:diagnosis_confidence 超出 0-100".into());
        }
    }
    let gate_errors = validate_gate_result_consistency(&value);
    invalid.extend(gate_errors);
    if settings.validation.stage1_coherence_checks {
        // 一致性检查（默认关闭）
    }
    if !invalid.is_empty() {
        return Err(ValidationError::invalid(invalid, "阶段一字段校验失败"));
    }
    Ok(value)
}

pub fn validate_stage2(
    raw: &str,
    frame: &KlineFrame,
    stage1: &Value,
    settings: &MarketSettings,
) -> Result<Value, ValidationError> {
    let cleaned = strip_fences(raw);
    let allow_repair = !settings.validation.disable_truncation_repair;
    let map = parse_json_object(&cleaned, allow_repair)?;
    let mut value = Value::Object(map);
    value = normalize_stage2(value, frame, stage1);

    let mut missing = Vec::new();
    for field in STAGE2_REQUIRED {
        if value.get(*field).is_none() {
            missing.push((*field).to_string());
        }
    }
    if !missing.is_empty() {
        return Err(ValidationError::missing(missing));
    }

    let mut invalid = Vec::new();
    let order_type = value
        .pointer("/decision/order_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if !ORDER_TYPES.contains(&order_type.as_str()) {
        invalid.push(format!("s2:order_type 非法值 {order_type:?}"));
    }
    if order_type == "不下单" {
        // 不下单铁律：全 null
        for field in [
            "entry_price",
            "take_profit_price",
            "stop_loss_price",
            "order_direction",
            "estimated_win_rate",
            "entry_basis_bar",
            "entry_basis_extreme",
            "entry_rule",
        ] {
            if value
                .pointer(&format!("/decision/{field}"))
                .is_some_and(|v| !v.is_null())
            {
                invalid.push(format!("s2:不下单时 decision.{field} 必须为 null"));
            }
        }
    } else {
        for field in [
            "entry_price",
            "take_profit_price",
            "stop_loss_price",
            "order_direction",
            "estimated_win_rate",
        ] {
            if value
                .pointer(&format!("/decision/{field}"))
                .is_none_or(Value::is_null)
            {
                invalid.push(format!("s2:有下单时 decision.{field} 必填"));
            }
        }
        if order_type == "突破单" {
            for field in ["entry_basis_bar", "entry_basis_extreme", "entry_rule"] {
                if value
                    .pointer(&format!("/decision/{field}"))
                    .is_none_or(Value::is_null)
                {
                    invalid.push(format!("s2:突破单 decision.{field} 必填"));
                }
            }
            let direction = value
                .pointer("/decision/order_direction")
                .and_then(Value::as_str);
            let expected = match direction {
                Some("做多") => "high",
                Some("做空") => "low",
                _ => "",
            };
            let actual = value
                .pointer("/decision/entry_basis_extreme")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !expected.is_empty() && actual != expected {
                invalid.push(format!(
                    "s2:突破单 basis_extreme 应为 {expected}，实际 {actual}"
                ));
            }
        }
        // 盈亏比与交易方程（归一化已强制不下单，此处兜底）
        let entry = f64_of(value.pointer("/decision/entry_price"));
        let tp = f64_of(value.pointer("/decision/take_profit_price"));
        let sl = f64_of(value.pointer("/decision/stop_loss_price"));
        let direction = value
            .pointer("/decision/order_direction")
            .and_then(Value::as_str);
        if let (Some(entry), Some(tp), Some(sl)) = (entry, tp, sl) {
            match compute_risk_reward(entry, tp, sl, direction) {
                Some(metrics) => {
                    if metrics.ratio < MIN_RISK_REWARD_RATIO
                        || metrics.ratio > MAX_RISK_REWARD_RATIO
                    {
                        invalid.push(format!(
                            "metrics:盈亏比 {:.2} 超出 [{MIN_RISK_REWARD_RATIO},{MAX_RISK_REWARD_RATIO}]",
                            metrics.ratio
                        ));
                    }
                }
                None => invalid.push("metrics:止损止盈相对入场价方向非法".into()),
            }
        }
        // 限价单 K1 新鲜度
        if order_type == "限价单" {
            let tick = infer_price_tick(frame);
            if let (Some(entry), Some(sl), Some(k1)) = (entry, sl, frame.bars.first()) {
                let long = direction == Some("做多") || (tp.unwrap_or(entry) > entry && sl < entry);
                let stale = if long {
                    k1.low <= entry + tick || k1.low <= sl + tick
                } else {
                    k1.high >= entry - tick || k1.high >= sl - tick
                };
                if stale {
                    invalid.push("breakout_price:限价单已被 K1 穿越，方案失效".into());
                }
            }
        }
    }
    let trace_errors = validate_stage2_trace_consistency(&value);
    invalid.extend(trace_errors);
    if !invalid.is_empty() {
        return Err(ValidationError::invalid(invalid, "阶段二字段校验失败"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::indicators::compute_indicators;
    use crate::market::types::{KlineBar, KlineFrame as Frame};

    fn frame() -> Frame {
        let bars: Vec<KlineBar> = (0..60)
            .rev()
            .map(|i| KlineBar {
                seq: 0,
                ts_open: 1_700_000_000_000.0 + i as f64 * 900_000.0,
                open: 100.0 + i as f64,
                high: 102.0 + i as f64,
                low: 99.0 + i as f64,
                close: 101.0 + i as f64,
                volume: 10.0,
                closed: true,
            })
            .collect();
        let mut bars = bars;
        for (index, bar) in bars.iter_mut().enumerate() {
            bar.seq = (index + 1) as u32;
        }
        let indicators = compute_indicators(&bars);
        Frame {
            symbol: "T".into(),
            timeframe: "15m".into(),
            bars,
            indicators,
            snapshot_ts_local_ms: 1,
        }
    }

    fn settings() -> MarketSettings {
        MarketSettings::default()
    }

    #[test]
    fn strips_markdown_fences_and_smart_quotes() {
        let raw = "```json\n{\"a\": “x”}\n```";
        assert_eq!(strip_fences(raw), "{\"a\": \"x\"}");
    }

    #[test]
    fn repairs_truncated_json() {
        let truncated = "{\"a\": {\"b\": [1, 2";
        let repaired = repair_truncated(truncated);
        assert!(serde_json::from_str::<Value>(&repaired).is_ok());
    }

    #[test]
    fn stage1_validates_required_enums() {
        let raw = r#"{"cycle_position": "broad_channel", "direction": "bullish",
            "diagnosis_confidence": 80, "market_phase": "stable",
            "detected_patterns": [], "key_signals": [], "htf_context": "x",
            "entry_setup": "y", "strategy_files_needed": [], "bar_by_bar_summary": [],
            "gate_trace": [{"node_id": "1.1", "answer": "是", "reason": "ok"}],
            "gate_result": "proceed"}"#;
        let result = validate_stage1(raw, &frame(), &settings());
        assert!(result.is_ok(), "{result:?}");
        let value = result.unwrap();
        // 程序节点被合并进 trace
        let trace = value["gate_trace"].as_array().unwrap();
        assert!(trace.iter().any(|item| item["node_id"] == "2.3"));
    }

    #[test]
    fn stage1_rejects_bad_cycle() {
        let raw = r#"{"cycle_position": "moon", "direction": "bullish",
            "diagnosis_confidence": 80, "market_phase": "stable",
            "detected_patterns": [], "key_signals": [], "htf_context": "x",
            "entry_setup": "y", "strategy_files_needed": [], "bar_by_bar_summary": [],
            "gate_trace": [{"node_id": "1.1", "answer": "是", "reason": "ok"}],
            "gate_result": "proceed"}"#;
        let error = validate_stage1(raw, &frame(), &settings()).unwrap_err();
        assert_eq!(error.category, 'c');
    }

    #[test]
    fn no_order_invariant_enforced() {
        let raw = r#"{"decision": {"order_type": "不下单", "reasoning": "x",
            "diagnosis_confidence": 50, "trade_confidence": 0,
            "entry_price": 123.0, "key_factors": [], "watch_points": [], "risk_assessment": "r",
            "diagnosis_confidence_reasoning": "d", "trade_confidence_reasoning": "t",
            "estimated_win_rate_reasoning": "w"},
            "diagnosis_summary": {"cycle_position": "trading_range", "direction": "neutral", "key_signals": []},
            "decision_trace": [], "terminal": {"node_id": "10.3", "outcome": "wait", "label": "w"},
            "next_bar_prediction": {"unpredictable": true, "reasoning": "r"},
            "next_cycle_prediction": {"unpredictable": true, "reasoning": "r"}}"#;
        let stage1 = serde_json::json!({"cycle_position": "trading_range", "direction": "neutral"});
        // 归一化采用修复策略：不下单时的价格/方向字段被清空而非拒绝。
        let value = validate_stage2(raw, &frame(), &stage1, &settings())
            .expect("lenient normalization repairs the no-order invariant");
        for field in [
            "entry_price",
            "take_profit_price",
            "stop_loss_price",
            "order_direction",
        ] {
            assert!(
                value
                    .pointer(&format!("/decision/{field}"))
                    .is_none_or(Value::is_null),
                "decision.{field} must be null after no-order repair"
            );
        }
    }

    #[test]
    fn prediction_probabilities_rescaled_and_argmax() {
        let mut prediction = json!({
            "direction": "bearish",
            "probabilities": {"bullish": 60.4, "bearish": 50.2, "neutral": 10.1},
            "reasoning": "r", "unpredictable": false, "features_used": ["kline_features", "made_up", "kline_features"],
        });
        normalize_next_bar_prediction(&mut prediction);
        let probabilities = prediction["probabilities"].as_object().unwrap();
        let total: u64 = probabilities.values().filter_map(Value::as_u64).sum();
        assert!((99..=101).contains(&total));
        assert_eq!(prediction["direction"], "bullish");
        let features = prediction["features_used"].as_array().unwrap();
        assert_eq!(features.len(), 1);
    }
}
