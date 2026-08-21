//! 指标计算与快照构建（与 Python indicators/ema.py、atr.py、snapshot.py 对齐）。
//! EMA/ATR 种子 = 前 period 个值的简单平均；ATR 用 Wilder 平滑。
//! bars 为新→旧排列；计算时反转成旧→新再算回来。

use super::types::{INDICATOR_WARMUP_BARS, IndicatorBundle, KlineBar, KlineFrame};

pub fn ema_full(values: &[f64], period: usize) -> Vec<Option<f64>> {
    let n = values.len();
    let mut result = vec![None; n];
    if n < period || period == 0 {
        return result;
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let seed: f64 = values[..period].iter().sum();
    result[period - 1] = Some(seed / period as f64);
    for i in period..n {
        let previous = result[i - 1].expect("seed exists");
        result[i] = Some(values[i] * alpha + previous * (1.0 - alpha));
    }
    result
}

fn true_range(high: f64, low: f64, prev_close: Option<f64>) -> f64 {
    match prev_close {
        Some(pc) => (high - low)
            .abs()
            .max((high - pc).abs())
            .max((low - pc).abs()),
        None => (high - low).abs(),
    }
}

pub fn atr_full(highs: &[f64], lows: &[f64], closes: &[f64], period: usize) -> Vec<Option<f64>> {
    let n = closes.len();
    let mut result = vec![None; n];
    if n < period || period == 0 {
        return result;
    }
    let tr: Vec<f64> = (0..n)
        .map(|i| {
            true_range(
                highs[i],
                lows[i],
                if i > 0 { Some(closes[i - 1]) } else { None },
            )
        })
        .collect();
    let seed: f64 = tr[..period].iter().sum();
    result[period - 1] = Some(seed / period as f64);
    for i in period..n {
        let previous = result[i - 1].expect("seed exists");
        result[i] = Some((previous * (period as f64 - 1.0) + tr[i]) / period as f64);
    }
    result
}

/// 输入 bars 为新→旧，输出指标与新→旧对齐。
pub fn compute_indicators(bars: &[KlineBar]) -> IndicatorBundle {
    let mut closes = Vec::with_capacity(bars.len());
    let mut highs = Vec::with_capacity(bars.len());
    let mut lows = Vec::with_capacity(bars.len());
    for bar in bars.iter().rev() {
        closes.push(bar.close);
        highs.push(bar.high);
        lows.push(bar.low);
    }
    let ema20 = ema_full(&closes, 20);
    let atr14 = atr_full(&highs, &lows, &closes, 14);
    IndicatorBundle {
        ema20: ema20.into_iter().rev().collect(),
        atr14: atr14.into_iter().rev().collect(),
    }
}

fn forming_at_head(bars: &[KlineBar]) -> bool {
    bars.first().is_some_and(|bar| !bar.closed)
}

/// 分析快照：只含已收盘棒，seq 重编为 1..=n，附带指标。
pub fn build_analysis_frame(
    bars_raw: &[KlineBar],
    n: usize,
    symbol: &str,
    timeframe: &str,
    now_ms: u64,
) -> Option<KlineFrame> {
    let forming = forming_at_head(bars_raw);
    let head = usize::from(forming);
    let avail_closed = bars_raw.len().saturating_sub(head);
    if avail_closed < n {
        return None;
    }
    let fetch_n = (n + INDICATOR_WARMUP_BARS).min(avail_closed);
    let selected: Vec<KlineBar> = bars_raw[head..head + fetch_n]
        .iter()
        .enumerate()
        .map(|(index, bar)| {
            let mut bar = *bar;
            bar.seq = (index + 1) as u32;
            bar.closed = true;
            bar
        })
        .collect();
    let indicators = compute_indicators(&selected);
    Some(KlineFrame {
        symbol: symbol.into(),
        timeframe: timeframe.into(),
        bars: selected,
        indicators,
        snapshot_ts_local_ms: now_ms,
    })
}

/// 图表帧：forming 棒（seq=0）+ n_closed 根已收盘棒。
pub fn build_live_frame(
    bars_raw: &[KlineBar],
    n_closed: usize,
    symbol: &str,
    timeframe: &str,
    now_ms: u64,
) -> Option<KlineFrame> {
    if bars_raw.is_empty() {
        return None;
    }
    let forming = forming_at_head(bars_raw);
    let start = usize::from(forming);
    let avail_closed = bars_raw.len().saturating_sub(start);
    let take = avail_closed.min(n_closed + INDICATOR_WARMUP_BARS);
    let mut bars: Vec<KlineBar> = Vec::with_capacity(take + usize::from(forming));
    if let Some(head) = bars_raw.first().filter(|_| forming) {
        let mut head = *head;
        head.seq = 0;
        bars.push(head);
    }
    for (index, bar) in bars_raw[start..start + take].iter().enumerate() {
        let mut bar = *bar;
        bar.seq = (index + 1) as u32;
        bars.push(bar);
    }
    let indicators = compute_indicators(&bars);
    Some(KlineFrame {
        symbol: symbol.into(),
        timeframe: timeframe.into(),
        bars,
        indicators,
        snapshot_ts_local_ms: now_ms,
    })
}

/// 从持久化 kline_data 重建分析帧（演示/增量复用）。
pub fn frame_from_records(
    bars: &[KlineBar],
    symbol: &str,
    timeframe: &str,
    now_ms: u64,
) -> KlineFrame {
    let mut normalized: Vec<KlineBar> = bars.iter().map(|bar| bar.normalized()).collect();
    normalized.sort_by(|a, b| {
        b.ts_open
            .partial_cmp(&a.ts_open)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (index, bar) in normalized.iter_mut().enumerate() {
        bar.seq = (index + 1) as u32;
    }
    let indicators = compute_indicators(&normalized);
    KlineFrame {
        symbol: symbol.into(),
        timeframe: timeframe.into(),
        bars: normalized,
        indicators,
        snapshot_ts_local_ms: now_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(seq: u32, close: f64) -> KlineBar {
        KlineBar {
            seq,
            ts_open: 1_700_000_000_000f64 + seq as f64 * 900_000.0,
            open: close - 1.0,
            high: close + 2.0,
            low: close - 2.0,
            close,
            volume: 100.0,
            closed: true,
        }
    }

    #[test]
    fn ema_seed_is_sma_of_first_period() {
        let values: Vec<f64> = (1..=30).map(|v| v as f64).collect();
        let ema = ema_full(&values, 20);
        assert!(ema[18].is_none());
        let seed = ema[19].unwrap();
        let expected: f64 = (1..=20).map(|v| v as f64).sum::<f64>() / 20.0;
        assert!((seed - expected).abs() < 1e-9);
        let alpha = 2.0 / 21.0;
        let next = ema[20].unwrap();
        assert!((next - (21.0 * alpha + seed * (1.0 - alpha))).abs() < 1e-9);
    }

    #[test]
    fn atr_uses_wilder_smoothing() {
        let closes = vec![10.0; 40];
        let highs: Vec<f64> = closes.iter().map(|c| c + 3.0).collect();
        let lows: Vec<f64> = closes.iter().map(|c| c - 1.0).collect();
        let atr = atr_full(&highs, &lows, &closes, 14);
        // TR 恒为 4（high-low=4），种子=4，后续保持 4
        assert!((atr[13].unwrap() - 4.0).abs() < 1e-9);
        assert!((atr[39].unwrap() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn analysis_frame_excludes_forming_and_rebases_seq() {
        // 供给必须 ≥ n + 预热根数（50），否则按可得数量截断。
        let mut bars: Vec<KlineBar> = (1..=100).rev().map(|i| bar(i, 100.0 + i as f64)).collect();
        bars.insert(
            0,
            KlineBar {
                seq: 0,
                closed: false,
                ..bar(0, 999.0)
            },
        );
        let frame = build_analysis_frame(&bars, 30, "XAUUSD", "15m", 1).expect("frame");
        assert_eq!(frame.bars.len(), 30 + INDICATOR_WARMUP_BARS);
        assert_eq!(frame.bars[0].seq, 1);
        assert!(frame.bars.iter().all(|b| b.closed));
        assert!(frame.indicators.ema20.len() == frame.bars.len());
    }

    #[test]
    fn live_frame_keeps_forming_head_with_seq_zero() {
        let mut bars: Vec<KlineBar> = (1..=60).rev().map(|i| bar(i, 100.0 + i as f64)).collect();
        bars.insert(
            0,
            KlineBar {
                seq: 0,
                closed: false,
                ..bar(0, 999.0)
            },
        );
        let frame = build_live_frame(&bars, 30, "XAUUSD", "15m", 1).expect("frame");
        assert_eq!(frame.bars[0].seq, 0);
        assert!(!frame.bars[0].closed);
        assert_eq!(frame.bars[1].seq, 1);
    }
}
