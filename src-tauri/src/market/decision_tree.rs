//! 二元决策树加载与 trace 校验（移植自 ai/decision_tree.py）。
//! 决策树定义嵌入自 market_prompts/二元决策.txt。

use once_cell_info::MARKET_PROMPTS;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::LazyLock;

pub const GATE_RESULTS: &[&str] = &["proceed", "wait", "unknown"];
pub const TRACE_ANSWERS: &[&str] = &["是", "否", "中性", "等待", "不适用"];
pub const TERMINAL_OUTCOMES: &[&str] = &["wait", "reject", "trade", "proceed"];

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub id: String,
    pub question: String,
    pub section_id: u32,
    pub section_title: String,
    pub branch_yes: Option<String>,
    pub branch_no: Option<String>,
}

#[derive(Debug, Default)]
pub struct DecisionTree {
    pub sections: Vec<(u32, String)>,
    pub nodes: BTreeMap<String, TreeNode>,
}

mod once_cell_info {
    include!(concat!(env!("OUT_DIR"), "/market_prompts.rs"));
}

pub static DECISION_TREE: LazyLock<DecisionTree> = LazyLock::new(|| {
    let source = MARKET_PROMPTS
        .iter()
        .find(|(name, _)| *name == "二元决策")
        .map(|(_, body)| *body)
        .unwrap_or_default();
    parse_decision_tree(source)
});

pub fn prompt_text(name: &str) -> &'static str {
    MARKET_PROMPTS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, body)| *body)
        .unwrap_or("")
}

pub fn parse_decision_tree(source: &str) -> DecisionTree {
    let mut tree = DecisionTree::default();
    let mut current_section: Option<(u32, String)> = None;
    let mut node_order: Vec<String> = Vec::new();
    for line in source.lines() {
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix("## ") {
            // "## 1. 标题" 或 "## 12. 标题"
            let (id, title) = match rest.split_once(". ") {
                Some((digits, title)) if digits.trim().parse::<u32>().is_ok() => (
                    digits.trim().parse::<u32>().unwrap(),
                    title.trim().to_string(),
                ),
                _ => match rest.split_once(' ') {
                    Some((digits, title)) if digits.trim().parse::<u32>().is_ok() => (
                        digits.trim().parse::<u32>().unwrap(),
                        title.trim().to_string(),
                    ),
                    _ => continue,
                },
            };
            current_section = Some((id, title.clone()));
            tree.sections.push((id, title));
            continue;
        }
        let Some(rest) = line.strip_prefix("### ") else {
            continue;
        };
        // "### 1.1 问题" / "### 14.1 问题" / "### 10.3A 问题"
        let (node_id, question) = match rest.split_once(' ') {
            Some((id, question)) => (id.trim().to_string(), question.trim().to_string()),
            None => continue,
        };
        if node_id.is_empty() || !node_id.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let (section_id, section_title) = current_section.clone().unwrap_or((0, String::new()));
        node_order.push(node_id.clone());
        tree.nodes.insert(
            node_id.clone(),
            TreeNode {
                id: node_id,
                question,
                section_id,
                section_title: section_title.clone(),
                branch_yes: None,
                branch_no: None,
            },
        );
    }
    // 第二遍提取分支文本
    let mut current: Option<String> = None;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("### ") {
            current = trimmed
                .strip_prefix("### ")
                .and_then(|rest| rest.split_once(' ').map(|(id, _)| id.trim().to_string()));
            continue;
        }
        let Some(node_id) = current.clone() else {
            continue;
        };
        if !tree.nodes.contains_key(&node_id) {
            continue;
        }
        if let Some(yes) = trimmed.strip_prefix("是：") {
            if let Some(node) = tree.nodes.get_mut(&node_id) {
                node.branch_yes = Some(yes.chars().take(140).collect());
            }
        } else if trimmed.starts_with("否") && trimmed.contains('：') {
            let text = trimmed.split_once('：').map(|x| x.1).unwrap_or("").trim();
            if let Some(node) = tree.nodes.get_mut(&node_id) {
                node.branch_no = Some(text.chars().take(140).collect());
            }
        }
    }
    let _ = node_order;
    tree
}

pub fn node_question(node_id: &str) -> String {
    DECISION_TREE
        .nodes
        .get(node_id)
        .map(|node| node.question.clone())
        .unwrap_or_else(|| format!("决策节点 {node_id}"))
}

/// node_id 数值排序键：14.1 < 14.10；主序号优先。
pub fn node_sort_key(node_id: &str) -> (u32, u32, String) {
    let cleaned = node_id.trim_start_matches('§');
    let mut parts = cleaned.split('.');
    let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(999);
    let minor_str = parts.next().unwrap_or("0");
    let minor_digits: String = minor_str
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let minor: u32 = minor_digits.parse().unwrap_or(0);
    let suffix = minor_str[minor_digits.len()..].to_string();
    (major, minor, suffix)
}

/// bar_range 归一化：K{n}-K{m}（老→新），K0 截到 K1。
pub fn normalize_bar_range(item: &mut Value, default_max_seq: u32) {
    let raw = item
        .get("bar_range")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let raw = raw.replace("K", "").replace('，', ",").replace(' ', "");
    let mut seqs: Vec<u32> = raw
        .split(['-', ','])
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    for seq in seqs.iter_mut() {
        if *seq == 0 {
            *seq = 1;
        }
        if default_max_seq > 0 {
            *seq = (*seq).min(default_max_seq);
        }
    }
    let text = match seqs.as_slice() {
        [] => "不适用".to_string(),
        [single] => format!("K{single}"),
        many => {
            let older = many.iter().copied().max().unwrap();
            let newer = many.iter().copied().min().unwrap();
            if older == newer {
                format!("K{older}")
            } else {
                format!("K{older}-K{newer}")
            }
        }
    };
    item["bar_range"] = json!(text);
    let (from, to) = if text == "不适用" {
        (None, None)
    } else {
        let older = seqs.iter().copied().max();
        let newer = seqs.iter().copied().min();
        (older, newer)
    };
    item["bar_from"] = from.map(Value::from).unwrap_or(Value::Null);
    item["bar_to"] = to.map(Value::from).unwrap_or(Value::Null);
}

/// gate_result 一致性校验（简化移植：排序 + 末节点答案 + 非空）。
pub fn validate_gate_result_consistency(stage1: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let trace = stage1
        .get("gate_trace")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if trace.is_empty() {
        errors.push("gate: gate_trace 为空".into());
        return errors;
    }
    let gate_result = stage1
        .get("gate_result")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut last_answer: Option<&str> = None;
    for item in &trace {
        let answer = item.get("answer").and_then(Value::as_str).unwrap_or("");
        if !TRACE_ANSWERS.contains(&answer) {
            errors.push(format!("gate: answer 非法 ({answer})"));
        }
        last_answer = Some(answer);
    }
    if (gate_result == "wait" || gate_result == "unknown")
        && last_answer.is_some_and(|a| a != "否" && a != "等待")
    {
        errors.push(format!(
            "gate: gate_result={gate_result} 但末节点答案为 {last_answer:?}（应为 否/等待）"
        ));
    }
    errors
}

/// 阶段二 trace 一致性（简化移植：terminal 一致 + §10.3 在 §11 前 + §9 在 §10.1 前）。
pub fn validate_stage2_trace_consistency(stage2: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let order_type = stage2
        .pointer("/decision/order_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    let outcome = stage2
        .pointer("/terminal/outcome")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !TERMINAL_OUTCOMES.contains(&outcome) {
        errors.push(format!("trace: terminal.outcome 非法 ({outcome})"));
    }
    if order_type == "不下单" && !matches!(outcome, "wait" | "reject" | "proceed") {
        errors.push("trace: 不下单但 terminal 非 wait/reject".into());
    }
    if order_type != "不下单" && outcome != "trade" {
        errors.push("trace: 有下单但 terminal 非 trade".into());
    }
    let ids: Vec<String> = stage2
        .get("decision_trace")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.get("node_id").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let has = |prefix: &str| {
        ids.iter()
            .any(|id| id == prefix || id.starts_with(&format!("{prefix}.")))
    };
    let position = |prefix: &str| {
        ids.iter()
            .position(|id| id == prefix || id.starts_with(&format!("{prefix}.")))
    };
    if order_type != "不下单" {
        if !has("9") {
            errors.push("trace: 有下单但缺少 §9 入场信号链".into());
        }
        if !has("10.1") || !has("10.2") || !has("10.3") {
            errors.push("trace: 有下单但缺少 §10.1-10.3 风险收益链".into());
        }
        if let (Some(pos_9), Some(pos_101)) = (position("9"), position("10.1"))
            && pos_9 > pos_101
        {
            errors.push("trace: §9 应在 §10.1 之前".into());
        }
        if let (Some(pos_103), Some(pos_11)) = (position("10.3"), position("11"))
            && pos_103 > pos_11
        {
            errors.push("trace: §10.3 应在 §11 之前".into());
        }
    }
    errors
}

/// 闸门短路时程序合成的阶段二"不下单"桩。
pub fn build_stage2_gate_wait_response(stage1: &Value) -> Value {
    let cycle = stage1
        .get("cycle_position")
        .cloned()
        .unwrap_or(json!("unknown"));
    let direction = stage1.get("direction").cloned().unwrap_or(json!("neutral"));
    let gate_trace = stage1
        .get("gate_trace")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let last_reason = gate_trace
        .as_array()
        .and_then(|items| items.last())
        .and_then(|item| item.get("reason"))
        .and_then(Value::as_str)
        .unwrap_or("闸门未通过")
        .to_string();
    let wait_node = |node_id: &str, answer: &str, reason: &str| {
        json!({
            "node_id": node_id,
            "question": node_question(node_id),
            "answer": answer,
            "reason": reason,
            "branch": null,
            "bar_range": "不适用",
            "skipped": false,
            "section": "闸门",
        })
    };
    json!({
        "decision": {
            "order_type": "不下单",
            "order_direction": null,
            "entry_price": null,
            "entry_basis_bar": null,
            "entry_basis_extreme": null,
            "entry_rule": null,
            "take_profit_price": null,
            "stop_loss_price": null,
            "reasoning": format!("阶段一闸门未通过：{last_reason}，按规则不下单。"),
            "diagnosis_confidence": stage1.get("diagnosis_confidence").cloned().unwrap_or(json!(0)),
            "diagnosis_confidence_reasoning": "闸门短路，阶段二未调用模型。",
            "trade_confidence": 0,
            "trade_confidence_reasoning": "闸门未通过，不评估交易信心。",
            "estimated_win_rate": null,
            "estimated_win_rate_reasoning": "未进入交易评估。",
            "key_factors": [last_reason],
            "watch_points": ["等待闸门条件好转后重新提交分析"],
            "risk_assessment": "无交易计划，无风险敞口。",
            "invalidation_condition": null,
        },
        "diagnosis_summary": {
            "cycle_position": cycle,
            "direction": direction,
            "key_signals": stage1.get("key_signals").cloned().unwrap_or(json!([])),
        },
        "decision_trace": [
            wait_node("9.0", "否", "闸门未通过，跳过信号搜索"),
            wait_node("10.1", "不适用", "无候选方案"),
            wait_node("10.3", "不适用", "无候选方案"),
            wait_node("11.0", "不适用", "闸门短路"),
        ],
        "terminal": { "node_id": "0.2", "outcome": "wait", "label": "闸门未通过，等待" },
        "bar_analysis": stage1.get("bar_analysis").cloned().unwrap_or(json!({})),
        "gate_shortcircuited": true,
        "next_bar_prediction": {
            "direction": null, "probabilities": null,
            "reasoning": "闸门短路，未生成预测。",
            "unpredictable": true,
            "features_used": ["stage1_diagnosis"],
        },
        "next_cycle_prediction": {
            "cycle": null, "direction": null, "probabilities": null,
            "reasoning": "闸门短路，未生成预测。",
            "unpredictable": true,
            "features_used": ["stage1_diagnosis"],
        },
        "node_overrides": [],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_loads_sections_and_nodes() {
        let tree = &*DECISION_TREE;
        assert!(!tree.sections.is_empty());
        assert!(tree.nodes.contains_key("1.1"));
        assert!(tree.nodes.contains_key("10.3"));
        assert!(tree.nodes.contains_key("13.5"));
        let node = &tree.nodes["1.1"];
        assert!(node.question.contains("数据"));
    }

    #[test]
    fn node_sort_orders_numerically() {
        assert!(node_sort_key("2.3") < node_sort_key("2.10"));
        assert!(node_sort_key("9.5") < node_sort_key("10.1"));
        assert!(node_sort_key("10.3") < node_sort_key("11.1"));
    }

    #[test]
    fn bar_range_clamps_k0_to_k1() {
        let mut item = json!({"bar_range": "K0-K3"});
        normalize_bar_range(&mut item, 40);
        assert_eq!(item["bar_range"], "K3-K1");
        assert_eq!(item["bar_from"], 3);
        assert_eq!(item["bar_to"], 1);
    }
}
