//! K 线几何特征计算（16 项），与 Python ai/kline_features.py 公式逐条对齐。
//! 输入 frame.bars 为新→旧；输出与 bars 同序。

use super::types::{GeometryFeature, KlineBar, KlineFrame};

fn classification_bar_type(bar: &KlineBar, prev: Option<&KlineBar>) -> &'static str {
    let range = bar.high - bar.low;
    if range <= 0.0 {
        return "flat";
    }
    if let Some(prev) = prev {
        if bar.high <= prev.high && bar.low >= prev.low {
            return "inside";
        }
        if bar.high >= prev.high && bar.low <= prev.low {
            return if bar.close >= bar.open {
                "outside_bull"
            } else {
                "outside_bear"
            };
        }
    }
    let body_ratio = (bar.close - bar.open).abs() / range;
    if body_ratio <= 0.25 {
        return "doji";
    }
    let close_pos = ((bar.close - bar.low) / range).clamp(0.0, 1.0);
    if bar.close > bar.open && close_pos >= 0.65 {
        return "trend_bull";
    }
    if bar.close < bar.open && close_pos <= 0.35 {
        return "trend_bear";
    }
    "other"
}

fn inside_sequence(bars: &[KlineBar], index: usize) -> &'static str {
    // 连续内包计数：当前内包前棒、前棒内包前前棒……
    let mut count = 0usize;
    let mut current = index;
    while current + 1 < bars.len() {
        let bar = &bars[current];
        let prev = &bars[current + 1];
        if bar.high <= prev.high && bar.low >= prev.low {
            count += 1;
            current += 1;
        } else {
            break;
        }
    }
    match count {
        0 => "none",
        1 => "single_inside",
        2 => "ii",
        _ => "iii",
    }
}

fn ioi_pattern(bars: &[KlineBar], index: usize) -> bool {
    // prev2 inside prev3, prev outside prev2, cur inside prev
    if index + 3 >= bars.len() {
        return false;
    }
    let cur = &bars[index];
    let prev = &bars[index + 1];
    let prev2 = &bars[index + 2];
    let prev3 = &bars[index + 3];
    let inside = |a: &KlineBar, b: &KlineBar| a.high <= b.high && a.low >= b.low;
    let outside = |a: &KlineBar, b: &KlineBar| a.high >= b.high && a.low <= b.low;
    inside(prev2, prev3) && outside(prev, prev2) && inside(cur, prev)
}

fn micro_double(bar: &KlineBar, prev: &KlineBar, atr: Option<f64>) -> &'static str {
    let Some(atr) = atr else { return "none" };
    if atr <= 0.0 {
        return "none";
    }
    let threshold = atr * 0.02;
    if (bar.low - prev.low).abs() <= threshold {
        return "MDB";
    }
    if (bar.high - prev.high).abs() <= threshold {
        return "MDT";
    }
    "none"
}

fn gap_state(bar: &KlineBar, ema: Option<f64>) -> &'static str {
    match ema {
        Some(ema) if ema.is_finite() => {
            if bar.low > ema {
                "bull_gap"
            } else if bar.high < ema {
                "bear_gap"
            } else {
                "none"
            }
        }
        _ => "none",
    }
}

fn ema_gap_count(bars: &[KlineBar], emas: &[Option<f64>], index: usize) -> Option<u32> {
    let state = gap_state(&bars[index], emas.get(index).copied().flatten());
    if state == "none" {
        return Some(0);
    }
    let mut count = 1u32;
    let mut cursor = index + 1;
    while cursor < bars.len() {
        if gap_state(&bars[cursor], emas.get(cursor).copied().flatten()) == state {
            count += 1;
            cursor += 1;
        } else {
            break;
        }
    }
    Some(count)
}

fn breakout_prev(bars: &[KlineBar], index: usize, lookback: usize) -> &'static str {
    let end = (index + 1 + lookback).min(bars.len());
    if index + 1 >= bars.len() {
        return "none";
    }
    let window = &bars[index + 1..end];
    if window.is_empty() {
        return "none";
    }
    let highest = window.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let lowest = window.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    let up = bars[index].high > highest;
    let down = bars[index].low < lowest;
    match (up, down) {
        (true, true) => "both",
        (true, false) => "up",
        (false, true) => "down",
        (false, false) => "none",
    }
}

fn follow_through(bars: &[KlineBar], index: usize) -> &'static str {
    // K(idx) 之后 1-2 根更新棒的收盘是否延续方向；idx=0（K1 最新）永远 pending
    if index == 0 {
        return "pending";
    }
    let bar = &bars[index];
    let direction: f64 = if bar.close >= bar.open { 1.0 } else { -1.0 };
    let next1 = &bars[index - 1];
    let next2 = if index >= 2 {
        Some(&bars[index - 2])
    } else {
        None
    };
    let continuation = |probe: &KlineBar| (probe.close - probe.open).signum() == direction;
    match (continuation(next1), next2.map(continuation)) {
        (true, Some(true)) => "yes",
        (true, _) => "yes",
        (false, Some(false)) => "failed",
        (false, Some(true)) => "yes",
        (false, None) => "pending",
    }
}

pub fn compute_geometry_features(frame: &KlineFrame, limit: Option<usize>) -> Vec<GeometryFeature> {
    let bars = &frame.bars;
    let emas = &frame.indicators.ema20;
    let atrs = &frame.indicators.atr14;
    let n = limit.unwrap_or(bars.len()).min(bars.len());
    let mut features = Vec::with_capacity(n);
    for index in 0..n {
        let bar = &bars[index];
        let prev = bars.get(index + 1);
        let atr = atrs
            .get(index)
            .copied()
            .flatten()
            .filter(|v| v.is_finite() && *v > 0.0);
        let ema = emas.get(index).copied().flatten().filter(|v| v.is_finite());
        let range = bar.high - bar.low;
        let body = (bar.close - bar.open).abs();

        let body_ratio = (range > 0.0).then(|| body / range);
        let upper_wick = (range > 0.0).then(|| (bar.high - bar.open.max(bar.close)) / range);
        let lower_wick = (range > 0.0).then(|| (bar.open.min(bar.close) - bar.low) / range);
        let close_position = (range > 0.0).then(|| ((bar.close - bar.low) / range).clamp(0.0, 1.0));
        let range_atr_ratio = atr.map(|a| range / a);

        let ema_relation = match ema {
            Some(ema) if bar.close > ema => "above",
            Some(ema) if bar.close < ema => "below",
            Some(_) => "touch",
            None => "unknown",
        };

        let overlap_prev_ratio = prev.map(|prev| {
            let union_high = bar.high.max(prev.high);
            let union_low = bar.low.min(prev.low);
            let union = union_high - union_low;
            if union <= 0.0 {
                None
            } else {
                Some(((bar.high.min(prev.high) - bar.low.max(prev.low)).max(0.0)) / union)
            }
        });

        features.push(GeometryFeature {
            seq: bar.seq,
            bar_type: classification_bar_type(bar, prev).into(),
            body_ratio,
            upper_wick_ratio: upper_wick,
            lower_wick_ratio: lower_wick,
            close_position,
            range_atr_ratio,
            ema_relation: ema_relation.into(),
            overlap_prev_ratio: overlap_prev_ratio.flatten(),
            inside_sequence: inside_sequence(bars, index).into(),
            ioi_pattern: ioi_pattern(bars, index),
            micro_double: prev
                .map(|p| micro_double(bar, p, atr))
                .unwrap_or("none")
                .into(),
            gap_bar: gap_state(bar, ema).into(),
            ema_gap_count: ema_gap_count(bars, emas, index),
            breakout_prev: breakout_prev(bars, index, 5).into(),
            follow_through_1_2: follow_through(bars, index).into(),
        });
    }
    features
}

/// 品种最小报价单位推断：扫描 OHLC 小数位数，10^-max_decimals（上限 6 位）。
pub fn infer_price_tick(frame: &KlineFrame) -> f64 {
    let mut max_decimals = 0u32;
    for bar in &frame.bars {
        for price in [bar.open, bar.high, bar.low, bar.close] {
            let text = format!("{price:.10}");
            let trimmed = text.trim_end_matches('0');
            let decimals = trimmed
                .find('.')
                .map(|dot| trimmed.len() - dot - 1)
                .unwrap_or(0) as u32;
            max_decimals = max_decimals.max(decimals);
        }
    }
    10f64.powi(-(max_decimals.min(6) as i32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::indicators::compute_indicators;

    fn frame(closes: &[f64]) -> KlineFrame {
        let bars: Vec<KlineBar> = closes
            .iter()
            .enumerate()
            .rev()
            .map(|(i, &c)| KlineBar {
                seq: 0,
                ts_open: 1700000000000.0 + i as f64 * 900000.0,
                open: c - 1.0,
                high: c + 2.0,
                low: c - 2.5,
                close: c,
                volume: 10.0,
                closed: true,
            })
            .collect();
        let mut bars = bars;
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
    fn trend_bar_detection_uses_close_position() {
        // 构造强阳线：close_position = 0.9
        let mut f = frame(&[100.0; 40]);
        let newest = &mut f.bars[0];
        newest.open = 10.0;
        newest.low = 10.0;
        newest.high = 20.0;
        newest.close = 19.0;
        let features = compute_geometry_features(&f, Some(1));
        assert_eq!(features[0].bar_type, "trend_bull");
    }

    #[test]
    fn inside_bar_detected_against_prev() {
        let mut f = frame(&[100.0; 40]);
        let newest = &mut f.bars[0];
        newest.high = 100.5;
        newest.low = 99.5;
        newest.open = 100.0;
        newest.close = 100.2;
        let features = compute_geometry_features(&f, Some(1));
        assert_eq!(features[0].bar_type, "inside");
    }

    #[test]
    fn tick_inferred_from_decimal_digits() {
        let f = frame(&[2345.12, 2345.68, 2346.01]);
        assert!((infer_price_tick(&f) - 0.01).abs() < 1e-12);
    }
}
