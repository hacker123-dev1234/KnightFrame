//! 校验重试策略（移植自 ai/retry_policy.py + retry_feedback.py）。
//! a/b/d 类重试 retry_max（3）次；c 类按字段前缀判定；不可变字段篡改判定为 cheat。

use super::types::{KlineFrame, ValidationSettings};
use super::validator::ValidationError;
use serde_json::Value;

pub fn max_retries_for_category(category: char, settings: &ValidationSettings) -> u32 {
    match category {
        'c' => settings.retry_max_semantic.min(settings.retry_max),
        _ => settings.retry_max,
    }
}

const FORMAT_PREFIXES: &[&str] = &[
    "gate_trace",
    "decision_trace",
    "bar_by_bar_summary",
    "bar_range",
    "incremental",
    "next_bar",
    "next_cycle",
];
const NO_RETRY_PREFIXES: &[&str] = &["metrics:", "trace:§14", "s2:order_direction"];
const SEMANTIC_PREFIXES: &[&str] = &[
    "s1:",
    "s2:",
    "gate:",
    "trace:",
    "breakout_price:",
    "signal_chain:",
];

pub fn should_retry(error: &ValidationError, attempt: u32, settings: &ValidationSettings) -> bool {
    if !settings.retry_enabled {
        return false;
    }
    let max = max_retries_for_category(error.category, settings);
    if attempt >= max {
        return false;
    }
    if error.category != 'c' {
        return true;
    }
    let fields: Vec<&str> = error
        .invalid_fields
        .iter()
        .map(String::as_str)
        .chain(error.missing_fields.iter().map(String::as_str))
        .collect();
    for prefix in NO_RETRY_PREFIXES {
        if fields.iter().any(|field| field.starts_with(prefix)) {
            return false;
        }
    }
    if fields.iter().any(|field| {
        FORMAT_PREFIXES
            .iter()
            .any(|prefix| field.starts_with(prefix))
    }) {
        return true;
    }
    fields.iter().any(|field| {
        SEMANTIC_PREFIXES
            .iter()
            .any(|prefix| field.starts_with(prefix))
    })
}

/// 不可变字段：阶段一 direction/cycle_position/gate_result；阶段二 diagnosis_summary.cycle_position。
pub fn detect_cheat(stage: &str, before: &Value, after: &Value, feedback: &str) -> Vec<String> {
    let immutable: &[&str] = if stage == "stage1" {
        &["direction", "cycle_position", "gate_result"]
    } else {
        &["diagnosis_summary.cycle_position"]
    };
    let mut cheats = Vec::new();
    for field in immutable {
        let previous = pointer_or_root(before, field);
        let current = pointer_or_root(after, field);
        if previous != current && !feedback.contains(field) {
            cheats.push(format!("{stage}:{field} 在重试中被修改且未在反馈中说明"));
        }
    }
    cheats
}

fn pointer_or_root(value: &Value, field: &str) -> Value {
    if field.contains('.') {
        value
            .pointer(&format!("/{}", field.replace('.', "/")))
            .cloned()
            .unwrap_or(Value::Null)
    } else {
        value.get(field).cloned().unwrap_or(Value::Null)
    }
}

/// 构造中文重试反馈消息。
pub fn build_retry_feedback(
    error: &ValidationError,
    stage: &str,
    attempt: u32,
    max_attempts: u32,
    _frame: &KlineFrame,
) -> String {
    let mut sections = vec![format!(
        "你上一轮输出的 {stage} JSON 校验失败（第 {attempt}/{max_attempts} 次重试）。"
    )];
    if !error.missing_fields.is_empty() {
        sections.push(format!(
            "缺少必填字段：{}。请补全这些字段。",
            error.missing_fields.join(", ")
        ));
    }
    if !error.invalid_fields.is_empty() {
        sections.push(format!(
            "以下字段有问题：\n{}\n请逐条修正。",
            error
                .invalid_fields
                .iter()
                .map(|field| format!("- {field}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if stage == "stage1" {
        sections.push("禁止修改不可变字段：direction、cycle_position、gate_result。".to_string());
    } else {
        sections.push("禁止修改 diagnosis_summary.cycle_position。".into());
        sections
            .push("order_type=不下单 时价格字段必须全为 null；有下单时三价+方向+胜率必填。".into());
    }
    sections.push("请重新输出**完整** JSON（不要只输出修正片段）。".to_string());
    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings() -> ValidationSettings {
        ValidationSettings::default()
    }

    #[test]
    fn category_a_retries_up_to_max() {
        let error = ValidationError {
            category: 'a',
            message: String::new(),
            missing_fields: vec![],
            invalid_fields: vec![],
        };
        assert!(should_retry(&error, 0, &settings()));
        assert!(should_retry(&error, 2, &settings()));
        assert!(!should_retry(&error, 3, &settings()));
    }

    #[test]
    fn metrics_failures_do_not_retry() {
        let error = ValidationError {
            category: 'c',
            message: String::new(),
            missing_fields: vec![],
            invalid_fields: vec!["metrics:盈亏比 0.8 超出范围".into()],
        };
        assert!(!should_retry(&error, 0, &settings()));
    }

    #[test]
    fn format_issues_retry_once() {
        let error = ValidationError {
            category: 'c',
            message: String::new(),
            missing_fields: vec![],
            invalid_fields: vec!["gate_trace: 末节点答案非法".into()],
        };
        assert!(should_retry(&error, 0, &settings()));
        assert!(!should_retry(&error, 1, &settings()));
    }

    #[test]
    fn cheat_detected_when_immutable_changed() {
        let before =
            json!({"direction": "bullish", "cycle_position": "spike", "gate_result": "proceed"});
        let after =
            json!({"direction": "bearish", "cycle_position": "spike", "gate_result": "proceed"});
        let cheats = detect_cheat("stage1", &before, &after, "请修正格式");
        assert_eq!(cheats.len(), 1);
        assert!(cheats[0].contains("direction"));
    }

    #[test]
    fn feedback_mentions_field_no_cheat() {
        let before = json!({"direction": "bullish"});
        let after = json!({"direction": "neutral"});
        let cheats = detect_cheat(
            "stage1",
            &before,
            &after,
            "direction 应为 neutral（证据：…）",
        );
        assert!(cheats.is_empty());
    }
}
