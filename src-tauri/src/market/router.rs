//! 策略文件路由（移植自 ai/router.py）：按 cycle_position + direction + detected_patterns
//! 选择注入阶段二的策略文件。

use serde_json::Value;

const UP_CHANNEL_FILES: &[&str] = &["上涨通道分析识别", "上涨通道交易策略"];
const DOWN_CHANNEL_FILES: &[&str] = &["下跌通道分析识别", "下跌通道交易策略"];
const CHANNEL_EXTRA: &str = "文件13-窄通道与宽通道策略";
const RANGE_FILES: &[&str] = &["震荡区间分析识别", "震荡区间交易策略"];
const SPIKE_UP_FILES: &[&str] = &["极速上涨分析识别", "极速上涨交易策略"];
const SPIKE_DOWN_FILES: &[&str] = &["极速下跌分析识别", "极速下跌交易策略"];

pub const STAGE1_TASK_FILES: &[&str] = &["市场诊断框架", "文件16-K线信号识别", "逐棒分析检查单"];
pub const STAGE2_BASE_FILES: &[&str] = &[
    "逐棒分析检查单",
    "文件16-K线信号识别",
    "文件17-止损和止盈与仓位管理",
];
pub const COMMON_FILES: &[&str] = &["提示词大纲_人设与思维方式", "二元决策"];

fn is_channel(cycle: &str) -> bool {
    matches!(
        cycle,
        "micro_channel" | "tight_channel" | "normal_channel" | "broad_channel"
    )
}

fn is_range(cycle: &str) -> bool {
    matches!(cycle, "trading_range" | "trending_tr")
}

fn base_files_for_cycle(
    cycle: &str,
    direction: &str,
    spike_stage: Option<&str>,
) -> Vec<&'static str> {
    if cycle == "spike" {
        return match (direction, spike_stage) {
            ("bullish", _) => SPIKE_UP_FILES.to_vec(),
            ("bearish", _) => SPIKE_DOWN_FILES.to_vec(),
            (_, Some("transitioning")) => {
                let mut files = if direction == "bearish" {
                    DOWN_CHANNEL_FILES.to_vec()
                } else {
                    UP_CHANNEL_FILES.to_vec()
                };
                files.push(CHANNEL_EXTRA);
                files
            }
            _ => {
                // ending：尖峰文件 + 通道文件
                let mut files = SPIKE_UP_FILES.to_vec();
                files.extend_from_slice(UP_CHANNEL_FILES);
                files.push(CHANNEL_EXTRA);
                files
            }
        };
    }
    if is_channel(cycle) {
        let mut files = if direction == "bearish" {
            DOWN_CHANNEL_FILES.to_vec()
        } else {
            UP_CHANNEL_FILES.to_vec()
        };
        files.push(CHANNEL_EXTRA);
        return files;
    }
    if is_range(cycle) {
        return RANGE_FILES.to_vec();
    }
    // extreme_tr / unknown：不加载策略文件（不下单）
    Vec::new()
}

fn overlay_files(patterns: &[String]) -> Vec<&'static str> {
    let mut files = Vec::new();
    let has = |needle: &str| patterns.iter().any(|p| p.contains(needle));
    if has("wedge") {
        files.push("文件14-楔形形态分析交易");
    }
    if has("reversal_attempt") || has("mtr") || has("final_flag") || has("h2") || has("l2") {
        files.push("文件15-二次入场机会");
    }
    if has("h1") || has("h2") || has("l1") || has("l2") {
        files.push("文件19-H1H2-L1L2计数");
    }
    if has("breakout_failure") || has("breakout_test") || has("pullback") {
        files.push("文件18-突破失败与突破测试");
    }
    if has("always_in") || has("ail") || has("ais") || has("20gb") || has("gap_bar") {
        files.push("文件20-AlwaysIn与20GB");
    }
    if has("barbwire") || has("overlap") || has("middle_range") {
        files.push("文件21-铁丝网与无交易环境");
    }
    if has("failed_signal") || has("magnet") {
        files.push("文件22-信号失败后的磁力位");
    }
    files
}

/// 主路由：返回策略文件名列表（去重、保序）。
pub fn route_strategy_files(stage1: &Value) -> Vec<String> {
    let cycle = stage1
        .get("cycle_position")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let direction = stage1
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("neutral");
    let spike_stage = stage1
        .get("spike_stage")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let patterns: Vec<String> = stage1
        .get("detected_patterns")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    let mut ordered: Vec<&str> = base_files_for_cycle(cycle, direction, spike_stage.as_deref());

    // 背景尖峰且与方向一致、当前非尖峰 → 追加尖峰文件
    let recent_spike = stage1
        .pointer("/trend_context/recent_spike")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !recent_spike.is_empty() && recent_spike == direction && cycle != "spike" {
        let spike_files = if direction == "bearish" {
            SPIKE_DOWN_FILES
        } else {
            SPIKE_UP_FILES
        };
        ordered.extend_from_slice(spike_files);
    }

    // 备选周期与主周期不同 → 追加其基础文件
    if let Some(alt) = stage1
        .get("alternative_cycle_position")
        .and_then(Value::as_str)
        .filter(|alt| !alt.is_empty() && *alt != cycle)
    {
        let alt_dir = if direction == "bearish" {
            "bearish"
        } else {
            "bullish"
        };
        ordered.extend(base_files_for_cycle(alt, alt_dir, None));
    }

    ordered.extend(overlay_files(&patterns));

    // 稳定去重
    let mut seen = std::collections::BTreeSet::new();
    ordered
        .into_iter()
        .filter(|name| seen.insert(name.to_string()))
        .map(|name| name.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn channels_route_by_direction() {
        let stage1 = json!({
            "cycle_position": "broad_channel",
            "direction": "bearish",
            "detected_patterns": [],
        });
        let files = route_strategy_files(&stage1);
        assert!(files.contains(&"下跌通道分析识别".to_string()));
        assert!(files.contains(&"文件13-窄通道与宽通道策略".to_string()));
    }

    #[test]
    fn extreme_tr_loads_nothing() {
        let stage1 = json!({"cycle_position": "extreme_tr", "direction": "neutral", "detected_patterns": []});
        assert!(route_strategy_files(&stage1).is_empty());
    }

    #[test]
    fn pattern_overlays_append_once() {
        let stage1 = json!({
            "cycle_position": "trading_range",
            "direction": "neutral",
            "detected_patterns": ["wedge", "h2", "reversal_attempt"],
        });
        let files = route_strategy_files(&stage1);
        assert_eq!(
            files.iter().filter(|f| *f == "文件15-二次入场机会").count(),
            1
        );
        assert!(files.contains(&"文件14-楔形形态分析交易".to_string()));
    }
}
