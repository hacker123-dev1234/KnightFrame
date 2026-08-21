//! 确定性决策节点引擎（移植自 ai/decision_nodes.py + trend_context.py）。
//! 阈值常量与 Python 版严格对齐，任何偏差都会导致程序判定与原版不一致。

use super::decision_tree::{node_question, node_sort_key};
use super::types::{KlineBar, KlineFrame, NodeFill};
use serde_json::{Value, json};

pub const BAR_COUNT_THRESHOLD: usize = 20;
pub const DIRECTION_WINDOW: usize = 8;
pub const DIRECTION_WINDOW_MED: usize = 20;
pub const DIRECTION_STRONG_SHORT_SCORE: i32 = 4;
pub const DIRECTION_BULL_THRESHOLD: i32 = 3;
pub const DIRECTION_BEAR_THRESHOLD: i32 = -3;
pub const ALWAYS_IN_NEAR_WINDOW: usize = 8;
pub const ALWAYS_IN_WINDOW: usize = 20;
pub const ALWAYS_IN_NEAR_SAME_SIDE_RATIO: f64 = 0.65;
pub const ALWAYS_IN_SAME_SIDE_RATIO: f64 = 0.70;
pub const ALWAYS_IN_PULLBACK_ATR_RATIO: f64 = 1.5;
pub const SIGNAL_BAR_LONG_ATR_RATIO: f64 = 2.0;
pub const EMA_SLOPE_LOOKBACK: usize = 10;
pub const TREND_BAR_DOMINANCE_RATIO: f64 = 1.5;
pub const OVERLAP_LOW_THRESHOLD: f64 = 0.45;
pub const OVERLAP_HIGH_THRESHOLD: f64 = 0.65;
pub const CHAOS_OVERLAP_THRESHOLD: f64 = 0.70;
pub const CHAOS_EMA_FLAT_ATR_RATIO: f64 = 0.05;
pub const CHAOS_DIRECTION_SCORE_MAX: i32 = 1;
pub const MOMENTUM_OVERLAP_WEAK: f64 = 0.50;
pub const MOMENTUM_TREND_RATIO_STRONG: f64 = 1.5;
pub const MOMENTUM_PULLBACK_DEEP_ATR: f64 = 3.0;
pub const MOMENTUM_TREND_BAR_MIN_RATIO: f64 = 0.50;

pub const LOCKED_NODES: &[&str] = &["1.1", "9.1"];
pub const SAFETY_GATE_NODES: &[&str] = &["1.1", "10.3", "14", "14.1"];
pub const OVERRIDABLE_NODES: &[&str] = &[
    "1.3", "2.3", "2.4", "2.5", "9.2", "9.3", "11.1", "11.2", "11.3", "11.4",
];

// ---------------------------------------------------------------------------
// 基础信号
// ---------------------------------------------------------------------------

fn is_trend_bull(bar: &KlineBar) -> bool {
    matches!(bar.body_ratio(), Some(r) if r > 0.25)
        && matches!(bar.close_position(), Some(p) if p >= 0.65)
}
fn is_trend_bear(bar: &KlineBar) -> bool {
    matches!(bar.body_ratio(), Some(r) if r > 0.25)
        && matches!(bar.close_position(), Some(p) if p <= 0.35)
}

fn ema_slope(frame: &KlineFrame, lookback: usize) -> Option<f64> {
    let emas = &frame.indicators.ema20;
    if emas.is_empty() {
        return None;
    }
    let k = lookback.min(emas.len() - 1);
    let newest = emas.first().copied().flatten()?;
    let older = emas.get(k).copied().flatten()?;
    Some(newest - older)
}

fn overlap_ratio(a: &KlineBar, b: &KlineBar) -> Option<f64> {
    let union_high = a.high.max(b.high);
    let union_low = a.low.min(b.low);
    let union = union_high - union_low;
    if union <= 0.0 {
        return None;
    }
    Some((a.high.min(b.high) - a.low.max(b.low)).max(0.0) / union)
}

fn mean_overlap(bars: &[KlineBar]) -> Option<f64> {
    let mut sum = 0.0;
    let mut count = 0usize;
    for window in [0, 1] {
        if window + 1 >= bars.len() {
            break;
        }
        if let Some(value) = overlap_ratio(&bars[window], &bars[window + 1]) {
            sum += value;
            count += 1;
        }
    }
    for index in 2..bars.len() {
        if let Some(value) = overlap_ratio(&bars[index - 1], &bars[index]) {
            sum += value;
            count += 1;
        }
    }
    (count > 0).then(|| sum / count as f64)
}

/// 线性递减加权重心：窗口内 bars[0] 权重 W，bars[W-1] 权重 1。
fn weighted_close_centroid(bars: &[KlineBar]) -> Option<f64> {
    if bars.is_empty() {
        return None;
    }
    let n = bars.len();
    let total_weight: f64 = (1..=n).sum::<usize>() as f64;
    let weighted: f64 = bars
        .iter()
        .enumerate()
        .map(|(index, bar)| bar.close * (n - index) as f64)
        .sum();
    Some(weighted / total_weight)
}

/// 2-bar pivot 摆动点检测（左右各 2 根）。
fn swing_points(bars: &[KlineBar]) -> (Vec<f64>, Vec<f64>) {
    let mut highs = Vec::new();
    let mut lows = Vec::new();
    if bars.len() < 5 {
        return (highs, lows);
    }
    for i in 2..bars.len() - 2 {
        let candidate = &bars[i];
        let is_high = bars[i - 2..=i + 2]
            .iter()
            .enumerate()
            .all(|(offset, bar)| offset == 2 || bar.high < candidate.high);
        let is_low = bars[i - 2..=i + 2]
            .iter()
            .enumerate()
            .all(|(offset, bar)| offset == 2 || bar.low > candidate.low);
        if is_high {
            highs.push(candidate.high);
        }
        if is_low {
            lows.push(candidate.low);
        }
    }
    (highs, lows)
}

fn swing_structure_score(bars: &[KlineBar]) -> Option<i32> {
    let (highs, lows) = swing_points(bars);
    if highs.len() < 2 || lows.len() < 2 {
        return None;
    }
    // swing_points 返回旧→新（因为 bars 新→旧且窗口向前推进），取末两个
    let hh = highs[highs.len() - 1] > highs[highs.len() - 2];
    let hl = lows[lows.len() - 1] > lows[lows.len() - 2];
    let ll = lows[lows.len() - 1] < lows[lows.len() - 2];
    let lh = highs[highs.len() - 1] < highs[highs.len() - 2];
    if hh && hl {
        Some(1)
    } else if ll && lh {
        Some(-1)
    } else {
        Some(0)
    }
}

// ---------------------------------------------------------------------------
// §2.3 五信号方向投票
// ---------------------------------------------------------------------------

fn direction_vote(frame: &KlineFrame, window: usize) -> (i32, [i32; 5]) {
    let bars = &frame.bars;
    let w = window.min(bars.len());
    let atr = frame
        .indicators
        .atr14
        .first()
        .copied()
        .flatten()
        .unwrap_or(0.0);
    let window_bars = &bars[..w];

    // S1 EMA 斜率
    let k = EMA_SLOPE_LOOKBACK.min(bars.len().saturating_sub(1));
    let s1 = ema_slope(frame, k.max(1))
        .map(|d| {
            let thr = 0.05 * atr;
            if d > thr {
                1
            } else if d < -thr {
                -1
            } else {
                0
            }
        })
        .unwrap_or(0);

    // S2 加权收盘重心（近半 vs 远半）
    let half = (w / 2).max(1);
    let near = weighted_close_centroid(&window_bars[..half]);
    let far = weighted_close_centroid(&window_bars[half.min(w - 1)..]);
    let s2 = match (near, far) {
        (Some(near), Some(far)) => {
            let thr = 0.1 * atr;
            let diff = near - far;
            if diff > thr {
                1
            } else if diff < -thr {
                -1
            } else {
                0
            }
        }
        _ => 0,
    };

    // S3 波段结构
    let s3 = swing_structure_score(window_bars).unwrap_or(0);

    // S4 趋势棒占比
    let bull_tb = window_bars.iter().filter(|b| is_trend_bull(b)).count();
    let bear_tb = window_bars.iter().filter(|b| is_trend_bear(b)).count();
    let s4 = match (bull_tb, bear_tb) {
        (0, 0) => 0,
        (0, _) => -1,
        (_, 0) => 1,
        (bull, bear) => {
            let ratio = bull as f64 / bear as f64;
            let inverse = bear as f64 / bull as f64;
            if ratio >= TREND_BAR_DOMINANCE_RATIO {
                1
            } else if inverse >= TREND_BAR_DOMINANCE_RATIO {
                -1
            } else {
                0
            }
        }
    };

    // S5 K 线重叠：低重叠时跟随 S1，其余为 0
    let mean = mean_overlap(window_bars).unwrap_or(0.5);
    let s5 = if mean < OVERLAP_LOW_THRESHOLD {
        if s1 > 0 {
            1
        } else if s1 < 0 {
            -1
        } else {
            0
        }
    } else {
        0
    };

    (s1 + s2 + s3 + s4 + s5, [s1, s2, s3, s4, s5])
}

pub fn judge_direction(frame: &KlineFrame) -> (String, NodeFill) {
    let bars = &frame.bars;
    let (mut score, signals) = direction_vote(frame, DIRECTION_WINDOW);
    // 中窗口确认：冲突且 |score|<4 时向 0 收敛
    let w_med = DIRECTION_WINDOW_MED.min(bars.len());
    let med_bars = &bars[..w_med];
    let half = (w_med / 2).max(1);
    let near = weighted_close_centroid(&med_bars[..half]);
    let far = weighted_close_centroid(&med_bars[half.min(w_med - 1)..]);
    let atr = frame
        .indicators
        .atr14
        .first()
        .copied()
        .flatten()
        .unwrap_or(0.0);
    let med_sign = match (near, far) {
        (Some(n), Some(f)) if n - f > 0.1 * atr => 1,
        (Some(n), Some(f)) if f - n > 0.1 * atr => -1,
        _ => 0,
    };
    if med_sign != 0 && med_sign != score.signum() && score.abs() < DIRECTION_STRONG_SHORT_SCORE {
        score -= med_sign.signum();
    }

    let (direction, branch) = if score >= DIRECTION_BULL_THRESHOLD {
        ("bullish", "bullish")
    } else if score <= DIRECTION_BEAR_THRESHOLD {
        ("bearish", "bearish")
    } else {
        ("neutral", "neutral")
    };
    let answer = if direction == "neutral" {
        "中性"
    } else {
        "是"
    };
    let signal_text = signals
        .iter()
        .map(|s| {
            if *s > 0 {
                "+1"
            } else if *s < 0 {
                "-1"
            } else {
                "0"
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    let fill = NodeFill {
        node_id: "2.3".into(),
        answer,
        reason: format!(
            "五信号投票得分 {score}（EMA斜率/重心/波段/趋势棒/重叠 = {signal_text}），判定 {direction}"
        ),
        bar_range: format!("K{}-K1", DIRECTION_WINDOW.min(bars.len())),
        branch: Some(branch.into()),
        section: Some("多空方向判断"),
    };
    (direction.into(), fill)
}

// ---------------------------------------------------------------------------
// §2.4 Always In 双窗口
// ---------------------------------------------------------------------------

#[allow(dead_code)] // PA 全量移植保留：AiVerdict 三元组（与 Python 版对齐）
struct AiVerdict {
    answer: &'static str,
    label: String,
    reason: String,
}

fn always_in_core(
    frame: &KlineFrame,
    window: usize,
    same_side_ratio: f64,
    slope_lookback: usize,
) -> (Option<&'static str>, Option<bool>) {
    // 返回 (方向 Some("bull"/"bear")|None, gate3 Some(bool)|None)
    let bars = &frame.bars;
    let w = window.min(bars.len());
    let window_bars = &bars[..w];
    let emas = &frame.indicators.ema20;
    let atr = frame
        .indicators
        .atr14
        .first()
        .copied()
        .flatten()
        .unwrap_or(0.0);

    let total_weight: f64 = (1..=w).sum::<usize>() as f64;
    let mut above = 0.0;
    for (index, bar) in window_bars.iter().enumerate() {
        let ema = emas.get(index).copied().flatten().unwrap_or(f64::NAN);
        if bar.close > ema {
            above += (w - index) as f64;
        }
    }
    let above_ratio = above / total_weight;
    let below_ratio = 1.0 - above_ratio;
    let slope = ema_slope(frame, slope_lookback).unwrap_or(0.0);

    let bull_core = above_ratio >= same_side_ratio && slope > 0.0;
    let bear_core = below_ratio >= same_side_ratio && slope < 0.0;
    if !bull_core && !bear_core {
        return (None, None);
    }
    let side = if bull_core { "bull" } else { "bear" };
    // Gate3：波段结构一致 + 浅回撤
    let swing = swing_structure_score(window_bars).unwrap_or(0);
    let max_close = window_bars.iter().map(|b| b.close).fold(f64::MIN, f64::max);
    let min_close = window_bars.iter().map(|b| b.close).fold(f64::MAX, f64::min);
    let shallow = atr > 0.0 && (max_close - min_close) / atr <= ALWAYS_IN_PULLBACK_ATR_RATIO;
    let swing_ok = match side {
        "bull" => swing > 0,
        _ => swing < 0,
    };
    (Some(side), Some(swing_ok && shallow))
}

pub fn judge_always_in(frame: &KlineFrame) -> NodeFill {
    let bars = &frame.bars;
    let near_w = ALWAYS_IN_NEAR_WINDOW.min(bars.len());
    let (near_side, near_gate3) = always_in_core(frame, near_w, ALWAYS_IN_NEAR_SAME_SIDE_RATIO, 5);
    let bg_w = ALWAYS_IN_WINDOW.min(bars.len());
    let (bg_side, _bg_gate3) = always_in_core(frame, bg_w, ALWAYS_IN_SAME_SIDE_RATIO, 10);

    let (answer, label, reason) = if let Some(side) = near_side {
        let always = if side == "bull" { "AIL" } else { "AIS" };
        let strength = if near_gate3.unwrap_or(false) {
            "强"
        } else {
            "弱"
        };
        (
            "是",
            always.to_string(),
            format!("近端窗口({near_w}根)核心成立，{strength}{always}"),
        )
    } else if let Some(side) = bg_side {
        let always = if side == "bull" { "AIL" } else { "AIS" };
        (
            "是",
            format!("弱{always}"),
            format!("背景窗口({bg_w}根)核心成立但近端未共振，弱{always}"),
        )
    } else {
        ("否", "无".into(), "双窗口均未形成核心方向持仓共识".into())
    };
    NodeFill {
        node_id: "2.4".into(),
        answer,
        reason,
        bar_range: format!("K{}-K1", bg_w),
        branch: (label != "无").then_some(label),
        section: Some("多空方向判断"),
    }
}

// ---------------------------------------------------------------------------
// §1.1 / §1.3 / §2.5
// ---------------------------------------------------------------------------

pub fn check_preflight_data(frame: &KlineFrame) -> Result<(), String> {
    let bars = &frame.bars;
    if bars.is_empty() {
        return Err("无 K 线数据".into());
    }
    if bars.len() < BAR_COUNT_THRESHOLD {
        return Err(format!(
            "K 线数量不足（{} < {BAR_COUNT_THRESHOLD}）",
            bars.len()
        ));
    }
    let ema_ready = frame
        .indicators
        .ema20
        .iter()
        .filter(|v| v.is_some())
        .count();
    let atr_ready = frame
        .indicators
        .atr14
        .iter()
        .filter(|v| v.is_some())
        .count();
    if ema_ready == 0 || atr_ready == 0 {
        return Err("EMA20/ATR14 预热不足".into());
    }
    for bar in bars {
        if !bar.high.is_finite() || !bar.low.is_finite() || bar.high < bar.low {
            return Err("存在非法 OHLC 数据".into());
        }
    }
    Ok(())
}

pub fn judge_data_sufficiency(frame: &KlineFrame) -> NodeFill {
    NodeFill {
        node_id: "1.1".into(),
        answer: "是",
        reason: format!(
            "共 {} 根已收盘 K 线，EMA20/ATR14 就绪，数据充分",
            frame.bars.len()
        ),
        bar_range: format!("K{}-K1", frame.bars.len()),
        branch: None,
        section: Some("是否可以决策"),
    }
}

pub fn judge_market_chaos(frame: &KlineFrame) -> NodeFill {
    let bars = &frame.bars;
    let w = ALWAYS_IN_NEAR_WINDOW.min(bars.len());
    let window_bars = &bars[..w];
    let atr = frame
        .indicators
        .atr14
        .first()
        .copied()
        .flatten()
        .unwrap_or(0.0);
    let slope = ema_slope(frame, 5).unwrap_or(0.0);
    let c1 = atr > 0.0 && slope.abs() < CHAOS_EMA_FLAT_ATR_RATIO * atr;
    let c2 = mean_overlap(window_bars).unwrap_or(0.0) >= CHAOS_OVERLAP_THRESHOLD;
    let bull_tb = window_bars.iter().filter(|b| is_trend_bull(b)).count();
    let bear_tb = window_bars.iter().filter(|b| is_trend_bear(b)).count();
    let tb_score: i32 = match (bull_tb, bear_tb) {
        (0, _) => -1,
        (_, 0) => 1,
        _ => 0,
    };
    let slope_sign: i32 = if slope > 0.0 {
        1
    } else if slope < 0.0 {
        -1
    } else {
        0
    };
    let c3 = (tb_score + slope_sign).abs() <= CHAOS_DIRECTION_SCORE_MAX;
    let chaos_score = c1 as i32 + c2 as i32 + c3 as i32;
    NodeFill {
        node_id: "1.3".into(),
        answer: "否",
        reason: format!(
            "混乱信号 {chaos_score}/3（EMA平坦={} 高重叠={} 无方向={}），未达极端混乱",
            c1 as u8, c2 as u8, c3 as u8
        ),
        bar_range: format!("K{w}-K1"),
        branch: None,
        section: Some("是否可以决策"),
    }
}

pub fn judge_momentum_strength(frame: &KlineFrame, direction: &str) -> NodeFill {
    let bars = &frame.bars;
    let w = ALWAYS_IN_NEAR_WINDOW.min(bars.len());
    let window_bars = &bars[..w];
    let atr = frame
        .indicators
        .atr14
        .first()
        .copied()
        .flatten()
        .unwrap_or(0.0);
    let bull_tb = window_bars.iter().filter(|b| is_trend_bull(b)).count();
    let bear_tb = window_bars.iter().filter(|b| is_trend_bear(b)).count();
    let total_tb = bull_tb + bear_tb;
    let m1_directional = match direction {
        "bullish" => bull_tb as f64 >= MOMENTUM_TREND_RATIO_STRONG * (bear_tb.max(1) as f64),
        "bearish" => bear_tb as f64 >= MOMENTUM_TREND_RATIO_STRONG * (bull_tb.max(1) as f64),
        _ => false,
    };
    let m1 = total_tb as f64 / w as f64 >= MOMENTUM_TREND_BAR_MIN_RATIO && m1_directional;
    let m2 = mean_overlap(window_bars).unwrap_or(1.0) < MOMENTUM_OVERLAP_WEAK;
    let max_close = window_bars.iter().map(|b| b.close).fold(f64::MIN, f64::max);
    let min_close = window_bars.iter().map(|b| b.close).fold(f64::MAX, f64::min);
    let m3 = atr > 0.0 && (max_close - min_close) / atr <= MOMENTUM_PULLBACK_DEEP_ATR;
    let strong = m1 as i32 + m2 as i32 + m3 as i32;
    let (answer, branch) = if strong >= 2 {
        ("是", None)
    } else if strong == 1 {
        ("中性", Some("broad_channel".to_string()))
    } else {
        ("否", None)
    };
    NodeFill {
        node_id: "2.5".into(),
        answer,
        reason: format!("惯性三信号（趋势棒占优={m1} 低重叠={m2} 浅回撤={m3}），命中 {strong}/3"),
        bar_range: format!("K{w}-K1"),
        branch,
        section: Some("多空方向判断"),
    }
}

// ---------------------------------------------------------------------------
// 趋势上下文（trend_context）
// ---------------------------------------------------------------------------

pub fn compute_trend_context(frame: &KlineFrame, trading_direction: &str) -> Value {
    let bars = &frame.bars;
    // 背景：K{n}-K41 子序列（跳过最近 40 根）
    let background_direction = if bars.len() > 40 {
        let background = &bars[40..];
        let (score, _) = direction_vote_frame(background, 30.min(background.len()));
        if score >= 2 {
            "bullish"
        } else if score <= -2 {
            "bearish"
        } else {
            "neutral"
        }
    } else {
        "neutral"
    };
    // 近端尖峰检测
    let near = &bars[..SPIKE_NEAR_WINDOW.min(bars.len())];
    let trend_bars = near
        .iter()
        .filter(|b| is_trend_bull(b) || is_trend_bear(b))
        .count();
    let overlap = mean_overlap(near).unwrap_or(1.0);
    let bull_tb = near.iter().filter(|b| is_trend_bull(b)).count();
    let bear_tb = near.iter().filter(|b| is_trend_bear(b)).count();
    let recent_spike = if trend_bars >= SPIKE_MIN_TREND_BARS
        && overlap <= SPIKE_OVERLAP_MAX
        && (bull_tb as i32 - bear_tb as i32).abs() >= 2
    {
        if bull_tb > bear_tb {
            "bullish"
        } else {
            "bearish"
        }
    } else {
        ""
    };
    let conflict = background_direction != "neutral"
        && !background_direction.is_empty()
        && trading_direction != "neutral"
        && background_direction != trading_direction;
    let relationship = if background_direction == "neutral" {
        "neutral_background"
    } else if conflict {
        "conflict"
    } else if trading_direction != "neutral" {
        "aligned"
    } else {
        "mixed"
    };
    json!({
        "background_direction": background_direction,
        "trading_direction": trading_direction,
        "primary_direction": trading_direction,
        "conflict": conflict,
        "relationship": relationship,
        "recent_spike": if recent_spike.is_empty() { Value::Null } else { json!(recent_spike) },
        "with_trend_rule": "背景与交易方向冲突时只做顺势回调，不逆势抄底摸顶",
    })
}

const SPIKE_NEAR_WINDOW: usize = 8;
const SPIKE_MIN_TREND_BARS: usize = 3;
const SPIKE_OVERLAP_MAX: f64 = 0.35;

/// 对任意子序列运行方向投票（子序列也要求新→旧）。
fn direction_vote_frame(bars: &[KlineBar], window: usize) -> (i32, [i32; 5]) {
    let synthetic = KlineFrame {
        symbol: String::new(),
        timeframe: String::new(),
        bars: bars.to_vec(),
        indicators: super::indicators::compute_indicators(bars),
        snapshot_ts_local_ms: 0,
    };
    direction_vote(&synthetic, window)
}

// ---------------------------------------------------------------------------
// §9.1-9.5 / §11（阶段二）
// ---------------------------------------------------------------------------

fn find_bar_by_label(frame: &KlineFrame, label: Option<&str>) -> Option<usize> {
    let label = label?;
    let text = label.replace(['K', 'k'], "");
    let seq: u32 = text.trim().parse().ok()?;
    frame.bars.iter().position(|bar| bar.seq == seq)
}

pub fn judge_signal_bar_nodes(frame: &KlineFrame, stage2: &mut Value) -> Vec<NodeFill> {
    let mut fills = Vec::new();
    let signal_label = stage2
        .pointer("/bar_analysis/signal_bar")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let order_direction = stage2
        .pointer("/decision/order_direction")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let order_type = stage2
        .pointer("/decision/order_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // §9.1 信号棒已收盘（锁定）
    let closed = signal_label
        .as_deref()
        .and_then(|label| find_bar_by_label(frame, Some(label)))
        .is_some();
    let label_text = signal_label.clone().unwrap_or_default();
    fills.push(NodeFill {
        node_id: "9.1".into(),
        answer: if closed { "是" } else { "否" },
        reason: if closed {
            format!("信号棒 K{label_text} 为已收盘 K 线（seq≥1）")
        } else {
            "信号棒缺失或未收盘".into()
        },
        bar_range: if label_text.is_empty() {
            "不适用".into()
        } else {
            format!("K{label_text}")
        },
        branch: None,
        section: Some("入场信号"),
    });

    if let (Some(label), Some(index)) = (
        signal_label.as_deref(),
        signal_label
            .as_deref()
            .and_then(|label| find_bar_by_label(frame, Some(label))),
    ) {
        let bar = &frame.bars[index];
        let atr = frame
            .indicators
            .atr14
            .get(index)
            .copied()
            .flatten()
            .unwrap_or(0.0);
        // §9.2 方向一致
        let consistent = match order_direction.as_str() {
            "做多" => bar.close > bar.open,
            "做空" => bar.close < bar.open,
            _ => true,
        };
        fills.push(NodeFill {
            node_id: "9.2".into(),
            answer: if order_type == "不下单" || consistent {
                "是"
            } else {
                "否"
            },
            reason: format!(
                "信号棒 K{label} {}，与 {} 方向{}",
                bar.candle_label(),
                order_direction,
                if consistent { "一致" } else { "冲突" }
            ),
            bar_range: format!("K{label}"),
            branch: None,
            section: Some("入场信号"),
        });
        // §9.3 信号棒不过长
        let ratio = if atr > 0.0 {
            (bar.high - bar.low) / atr
        } else {
            0.0
        };
        let not_long = ratio <= SIGNAL_BAR_LONG_ATR_RATIO;
        fills.push(NodeFill {
            node_id: "9.3".into(),
            answer: if not_long { "是" } else { "否" },
            reason: format!("信号棒振幅 {ratio:.2}×ATR（阈值 {SIGNAL_BAR_LONG_ATR_RATIO}）"),
            bar_range: format!("K{label}"),
            branch: None,
            section: Some("入场信号"),
        });
        // §9.5 跟随
        let follow = frame
            .bars
            .get(index.checked_sub(1).unwrap_or(index))
            .map(|next| {
                let same = (next.close - next.open).signum() == (bar.close - bar.open).signum();
                if index == 0 {
                    "pending"
                } else if same {
                    "yes"
                } else {
                    "no"
                }
            })
            .unwrap_or("pending");
        fills.push(NodeFill {
            node_id: "9.5".into(),
            answer: match follow {
                "yes" => "是",
                "no" => "否",
                _ => "等待",
            },
            reason: format!("信号棒跟随状态：{follow}"),
            bar_range: format!("K{label}"),
            branch: None,
            section: Some("入场信号"),
        });
    }
    fills
}

/// §11 下单方式路由（可被 AI 覆盖）。
pub fn route_order_method(frame: &KlineFrame, stage1: &Value, stage2: &mut Value) -> Vec<NodeFill> {
    let cycle = stage1
        .get("cycle_position")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let spike_stage = stage1.get("spike_stage").and_then(Value::as_str);
    let order_type = stage2
        .pointer("/decision/order_type")
        .and_then(Value::as_str)
        .unwrap_or("不下单")
        .to_string();
    let trade_equation_passed = stage2
        .get("decision_trace")
        .and_then(Value::as_array)
        .map(|trace| {
            trace.iter().any(|item| {
                item.get("node_id").and_then(Value::as_str) == Some("10.3")
                    && item.get("answer").and_then(Value::as_str) == Some("是")
            })
        })
        .unwrap_or(false);
    let no_order = order_type == "不下单";
    let mut fills = Vec::new();
    let recommend = |cycle: &str, spike_stage: Option<&str>| -> &'static str {
        match cycle {
            "spike" if spike_stage == Some("transitioning") => "突破单",
            "spike" => "市价单",
            "micro_channel" | "tight_channel" | "normal_channel" | "trending_tr" => "突破单",
            "broad_channel" | "trading_range" => "限价单",
            _ => "不下单",
        }
    };
    let recommended = recommend(cycle, spike_stage);
    let final_method = if no_order || recommended == "不下单" {
        "不下单".to_string()
    } else if recommended == "突破单" && order_type == "突破单" {
        let has_basis = stage2
            .pointer("/decision/entry_basis_bar")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
        if has_basis {
            "突破单".to_string()
        } else if trade_equation_passed {
            "限价单".to_string()
        } else {
            "不下单".to_string()
        }
    } else if recommended == "市价单" && order_type != "市价单" {
        // spike 通道内回调时保留其他类型
        order_type.clone()
    } else {
        order_type.clone()
    };
    let node_id = match recommended {
        "市价单" => "11.1",
        "突破单" => "11.2",
        "限价单" => "11.3",
        _ => "11.4",
    };
    fills.push(NodeFill {
        node_id: node_id.into(),
        answer: if final_method == "不下单" {
            "否"
        } else {
            "是"
        },
        reason: format!(
            "周期 {cycle} 路由建议 {recommended}，最终 {final_method}（AI 给出 {order_type}）"
        ),
        bar_range: "K1".into(),
        branch: Some(final_method.clone()),
        section: Some("下单方式"),
    });
    if let Some(decision) = stage2.get_mut("decision") {
        decision["order_type"] = json!(final_method);
        if final_method == "不下单" {
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
                decision[field] = Value::Null;
            }
        }
    }
    let _ = frame;
    fills
}

// ---------------------------------------------------------------------------
// 覆盖仲裁 + trace 合并
// ---------------------------------------------------------------------------

#[allow(dead_code)] // PA 全量移植保留：覆盖仲裁强度序（与 Python 版对齐）
fn conservative_rank(answer: &str) -> i32 {
    match answer {
        "是" => 5,
        "中性" | "等待" => 3,
        "否" => 1,
        _ => 0,
    }
}

pub fn apply_overrides(
    stage: &str,
    program_nodes: &[NodeFill],
    node_overrides: &[Value],
    out: &mut Value,
) -> Vec<String> {
    let mut notes = Vec::new();
    for entry in node_overrides {
        let Some(node_id) = entry.get("node_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(answer) = entry.get("answer").and_then(Value::as_str) else {
            continue;
        };
        let reason = entry
            .get("override_reason")
            .and_then(Value::as_str)
            .unwrap_or("AI 覆盖");
        if LOCKED_NODES.contains(&node_id) {
            notes.push(format!("§{node_id} 为锁定节点，拒绝覆盖"));
            continue;
        }
        if SAFETY_GATE_NODES.contains(&node_id) {
            notes.push(format!("§{node_id} 为安全闸门，仅允许更保守覆盖"));
            continue;
        }
        let Some(fill) = program_nodes.iter().find(|fill| fill.node_id == node_id) else {
            continue;
        };
        if stage == "stage1" && node_id == "2.3" {
            let branch = entry.get("branch").and_then(Value::as_str).unwrap_or("");
            let consistent = match answer {
                "是" => branch == "bullish" || branch == "bearish",
                "中性" => branch == "neutral",
                _ => false,
            };
            if !consistent {
                notes.push(format!(
                    "§2.3 覆盖 answer({answer}) 与 branch({branch}) 不一致，拒绝"
                ));
                continue;
            }
        }
        let trace_key = if stage == "stage1" {
            "gate_trace"
        } else {
            "decision_trace"
        };
        if let Some(trace) = out.get_mut(trace_key).and_then(Value::as_array_mut) {
            for item in trace.iter_mut() {
                if item.get("node_id").and_then(Value::as_str) == Some(node_id) {
                    item["program_answer"] = json!(fill.answer);
                    item["program_branch"] = fill
                        .branch
                        .as_ref()
                        .map(|b| json!(b))
                        .unwrap_or(Value::Null);
                    item["override_reason"] = json!(reason);
                    item["overridden_by_ai"] = json!(true);
                    item["answer"] = json!(answer);
                    if let Some(branch) = entry.get("branch").and_then(Value::as_str) {
                        item["branch"] = json!(branch);
                    }
                    break;
                }
            }
        }
        notes.push(format!("§{node_id} 被 AI 覆盖为 {answer}"));
    }
    notes
}

/// 把程序节点合并进 AI trace：按 node_id 数值排序；AI 覆盖优先保留其填写的字段。
pub fn merge_program_nodes(trace_key: &str, out: &mut Value, fills: &[NodeFill]) {
    let Some(trace) = out.get_mut(trace_key).and_then(Value::as_array_mut) else {
        let items: Vec<Value> = fills
            .iter()
            .map(|fill| fill.to_trace_value(&node_question(&fill.node_id)))
            .collect();
        out[trace_key] = json!(items);
        return;
    };
    for fill in fills {
        let existing = trace.iter_mut().find(|item| {
            item.get("node_id").and_then(Value::as_str) == Some(fill.node_id.as_str())
        });
        match existing {
            Some(item) => {
                if item.get("overridden_by_ai").and_then(Value::as_bool) != Some(true) {
                    if item
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .is_empty()
                    {
                        item["reason"] = json!(fill.reason);
                    }
                    if (item.get("branch").is_none() || item.get("branch") == Some(&Value::Null))
                        && let Some(branch) = &fill.branch
                    {
                        item["branch"] = json!(branch);
                    }
                    item["program_answer"] = json!(fill.answer);
                }
            }
            None => {
                let mut value = fill.to_trace_value(&node_question(&fill.node_id));
                if let Some(entry) = fills.iter().find(|f| f.node_id == fill.node_id) {
                    value["program_answer"] = json!(entry.answer);
                }
                trace.push(value);
            }
        }
    }
    trace.sort_by(|a, b| {
        let ka = a.get("node_id").and_then(Value::as_str).map(node_sort_key);
        let kb = b.get("node_id").and_then(Value::as_str).map(node_sort_key);
        ka.cmp(&kb)
    });
}

/// 阶段一引擎入口：填充程序节点 + 同步 direction + trend_context。
pub fn apply_stage1(out: &mut Value, frame: &KlineFrame) {
    if check_preflight_data(frame).is_ok() {
        let sufficiency = judge_data_sufficiency(frame);
        merge_program_nodes("gate_trace", out, &[sufficiency]);
    }
    let chaos = judge_market_chaos(frame);
    let (direction, direction_fill) = judge_direction(frame);
    let always_in = judge_always_in(frame);
    let momentum = judge_momentum_strength(frame, &direction);
    let fills = vec![chaos, direction_fill, always_in, momentum];
    merge_program_nodes("gate_trace", out, &fills);

    // 同步 direction（程序判定优先于 AI 值）
    out["direction"] = json!(direction);
    let trading_direction = direction;
    out["trend_context"] = compute_trend_context(frame, &trading_direction);

    // gate_result 修复：无有效阻断且 AI 给 wait/unknown → proceed
    let trace = out
        .get("gate_trace")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let blocked = trace.iter().any(|item| {
        matches!(
            (
                item.get("node_id").and_then(Value::as_str),
                item.get("answer").and_then(Value::as_str)
            ),
            (Some("1.2"), Some("否")) | (Some("1.3"), Some("是"))
        )
    });
    let gate_result = out
        .get("gate_result")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if !blocked && (gate_result == "wait" || gate_result == "unknown") {
        out["gate_result"] = json!("proceed");
    }
}

/// 阶段二引擎入口。
pub fn apply_stage2(out: &mut Value, frame: &KlineFrame, stage1: &Value) {
    let signal_fills = judge_signal_bar_nodes(frame, out);
    merge_program_nodes("decision_trace", out, &signal_fills);
    let method_fills = route_order_method(frame, stage1, out);
    merge_program_nodes("decision_trace", out, &method_fills);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::indicators::compute_indicators;
    use crate::market::types::KlineFrame;

    fn synthetic_frame(closes: &[f64]) -> KlineFrame {
        let n = closes.len();
        let mut bars: Vec<KlineBar> = closes
            .iter()
            .rev()
            .enumerate()
            .map(|(index, &close)| {
                let drift = (n - index) as f64 * 0.05;
                KlineBar {
                    seq: 0,
                    ts_open: 1_700_000_000_000.0 + index as f64 * 900_000.0,
                    open: close - drift - 0.4,
                    high: close + 0.8,
                    low: close - drift - 1.1,
                    close,
                    volume: 10.0,
                    closed: true,
                }
            })
            .collect();
        for (index, bar) in bars.iter_mut().enumerate() {
            bar.seq = (index + 1) as u32;
        }
        let indicators = compute_indicators(&bars);
        KlineFrame {
            symbol: "TEST".into(),
            timeframe: "15m".into(),
            bars,
            indicators,
            snapshot_ts_local_ms: 1,
        }
    }

    #[test]
    fn preflight_requires_twenty_bars() {
        let frame = synthetic_frame(&[100.0; 15]);
        assert!(check_preflight_data(&frame).is_err());
        let frame = synthetic_frame(&[100.0; 60]);
        assert!(check_preflight_data(&frame).is_ok());
    }

    #[test]
    fn rising_market_votes_bullish() {
        let closes: Vec<f64> = (0..80).map(|i| 100.0 + i as f64 * 1.2).collect();
        let frame = synthetic_frame(&closes);
        let (direction, fill) = judge_direction(&frame);
        assert_eq!(direction, "bullish");
        assert_eq!(fill.answer, "是");
        let chaos = judge_market_chaos(&frame);
        assert_eq!(chaos.answer, "否");
    }

    #[test]
    fn flat_market_votes_neutral() {
        let closes: Vec<f64> = (0..80)
            .map(|i| 100.0 + ((i % 7) as f64 - 3.0) * 0.1)
            .collect();
        let frame = synthetic_frame(&closes);
        let (direction, fill) = judge_direction(&frame);
        assert_eq!(direction, "neutral");
        assert_eq!(fill.answer, "中性");
    }

    #[test]
    fn stage1_engine_syncs_direction_and_repairs_gate() {
        let closes: Vec<f64> = (0..80).map(|i| 100.0 + i as f64).collect();
        let frame = synthetic_frame(&closes);
        let mut out = serde_json::json!({
            "direction": "bearish",
            "gate_trace": [],
            "gate_result": "wait",
        });
        apply_stage1(&mut out, &frame);
        assert_eq!(out["direction"], "bullish");
        assert_eq!(out["gate_result"], "proceed");
        let trace = out["gate_trace"].as_array().unwrap();
        assert!(trace.iter().any(|item| item["node_id"] == "1.1"));
        assert!(trace.iter().any(|item| item["node_id"] == "2.3"));
    }

    #[test]
    fn order_method_routes_by_cycle() {
        let frame = synthetic_frame(&[100.0; 60]);
        let stage1 = serde_json::json!({"cycle_position": "tight_channel", "direction": "bullish"});
        let mut stage2 = serde_json::json!({
            "decision": {"order_type": "突破单", "order_direction": "做多",
                          "entry_basis_bar": "K3", "entry_basis_extreme": "high"},
            "decision_trace": [{"node_id": "10.3", "answer": "是"}],
        });
        let fills = route_order_method(&frame, &stage1, &mut stage2);
        assert_eq!(stage2["decision"]["order_type"], "突破单");
        assert_eq!(fills[0].node_id, "11.2");
    }
}
