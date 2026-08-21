//! Prompt 组装引擎（移植自 ai/prompt_assembler.py + decision_stance.py）。
//! system prompt 进程级缓存；增量分析采用 4 消息结构以命中 KV Cache。

use super::decision_tree::{DECISION_TREE, prompt_text};
use super::features::compute_geometry_features;
use super::types::{AnalysisRecord, KlineFrame, MarketSettings};
use serde_json::{Value, json};
use std::sync::LazyLock;

pub const LANGUAGE_RULE: &str = "全程使用简体中文思考和输出；所有 JSON 字段的字符串值也使用简体中文（枚举值按 schema 原样输出）。";
pub const TERMINOLOGY: &str = "严格使用 Al Brooks 价格行为术语：微型通道/窄通道/正常通道/宽通道/尖峰/趋势型交易区间/交易区间/极端交易区间/二次入场/H1H2/L1L2/Always-In/铁丝网/磁力位/突破跟随/内包棒/外包棒/信号棒/入场棒。";
pub const OUTPUT_RULE: &str = "最终只输出一个合法 JSON 对象，不要 markdown 围栏，不要 JSON 之外的任何文字。思考过程写在思考区，不要写进 JSON。";

const STAGE1_OUTPUT_CONTRACT: &str = r#"输出 JSON 必须包含以下字段：
- cycle_position: spike|micro_channel|tight_channel|normal_channel|broad_channel|trending_tr|trading_range|extreme_tr|unknown
- alternative_cycle_position: 备选周期或 null
- direction: bullish|bearish|neutral
- diagnosis_confidence: 0-100
- spike_stage: cycle_position=spike 时必填 active|ending|transitioning，否则 null
- market_phase: stable|transitioning
- transition_risk: market_phase=transitioning 时必填 high|medium|low
- detected_patterns: 字符串数组（wedge/mtr/reversal_attempt/h2/h1/l1/l2/breakout_failure/breakout_test/always_in/ail/ais/20gb/gap_bar/barbwire/overlap/middle_range/failed_signal/magnet/final_flag/pullback）
- key_signals: 字符串数组
- htf_context: 高时间框架描述
- entry_setup: 入场结构描述
- strategy_files_needed: 需要加载的策略文件名数组
- risk_warning: 风险提示
- support_levels / resistance_levels: 最多 3 个的价格描述数组
- bar_analysis: {always_in: long|short|neutral, last_closed_bar, bar_type, signal_bar, entry_setup_type, follow_through, entry_bar, second_entry, tr_position, breakout_quality}
- bar_by_bar_summary: 8-12 条 {bar:"K1", role, bar_type, context_effect, follow_through, trapped_side, reason}
- gate_trace: 决策树节点追踪数组（含 §0.1-§2.5 中你实际评估的节点）
- gate_result: proceed|wait|unknown
- node_overrides: 覆盖程序节点时填写 [{node_id, answer, branch, override_reason}]"#;

const STAGE2_OUTPUT_CONTRACT: &str = r#"输出 JSON 必须包含以下字段：
- decision: {order_type: 限价单|突破单|市价单|不下单, order_direction: 做多|做空|null, entry_price, take_profit_price, stop_loss_price, entry_basis_bar(突破单必填), entry_basis_extreme(突破单必填 high|low), entry_rule(突破单必填), reasoning, diagnosis_confidence, diagnosis_confidence_reasoning, trade_confidence, trade_confidence_reasoning, estimated_win_rate(有下单必填 0-100), estimated_win_rate_reasoning, key_factors, watch_points, risk_assessment, invalidation_condition}
- diagnosis_summary: {cycle_position, direction, key_signals}
- decision_trace: §3-§14 决策树节点追踪数组（有下单时必须包含 §9、§10.1、§10.2、§10.3、§11）
- terminal: {node_id, outcome: wait|reject|trade|proceed, label}
- bar_analysis（可选）
- next_bar_prediction: {direction: bullish|bearish|neutral|null, probabilities: {bullish,bearish,neutral} 三者和为 100±1 且 direction=argmax, reasoning(1-1500字), unpredictable: bool, features_used}
- next_cycle_prediction: {cycle, direction, probabilities: 8 个周期枚举概率和为 100±1 且 cycle=argmax, reasoning, unpredictable, features_used}
- node_overrides
铁律：
1. order_type=不下单 时，order_direction/entry_price/entry_basis_bar/entry_basis_extreme/entry_rule/take_profit_price/stop_loss_price/estimated_win_rate 必须全为 null。
2. 有下单时三价 + order_direction + estimated_win_rate 必填。
3. 突破单做多 basis_extreme=high，做空=low。
4. 盈亏比（reward/risk）必须在 1.0-1.5 之间，且 胜率×回报 > 败率×风险。
5. §14 触犯任何禁止行为时强制不下单。"#;

fn join_files(files: &[&str]) -> String {
    files
        .iter()
        .map(|name| {
            let body = prompt_text(name);
            format!("# {name}\n{body}")
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

pub static SYSTEM_PROMPT: LazyLock<String> = LazyLock::new(|| {
    [
        LANGUAGE_RULE.to_string(),
        TERMINOLOGY.to_string(),
        OUTPUT_RULE.to_string(),
        join_files(super::router::COMMON_FILES),
    ]
    .join("\n\n---\n\n")
});

/// UTC 毫秒 → "MM-DD HH:MM"（表格紧凑展示，Python 版用本地时间，此处用 UTC 简化）。
pub fn format_ts(ts_ms: f64) -> String {
    let seconds = (ts_ms / 1000.0).floor() as i64;
    let days = seconds.div_euclid(86400);
    let secs_of_day = seconds.rem_euclid(86400);
    // civil_from_days (Howard Hinnant 算法)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let _ = era;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    format!("{m:02}-{d:02} {hour:02}:{minute:02}")
}

pub fn render_kline_table(frame: &KlineFrame, limit: usize) -> String {
    let fmt_indicator = |value: &Option<f64>| {
        value
            .map(|x| format!("{x:.2}"))
            .unwrap_or_else(|| "—".into())
    };
    let mut rows = vec!["序号|时间|开|高|低|收|阴阳|量|EMA20|ATR14".to_string()];
    for (index, bar) in frame.bars.iter().take(limit).enumerate() {
        let ema = frame
            .indicators
            .ema20
            .get(index)
            .map(fmt_indicator)
            .unwrap_or_else(|| "—".into());
        let atr = frame
            .indicators
            .atr14
            .get(index)
            .map(fmt_indicator)
            .unwrap_or_else(|| "—".into());
        rows.push(format!(
            "K{}|{}|{:.2}|{:.2}|{:.2}|{:.2}|{}|{:.0}|{}|{}",
            bar.seq,
            format_ts(bar.ts_open),
            bar.open,
            bar.high,
            bar.low,
            bar.close,
            bar.candle_label(),
            bar.volume,
            ema,
            atr
        ));
    }
    rows.join("\n")
}

pub fn render_feature_table(frame: &KlineFrame, limit: usize) -> String {
    let features = compute_geometry_features(frame, Some(limit));
    let mut rows = vec!["序号|类型|实体比|上影|下影|收盘位|振幅ATR|EMA关系|与前棒重叠|内包|微型双底顶|跳空|突破前高/低|跟随".to_string()];
    for feature in &features {
        let fmt = |value: Option<f64>| {
            value
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "—".into())
        };
        rows.push(format!(
            "K{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            feature.seq,
            feature.bar_type,
            fmt(feature.body_ratio),
            fmt(feature.upper_wick_ratio),
            fmt(feature.lower_wick_ratio),
            fmt(feature.close_position),
            fmt(feature.range_atr_ratio),
            feature.ema_relation,
            fmt(feature.overlap_prev_ratio),
            feature.inside_sequence,
            feature.micro_double,
            feature.gap_bar,
            feature.breakout_prev,
            feature.follow_through_1_2,
        ));
    }
    rows.join("\n")
}

pub fn render_three_window_summary(frame: &KlineFrame, trend_ctx: &Value) -> String {
    format!(
        "背景方向（K41 以前）：{}；交易方向（近期）：{}；关系：{}；近端尖峰：{}；规则：{}",
        trend_ctx
            .get("background_direction")
            .and_then(Value::as_str)
            .unwrap_or("neutral"),
        trend_ctx
            .get("trading_direction")
            .and_then(Value::as_str)
            .unwrap_or("neutral"),
        trend_ctx
            .get("relationship")
            .and_then(Value::as_str)
            .unwrap_or("mixed"),
        trend_ctx
            .get("recent_spike")
            .and_then(Value::as_str)
            .unwrap_or("无"),
        trend_ctx
            .get("with_trend_rule")
            .and_then(Value::as_str)
            .unwrap_or(""),
    ) + &format!("（参考 K 线总数：{}）", frame.bars.len())
}

pub fn stance_guidance(stance: &str) -> String {
    let common = "通用约束：盈亏比上限 1.5:1；突破单不可行时尝试限价单；限价单对照 K1 是否已穿越；estimated_win_rate 必填；decision 与 decision_trace 必须一致。";
    let specific = match stance {
        "conservative" => "保守：优先清晰信号；边际情况倾向不下单；盈亏比要求 ≥1.5:1。",
        "balanced" => {
            "均衡：允许次优但可执行的二类 setup；可接受 ~1.2:1 盈亏比；方向一致时优先考虑下单。"
        }
        "aggressive" => "激进：接受更早更不完美的入场；可接受 ~1.0:1 盈亏比；主动寻找可下单方案。",
        "extreme_aggressive" => {
            "极度激进：必须给出具体做多/做空方案；不可因犹豫输出不下单（§14 硬性禁止除外）。"
        }
        _ => "均衡：允许次优但可执行的二类 setup；可接受 ~1.2:1 盈亏比；方向一致时优先考虑下单。",
    };
    format!("{common}\n{specific}")
}

/// 从已校验的阶段一提取传给阶段二的紧凑子集。
pub fn compact_stage1(stage1: &Value) -> Value {
    json!({
        "cycle_position": stage1.get("cycle_position").cloned().unwrap_or(json!("unknown")),
        "alternative_cycle_position": stage1.get("alternative_cycle_position").cloned().unwrap_or(Value::Null),
        "direction": stage1.get("direction").cloned().unwrap_or(json!("neutral")),
        "spike_stage": stage1.get("spike_stage").cloned().unwrap_or(Value::Null),
        "market_phase": stage1.get("market_phase").cloned().unwrap_or(json!("stable")),
        "transition_risk": stage1.get("transition_risk").cloned().unwrap_or(Value::Null),
        "detected_patterns": stage1.get("detected_patterns").cloned().unwrap_or(json!([])),
        "key_signals": stage1.get("key_signals").cloned().unwrap_or(json!([])),
        "risk_warning": stage1.get("risk_warning").cloned().unwrap_or(json!("")),
        "support_levels": stage1.get("support_levels").cloned().unwrap_or(json!([])),
        "resistance_levels": stage1.get("resistance_levels").cloned().unwrap_or(json!([])),
        "bar_analysis": stage1.get("bar_analysis").cloned().unwrap_or(json!({})),
        "bar_by_bar_summary": stage1.get("bar_by_bar_summary").cloned().unwrap_or(json!([])),
        "trend_context": stage1.get("trend_context").cloned().unwrap_or(json!({})),
        "entry_setup": stage1.get("entry_setup").cloned().unwrap_or(json!("")),
    })
}

fn decision_tree_outline() -> String {
    DECISION_TREE
        .sections
        .iter()
        .filter(|(id, _)| *id >= 3)
        .map(|(id, title)| {
            let nodes: Vec<String> = DECISION_TREE
                .nodes
                .values()
                .filter(|node| node.section_id == *id)
                .map(|node| format!("  §{} {}", node.id, node.question))
                .collect();
            format!("## {id}. {title}\n{}", nodes.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub struct Assembled {
    pub system: String,
    pub user: String,
}

pub fn build_stage1(
    frame: &KlineFrame,
    settings: &MarketSettings,
    experience: &str,
    trend_ctx: &Value,
) -> Assembled {
    let limit = settings.general.analysis_bar_count as usize;
    let task_files = join_files(super::router::STAGE1_TASK_FILES);
    let pattern_briefs = if settings.prompt.stage1_inject_pattern_briefs {
        prompt_text("文件16-K线信号识别")
    } else {
        ""
    };
    let kline_table = render_kline_table(frame, limit);
    let feature_table = render_feature_table(frame, limit);
    let window_summary = render_three_window_summary(frame, trend_ctx);
    let experience_block = if experience.is_empty() {
        String::new()
    } else {
        format!("\n# 经验库参考案例\n{experience}\n")
    };
    let user = format!(
        r#"{LANGUAGE_RULE}

# 任务：市场诊断（阶段一）
你将看到一组已收盘 K 线（K1=最新已收盘，K 序号越大越旧）。请按价格行为框架完成市场诊断。

# 品种与周期
{symbol} · {timeframe} · 共 {bar_count} 根已收盘 K 线

# K 线数据表
{kline_table}

# 程序预计算几何特征表
{feature_table}

# 三窗口趋势背景
{window_summary}

# 诊断任务指引
{task_files}
{pattern_briefs}

# 输出契约
{STAGE1_OUTPUT_CONTRACT}
{experience_block}请基于以上信息输出阶段一诊断 JSON。"#,
        symbol = frame.symbol,
        timeframe = frame.timeframe,
        bar_count = frame.bars.len(),
    );
    Assembled {
        system: SYSTEM_PROMPT.clone(),
        user,
    }
}

pub fn build_incremental_stage1(
    frame: &KlineFrame,
    settings: &MarketSettings,
    previous: &AnalysisRecord,
    new_bar_count: usize,
    trend_ctx: &Value,
) -> Assembled {
    let full = build_stage1(frame, settings, "", trend_ctx);
    let previous_user = previous
        .stage1_messages
        .iter()
        .rev()
        .find(|message| message.get("role") == Some(&json!("user")))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let previous_assistant = previous
        .stage1_diagnosis
        .clone()
        .map(|value| serde_json::to_string_pretty(&value).unwrap_or_default())
        .unwrap_or_default();
    // 只渲染新增 K 线
    let new_limit = new_bar_count.min(frame.bars.len());
    let new_bars = KlineFrame {
        symbol: frame.symbol.clone(),
        timeframe: frame.timeframe.clone(),
        bars: frame.bars[..new_limit].to_vec(),
        indicators: IndicatorView::slice(&frame.indicators, new_limit),
        snapshot_ts_local_ms: frame.snapshot_ts_local_ms,
    };
    let new_kline_table = render_kline_table(&new_bars, new_limit);
    let new_feature_table = render_feature_table(&new_bars, new_limit);
    let window_summary = render_three_window_summary(frame, trend_ctx);
    let incremental_user = format!(
        r#"{LANGUAGE_RULE}

# 任务：增量市场诊断
上一轮分析后新增了 {new_bar_count} 根已收盘 K 线。请基于上一轮结论与新增 K 线做增量诊断。

反锚定要求：不要因为上一轮结论就倾向延续。问自己：如果你第一次看到这组完整 K 线，会得出什么结论？只有新增证据真正支持时才保留上一轮判断，否则必须改写。

# 新增 K 线（K1-K{new_limit} 为新增）
{new_kline_table}

# 新增 K 线几何特征
{new_feature_table}

# 三窗口趋势背景
{window_summary}

# 输出契约
{STAGE1_OUTPUT_CONTRACT}

请输出**本轮完整**的阶段一诊断 JSON（不是差异），并附加字段：
- incremental_delta: {{new_closed_bars: ["K1",...], changed_fields: [...], summary: "一句话说明相对上一轮的变化"}}
- node_overrides 如需改写程序节点填写。"#
    );
    Assembled {
        system: full.system,
        user: format!(
            "__PREV_USER__\n{previous_user}\n__PREV_ASSISTANT__\n{previous_assistant}\n__NEW_USER__\n{incremental_user}"
        ),
    }
}

/// 把增量 4 消息结构拆成实际 messages 数组。
pub fn expand_incremental(assembled: &Assembled) -> Vec<Value> {
    let text = &assembled.user;
    let (head, new_user) = text
        .split_once("__NEW_USER__\n")
        .unwrap_or(("", text.as_str()));
    let (prev_user, prev_assistant) = head
        .strip_prefix("__PREV_USER__\n")
        .unwrap_or(head)
        .split_once("__PREV_ASSISTANT__\n")
        .unwrap_or((head, ""));
    vec![
        json!({"role": "system", "content": assembled.system}),
        json!({"role": "user", "content": prev_user.trim()}),
        json!({"role": "assistant", "content": prev_assistant.trim()}),
        json!({"role": "user", "content": new_user.trim()}),
    ]
}

pub fn build_stage2(
    frame: &KlineFrame,
    stage1: &Value,
    strategy_files: &[String],
    settings: &MarketSettings,
    experience: &str,
) -> Assembled {
    let mut strategy_text = String::new();
    for name in strategy_files {
        let body = prompt_text(name);
        if !body.is_empty() {
            strategy_text.push_str(&format!("\n\n# 策略文件：{name}\n{body}"));
        }
    }
    let base_files = join_files(super::router::STAGE2_BASE_FILES);
    let system_prompt = SYSTEM_PROMPT.clone();
    let system = format!("{system_prompt}\n\n---\n\n# 阶段二基础文件\n{base_files}");
    let compact_stage1_json =
        serde_json::to_string_pretty(&compact_stage1(stage1)).unwrap_or_default();
    let tree_outline = decision_tree_outline();
    let stance = settings.stance().to_string();
    let stance_text = stance_guidance(&stance);
    let k1_close = frame.bars.first().map(|b| b.close).unwrap_or(0.0);
    let k1_ema = frame
        .indicators
        .ema20
        .first()
        .and_then(|v| v.map(|x| format!("{x:.2}")))
        .unwrap_or_else(|| "—".into());
    let k1_atr = frame
        .indicators
        .atr14
        .first()
        .and_then(|v| v.map(|x| format!("{x:.2}")))
        .unwrap_or_else(|| "—".into());
    let experience_block = if experience.is_empty() {
        String::new()
    } else {
        format!("\n# 经验库参考案例\n{experience}\n")
    };
    let user = format!(
        r#"{LANGUAGE_RULE}

# 任务：交易决策（阶段二）
基于阶段一诊断结果，按二元决策树走完整决策链，给出具体交易建议。绝不执行真实下单，仅输出建议。

# 阶段一诊断结果
{compact_stage1_json}

# 品种与周期
{symbol} · {timeframe} · K 线收盘价参考：K1 收盘 {k1_close:.2}，EMA20 {k1_ema}，ATR14 {k1_atr}

# 决策树大纲（§3-§14）
{tree_outline}

# 交易倾向（{stance}）
{stance_text}
{strategy_text}
{experience_block}
# 输出契约
{STAGE2_OUTPUT_CONTRACT}

请输出阶段二决策 JSON。"#,
        symbol = frame.symbol,
        timeframe = frame.timeframe,
    );
    Assembled { system, user }
}

pub fn build_stage1_messages(assembled: &Assembled) -> Vec<Value> {
    if assembled.user.contains("__PREV_USER__") {
        expand_incremental(assembled)
    } else {
        vec![
            json!({"role": "system", "content": assembled.system}),
            json!({"role": "user", "content": assembled.user}),
        ]
    }
}

// 指标切片辅助（增量消息只包含新增棒的指标）。
struct IndicatorView;

impl IndicatorView {
    fn slice(bundle: &super::types::IndicatorBundle, n: usize) -> super::types::IndicatorBundle {
        super::types::IndicatorBundle {
            ema20: bundle.ema20.iter().take(n).cloned().collect(),
            atr14: bundle.atr14.iter().take(n).cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_contains_common_files() {
        assert!(SYSTEM_PROMPT.contains("二元决策"));
        assert!(SYSTEM_PROMPT.contains("人设"));
    }

    #[test]
    fn ts_formats_as_month_day_time() {
        assert_eq!(format_ts(0.0), "01-01 00:00");
        // 2026-08-14 00:00:00 UTC = 1786665600000
        assert_eq!(format_ts(1_786_665_600_000.0), "08-14 00:00");
    }
}
