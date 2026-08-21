//! 市场数据源（移植自 pa_agent/data/）：
//! - yfinance：Yahoo v8 chart HTTP API（期货/加密/股票/指数）
//! - tradingview：非官方 ~m~ 帧协议 WebSocket（匿名）
//! - mt5：通过本机 Python MetaTrader5 包的进程桥（仅 Windows）

use super::types::{KlineBar, timeframe_to_seconds};
use crate::error::{KfResult, LocalizedError};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::{Duration, Instant};

pub const SOURCE_KINDS: &[&str] = &["tradingview", "yfinance", "mt5", "eastmoney"];

pub fn source_supported_timeframes(kind: &str) -> Vec<&'static str> {
    match kind {
        "yfinance" => vec!["1m", "5m", "15m", "30m", "1h", "1d", "1w"],
        "mt5" => vec!["1m", "5m", "15m", "30m", "1h", "4h", "1d"],
        "eastmoney" => vec!["5m", "15m", "30m", "1h", "1d", "1w"],
        _ => vec!["1m", "5m", "15m", "30m", "1h", "4h", "1d", "1w"],
    }
}

// ---------------------------------------------------------------------------
// yfinance（Yahoo v8 chart API）
// ---------------------------------------------------------------------------

fn yahoo_interval(timeframe: &str) -> Option<&'static str> {
    Some(match timeframe {
        "1m" => "1m",
        "2m" => "2m",
        "5m" => "5m",
        "15m" => "15m",
        "30m" => "30m",
        "1h" => "1h",
        "1d" => "1d",
        "1w" => "1wk",
        "1M" => "1mo",
        _ => return None,
    })
}

fn yahoo_range(timeframe: &str, bars: usize) -> &'static str {
    let seconds = timeframe_to_seconds(timeframe).unwrap_or(900);
    let span = seconds * bars.max(30) as u64;
    if span <= 60 * 60 * 24 * 7 {
        "7d"
    } else if span <= 60 * 60 * 24 * 60 {
        "60d"
    } else if span <= 60 * 60 * 24 * 730 {
        "2y"
    } else {
        "10y"
    }
}

pub async fn fetch_yfinance(
    client: &reqwest::Client,
    symbol: &str,
    timeframe: &str,
    n: usize,
    now_ms: u64,
) -> KfResult<Vec<KlineBar>> {
    let interval = yahoo_interval(timeframe)
        .ok_or_else(|| LocalizedError::new("error.market_timeframe").arg("timeframe", timeframe))?;
    let range = yahoo_range(timeframe, n);
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval={interval}&range={range}&includePrePost=false",
        urlencoding_lite(symbol),
    );
    let response = client
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) KnightFrame/0.1",
        )
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|error| LocalizedError::new("error.market_fetch").arg("detail", error))?;
    if !response.status().is_success() {
        return Err(
            LocalizedError::new("error.market_status").arg("status", response.status().as_u16())
        );
    }
    let value: Value = response
        .json()
        .await
        .map_err(|error| LocalizedError::new("error.market_decode").arg("detail", error))?;
    let result = value
        .pointer("/chart/result/0")
        .ok_or_else(|| LocalizedError::new("error.market_empty"))?;
    let timestamps: Vec<f64> = result
        .get("timestamp")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    let quote = result
        .pointer("/indicators/quote/0")
        .ok_or_else(|| LocalizedError::new("error.market_empty"))?;
    let extract = |field: &str| -> Vec<Option<f64>> {
        quote
            .get(field)
            .and_then(Value::as_array)
            .map(|items| items.iter().map(Value::as_f64).collect())
            .unwrap_or_default()
    };
    let opens = extract("open");
    let highs = extract("high");
    let lows = extract("low");
    let closes = extract("close");
    let volumes = extract("volume");
    if timestamps.is_empty() || closes.is_empty() {
        return Err(LocalizedError::new("error.market_empty"));
    }
    let duration_ms = (timeframe_to_seconds(timeframe).unwrap_or(900) * 1000) as f64;
    let mut bars: Vec<KlineBar> = timestamps
        .iter()
        .enumerate()
        .filter_map(|(index, ts)| {
            let close = closes.get(index).copied().flatten()?;
            let open = opens.get(index).copied().flatten().unwrap_or(close);
            let high = highs
                .get(index)
                .copied()
                .flatten()
                .unwrap_or(close.max(open));
            let low = lows
                .get(index)
                .copied()
                .flatten()
                .unwrap_or(close.min(open));
            let volume = volumes.get(index).copied().flatten().unwrap_or(0.0);
            Some(KlineBar {
                seq: 0,
                ts_open: ts * 1000.0,
                open,
                high,
                low,
                close,
                volume,
                closed: true,
            })
        })
        .map(|bar| bar.normalized())
        .collect();
    // 最新棒是否形成中
    if let Some(last) = bars.last_mut() {
        let elapsed = now_ms as f64 - last.ts_open;
        last.closed = elapsed >= duration_ms;
    }
    bars.sort_by(|a, b| {
        b.ts_open
            .partial_cmp(&a.ts_open)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bars.truncate(n + 1);
    rebase(&mut bars);
    Ok(bars)
}

fn urlencoding_lite(text: &str) -> String {
    // Yahoo 符号只需处理 '='（期货）与 '^'（指数）
    text.replace('=', "%3D").replace('^', "%5E")
}

// ---------------------------------------------------------------------------
// TradingView（~m~ 帧 WebSocket）
// ---------------------------------------------------------------------------

fn tv_interval(timeframe: &str) -> Option<&'static str> {
    Some(match timeframe {
        "1m" => "1",
        "3m" => "3",
        "5m" => "5",
        "15m" => "15",
        "30m" => "30",
        "45m" => "45",
        "1h" => "60",
        "2h" => "120",
        "3h" => "180",
        "4h" => "240",
        "1d" => "1D",
        "1w" => "1W",
        "1M" => "1M",
        _ => return None,
    })
}

fn tv_frame(payload: &str) -> String {
    format!("~m~{}~m~{}", payload.chars().count(), payload)
}

fn tv_json_message(method: &str, params: &Value) -> String {
    let payload = format!(
        "{{\"m\":\"{}\",\"p\":{}}}",
        method,
        serde_json::to_string(params).unwrap_or_default()
    );
    tv_frame(&payload)
}

/// 拆分 ~m~<len>~m~<data> 帧。
fn split_frames(raw: &str) -> Vec<String> {
    let mut frames = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find("~m~") {
        let after = &rest[start + 3..];
        let Some(len_end) = after.find("~m~") else {
            break;
        };
        let Ok(length) = after[..len_end].parse::<usize>() else {
            rest = after;
            continue;
        };
        let payload_start = &after[len_end + 3..];
        let end_position = payload_start
            .char_indices()
            .nth(length)
            .map(|(index, _)| index)
            .unwrap_or(payload_start.len());
        frames.push(payload_start[..end_position].to_string());
        rest = &payload_start[end_position..];
    }
    frames
}

fn random_session(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(1);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{prefix}{seed:x}{count:04x}")
}

pub async fn fetch_tradingview(
    symbol: &str,
    exchange: &str,
    timeframe: &str,
    n: usize,
    now_ms: u64,
) -> KfResult<Vec<KlineBar>> {
    let interval = tv_interval(timeframe)
        .ok_or_else(|| LocalizedError::new("error.market_timeframe").arg("timeframe", timeframe))?;
    // 连接超时收紧到 7s：被墙/不可达时尽快失败，让跨源兜底接管
    let (ws, _response) = match tokio::time::timeout(
        Duration::from_secs(7),
        tokio_tungstenite::connect_async(
            "wss://data.tradingview.com/socket.io/?EIO=4&transport=websocket",
        ),
    )
    .await
    {
        Ok(result) => result
            .map_err(|error| LocalizedError::new("error.market_tv_connect").arg("detail", error))?,
        Err(_) => {
            return Err(LocalizedError::new("error.market_tv_connect")
                .arg("detail", "connect timeout after 7s"));
        }
    };
    let (mut writer, mut reader) = ws.split();

    let chart_session = random_session("cs_");
    let quote_session = random_session("qs_");
    let symbol_id = "sds_sym_1";

    // TradingView 需要完整符号（OANDA:XAUUSD）；裸代码在多数源上无法解析
    let trimmed_exchange = exchange.trim();
    let full_symbol = if trimmed_exchange.is_empty()
        || symbol.contains(':')
        || symbol.eq_ignore_ascii_case(trimmed_exchange)
    {
        symbol.to_string()
    } else {
        format!("{trimmed_exchange}:{symbol}")
    };

    let mut init_sent = false;
    let symbol_spec = serde_json::json!(format!(
        "={{\"symbol\":\"{full_symbol}\",\"adjustment\":\"splits\",\"session\":\"regular\"}}"
    ));
    let messages = vec![
        tv_json_message(
            "chart_create_session",
            &serde_json::json!([chart_session, ""]),
        ),
        tv_json_message(
            "set_auth_token",
            &serde_json::json!(["unauthorized_user_token"]),
        ),
        tv_json_message("quote_create_session", &serde_json::json!([quote_session])),
        tv_json_message(
            "quote_set_fields",
            &serde_json::json!([quote_session, "lp", "ch", "chp", "description", "volume"]),
        ),
        tv_json_message(
            "resolve_symbol",
            &serde_json::json!([chart_session, symbol_id, symbol_spec]),
        ),
        tv_json_message(
            "create_series",
            &serde_json::json!([
                chart_session,
                "sds_1",
                "s1",
                symbol_id,
                interval,
                (n + 2).to_string(),
                ""
            ]),
        ),
    ];

    let deadline = Instant::now() + Duration::from_secs(12);
    let mut bars: Vec<KlineBar> = Vec::new();
    let mut series_completed = false;
    loop {
        let elapsed = deadline.checked_duration_since(Instant::now());
        let Some(timeout) = elapsed else {
            break;
        };
        let message = tokio::select! {
            message = tokio::time::timeout(timeout, reader.next()) => match message {
                Ok(Some(Ok(message))) => message,
                Ok(Some(Err(error))) => return Err(LocalizedError::new("error.market_tv_stream").arg("detail", error)),
                _ => break,
            },
        };
        let text = match message {
            tokio_tungstenite::tungstenite::Message::Text(text) => text,
            tokio_tungstenite::tungstenite::Message::Ping(payload) => {
                let _ = writer
                    .send(tokio_tungstenite::tungstenite::Message::Pong(payload))
                    .await;
                continue;
            }
            tokio_tungstenite::tungstenite::Message::Close(_) => break,
            _ => continue,
        };
        let text = text.as_str();
        // 心跳回显
        if text.contains("~m~") {
            for frame in split_frames(text) {
                if frame.starts_with('h')
                    && frame[1..].chars().all(|c| c.is_ascii_alphanumeric())
                    && frame.len() <= 12
                {
                    let _ = writer
                        .send(tokio_tungstenite::tungstenite::Message::Text(
                            tv_frame(&frame).into(),
                        ))
                        .await;
                }
            }
        }
        // 连接建立后发送初始化序列
        if !init_sent && (text.starts_with("0") || text.contains("~m~")) {
            for message in &messages {
                let _ = writer
                    .send(tokio_tungstenite::tungstenite::Message::Text(
                        message.clone().into(),
                    ))
                    .await;
            }
            init_sent = true;
            continue;
        }
        for frame in split_frames(text) {
            let Ok(value) = serde_json::from_str::<Value>(&frame) else {
                continue;
            };
            let method = value.get("m").and_then(Value::as_str).unwrap_or("");
            if method == "timescale_update" {
                if let Some(series) = value.pointer("/p/1/sds_1/s").and_then(Value::as_array) {
                    for bar in series {
                        let Some(time) = bar.get("i").and_then(Value::as_i64) else {
                            continue;
                        };
                        let values = match bar.get("v") {
                            Some(Value::Array(items)) => items.clone(),
                            Some(Value::Object(map)) => {
                                // 备用：o/h/l/c/v 命名键
                                let pick = |key: &str| map.get(key).cloned().unwrap_or(Value::Null);
                                vec![
                                    Value::Null,
                                    pick("o"),
                                    pick("h"),
                                    pick("l"),
                                    pick("c"),
                                    pick("v"),
                                ]
                            }
                            _ => continue,
                        };
                        let number = |value: &Value| value.as_f64().filter(|v| v.is_finite());
                        let open = values.get(1).and_then(number);
                        let high = values.get(2).and_then(number);
                        let low = values.get(3).and_then(number);
                        let close = values.get(4).and_then(number);
                        let volume = values.get(5).and_then(number).unwrap_or(0.0);
                        let Some((open, high, low, close)) =
                            (|| Some((open?, high?, low?, close?)))()
                        else {
                            continue;
                        };
                        bars.push(KlineBar {
                            seq: 0,
                            ts_open: (time * 1000) as f64,
                            open,
                            high,
                            low,
                            close,
                            volume,
                            closed: true,
                        });
                    }
                }
            } else if method == "series_completed" {
                series_completed = true;
            } else if method == "symbol_error"
                || method == "protocol_error"
                || method == "critical_error"
            {
                return Err(LocalizedError::new("error.market_tv_symbol").arg("symbol", symbol));
            }
        }
        if series_completed && !bars.is_empty() {
            break;
        }
    }
    let _ = writer.close().await;
    if bars.is_empty() {
        return Err(LocalizedError::new("error.market_empty"));
    }
    let duration_ms = (timeframe_to_seconds(timeframe).unwrap_or(900) * 1000) as f64;
    if let Some(last) = bars.last_mut() {
        let elapsed = now_ms as f64 - last.ts_open;
        last.closed = elapsed >= duration_ms;
    }
    bars.sort_by(|a, b| {
        b.ts_open
            .partial_cmp(&a.ts_open)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bars.dedup_by(|a, b| a.ts_open == b.ts_open);
    rebase(&mut bars);
    Ok(bars)
}

fn rebase(bars: &mut [KlineBar]) {
    // bars 已按新→旧排序；forming 头 seq=0，其后已收盘棒从 1 开始编号
    let head_forming = bars.first().is_some_and(|bar| !bar.closed);
    let offset = usize::from(head_forming);
    for (index, bar) in bars.iter_mut().enumerate() {
        bar.seq = if index == 0 && head_forming {
            0
        } else {
            (index + 1 - offset) as u32
        };
    }
}

// ---------------------------------------------------------------------------
// MT5（Python 进程桥）
// ---------------------------------------------------------------------------

const MT5_BRIDGE_SCRIPT: &str = r#"
import json, sys
import MetaTrader5 as mt5
symbol, tf_name, count = sys.argv[1], sys.argv[2], int(sys.argv[3])
try:
    tf_const = getattr(mt5, tf_name)
    if not mt5.initialize():
        print(json.dumps({"error": "mt5 initialize failed"})); raise SystemExit(0)
    rates = mt5.copy_rates_from_pos(symbol, tf_const, 0, count)
    mt5.shutdown()
    if rates is None or len(rates) == 0:
        print(json.dumps({"error": "no rates"})); raise SystemExit(0)
    bars = []
    for rate in rates:
        volume = float(rate["tick_volume"] or 0) or float(rate["real_volume"] or 0)
        bars.append({
            "ts_open": float(rate["time"]) * 1000.0,
            "open": float(rate["open"]), "high": float(rate["high"]),
            "low": float(rate["low"]), "close": float(rate["close"]),
            "volume": volume,
        })
    print(json.dumps({"bars": bars}))
except Exception as error:
    print(json.dumps({"error": str(error)}))
"#;

fn mt5_timeframe_constant(timeframe: &str) -> Option<&'static str> {
    Some(match timeframe {
        "1m" => "TIMEFRAME_M1",
        "2m" => "TIMEFRAME_M2",
        "3m" => "TIMEFRAME_M3",
        "5m" => "TIMEFRAME_M5",
        "10m" => "TIMEFRAME_M10",
        "15m" => "TIMEFRAME_M15",
        "30m" => "TIMEFRAME_M30",
        "1h" => "TIMEFRAME_H1",
        "2h" => "TIMEFRAME_H2",
        "3h" => "TIMEFRAME_H3",
        "4h" => "TIMEFRAME_H4",
        "6h" => "TIMEFRAME_H6",
        "8h" => "TIMEFRAME_H8",
        "12h" => "TIMEFRAME_H12",
        "1d" => "TIMEFRAME_D1",
        "1w" => "TIMEFRAME_W1",
        "1M" => "TIMEFRAME_MN1",
        _ => return None,
    })
}

pub async fn fetch_mt5(
    symbol: &str,
    timeframe: &str,
    n: usize,
    now_ms: u64,
) -> KfResult<Vec<KlineBar>> {
    let constant = mt5_timeframe_constant(timeframe)
        .ok_or_else(|| LocalizedError::new("error.market_timeframe").arg("timeframe", timeframe))?;
    let output = tokio::process::Command::new("python")
        .arg("-c")
        .arg(MT5_BRIDGE_SCRIPT)
        .arg(symbol)
        .arg(constant)
        .arg((n + 2).to_string())
        .output()
        .await
        .map_err(|error| LocalizedError::new("error.market_mt5_python").arg("detail", error))?;
    if !output.status.success() {
        return Err(LocalizedError::new("error.market_mt5_python")
            .arg("detail", String::from_utf8_lossy(&output.stderr)));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(stdout.trim())
        .map_err(|error| LocalizedError::new("error.market_mt5_decode").arg("detail", error))?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(LocalizedError::new("error.market_mt5").arg("detail", error));
    }
    let duration_ms = (timeframe_to_seconds(timeframe).unwrap_or(900) * 1000) as f64;
    let mut bars: Vec<KlineBar> = value
        .get("bars")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let field = |key: &str| item.get(key).and_then(Value::as_f64);
                    Some(KlineBar {
                        seq: 0,
                        ts_open: field("ts_open")?,
                        open: field("open")?,
                        high: field("high")?,
                        low: field("low")?,
                        close: field("close")?,
                        volume: item.get("volume").and_then(Value::as_f64).unwrap_or(0.0),
                        closed: true,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if bars.is_empty() {
        return Err(LocalizedError::new("error.market_empty"));
    }
    if let Some(last) = bars.last_mut() {
        let elapsed = now_ms as f64 - last.ts_open;
        last.closed = elapsed >= duration_ms;
    }
    bars.sort_by(|a, b| {
        b.ts_open
            .partial_cmp(&a.ts_open)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rebase(&mut bars);
    Ok(bars)
}

// ---------------------------------------------------------------------------
// eastmoney（东方财富 push2his K 线 API，A 股，大陆直连）
// ---------------------------------------------------------------------------

const EM_KLINE_HOSTS: &[&str] = &[
    "push2his.eastmoney.com",
    "33.push2his.eastmoney.com",
    "63.push2his.eastmoney.com",
    "7.push2his.eastmoney.com",
    "38.push2his.eastmoney.com",
    "48.push2his.eastmoney.com",
];
const EM_UT: &str = "fa5fd1943c7b386f172d6893dbfba10b";

/// A 股代码 / 常用别名 → secid 候选（1=沪，0=深；shXXXXXX/szXXXXXX/指数直传）。
/// 别名覆盖常见国际代码：XAUUSD 等在东方财富映射到上海黄金交易所品种，
/// 让"默认品种拉不出数据"的用户零配置直连可用数据源。
fn em_secid_candidates(symbol: &str) -> Vec<String> {
    let trimmed = symbol.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // 完整 secid 直传（如 "118.AU9999"、"1.000001"）
    if trimmed.contains('.')
        && trimmed.split_once('.').is_some_and(|(market, code)| {
            !market.is_empty()
                && !code.is_empty()
                && code.chars().all(|ch| ch.is_ascii_alphanumeric())
        })
    {
        return vec![trimmed.to_string()];
    }
    let lower = trimmed.to_ascii_lowercase();
    // 国际品种别名 → 上金所现货（118 市场）
    match lower.as_str() {
        "xauusd" | "xau" | "gold" | "黄金" | "伦敦金" | "现货黄金" => {
            return vec!["118.AU9999".into()];
        }
        "xagusd" | "xag" | "silver" | "白银" | "现货白银" => {
            return vec!["118.AG9999".into()];
        }
        "shci" | "szzs" | "上证指数" | "上证" => {
            return vec!["1.000001".into()];
        }
        _ => {}
    }
    if let Some(code) = lower.strip_prefix("sh")
        && code.len() == 6
        && code.chars().all(|ch| ch.is_ascii_digit())
    {
        return vec![format!("1.{code}")];
    }
    if let Some(code) = lower.strip_prefix("sz")
        && code.len() == 6
        && code.chars().all(|ch| ch.is_ascii_digit())
    {
        return vec![format!("0.{code}")];
    }
    if trimmed.len() == 6 && trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        // 399xxx 深市指数；6/9 开头沪市；其余深市（000001 = 平安银行）
        let market = if trimmed.starts_with('6') || trimmed.starts_with('9') {
            "1"
        } else {
            "0"
        };
        return vec![format!("{market}.{trimmed}")];
    }
    Vec::new()
}

/// 查询串编码：非保留字符原样，其余按 UTF-8 百分号编码（中文品种名用）。
fn encode_query(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// suggest 响应 → 首个沪深 secid（纯函数，便于无头测试）。
/// SecurityType 实测：1=A股 5=指数 8=ETF；16=债券等噪声排除。
fn parse_suggest_secid(value: &Value) -> Option<String> {
    let items = value.pointer("/QuotationCodeTable/Data")?.as_array()?;
    for item in items {
        let quote_id = item.get("QuoteID").and_then(Value::as_str)?;
        let security_type = item
            .get("SecurityType")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(security_type, "1" | "5" | "8") {
            continue;
        }
        // secid 形态 "<market>.<6位代码>"；只认沪深（1./0.）
        let mut parts = quote_id.splitn(2, '.');
        let (market, code) = (parts.next()?, parts.next()?);
        if matches!(market, "1" | "0")
            && code.len() == 6
            && code.chars().all(|ch| ch.is_ascii_digit())
        {
            return Some(quote_id.to_string());
        }
    }
    None
}

/// 中文名/别名在线解析：东方财富统一搜索 suggest API。
/// 输入"贵州茅台"/"茅台"/"纳斯达克"等自然语言品种名，返回首个
/// A 股/指数/ETF 命中的 secid（响应项的 QuoteID 即 "1.600519" 形态）。
/// 解析失败返回 None（由调用方报错）。
async fn em_search_secid(client: &reqwest::Client, query: &str) -> Option<String> {
    let url = format!(
        "https://searchadapter.eastmoney.com/api/suggest/get?input={}&type=14&count=6",
        encode_query(query)
    );
    let value: Value = client
        .get(&url)
        .header("Referer", "https://www.eastmoney.com/")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    parse_suggest_secid(&value)
}

fn em_klt(timeframe: &str) -> Option<&'static str> {
    Some(match timeframe {
        "5m" => "5",
        "15m" => "15",
        "30m" => "30",
        "1h" => "60",
        "1d" => "101",
        "1w" => "102",
        "1M" => "103",
        _ => return None,
    })
}

/// "YYYY-MM-DD[ HH:MM[:SS]]"（北京时间）→ UTC 毫秒。
fn em_time_to_ms(text: &str) -> Option<f64> {
    let digits: Vec<i64> = text
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(str::parse::<i64>)
        .collect::<Result<_, _>>()
        .ok()?;
    if digits.len() < 3 {
        return None;
    }
    let (year, month, day) = (digits[0], digits[1], digits[2]);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let hour = digits.get(3).copied().unwrap_or(0);
    let minute = digits.get(4).copied().unwrap_or(0);
    let second = digits.get(5).copied().unwrap_or(0);
    // days_from_civil（Howard Hinnant）
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - 8 * 3_600;
    Some((seconds * 1000) as f64)
}

pub async fn fetch_eastmoney(
    client: &reqwest::Client,
    symbol: &str,
    timeframe: &str,
    n: usize,
    now_ms: u64,
) -> KfResult<Vec<KlineBar>> {
    let mut secid_candidates = em_secid_candidates(symbol);
    if secid_candidates.is_empty() {
        // 中文品种名（贵州茅台/茅台/上证）等离线表没有的输入：
        // 走东财统一搜索在线解析，仍解析不了才报"无法解析品种"。
        if let Some(secid) = em_search_secid(client, symbol).await {
            secid_candidates.push(secid);
        }
    }
    if secid_candidates.is_empty() {
        return Err(LocalizedError::new("error.market_tv_symbol").arg("symbol", symbol));
    }
    let klt = em_klt(timeframe)
        .ok_or_else(|| LocalizedError::new("error.market_timeframe").arg("timeframe", timeframe))?;
    let mut last_error: Option<String> = None;
    for secid in &secid_candidates {
        let url = format!(
            "https://{}/api/qt/stock/kline/get?fields1=f1,f2,f3,f4,f5,f6&fields2=f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61&ut={EM_UT}&klt={klt}&fqt=1&secid={secid}&beg=0&end=20500000&lmt={}",
            EM_KLINE_HOSTS[0],
            (n + 60).min(1200),
        );
        for host in EM_KLINE_HOSTS {
            let url = url.replace(EM_KLINE_HOSTS[0], host);
            let result = client
            .get(&url)
            .header("Referer", "https://quote.eastmoney.com/")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            )
            .timeout(Duration::from_secs(15))
            .send()
            .await;
            let response = match result {
                Ok(response) => response,
                Err(error) => {
                    last_error = Some(error.to_string());
                    continue;
                }
            };
            if !response.status().is_success() {
                last_error = Some(format!("status {}", response.status().as_u16()));
                continue;
            }
            let payload: Value = match response.json().await {
                Ok(value) => value,
                Err(error) => {
                    last_error = Some(error.to_string());
                    continue;
                }
            };
            let klines = payload
                .pointer("/data/klines")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if klines.is_empty() {
                last_error = Some("empty".into());
                continue;
            }
            // klines 旧→新；每行 "time,open,close,high,low,volume,amount,..."
            let mut bars: Vec<KlineBar> = Vec::with_capacity(klines.len());
            for row in &klines {
                let Some(line) = row.as_str() else { continue };
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() < 6 {
                    continue;
                }
                let (Some(ts), Ok(open), Ok(close), Ok(high), Ok(low), Ok(volume)) = (
                    em_time_to_ms(parts[0].trim()),
                    parts[1].trim().parse::<f64>(),
                    parts[2].trim().parse::<f64>(),
                    parts[3].trim().parse::<f64>(),
                    parts[4].trim().parse::<f64>(),
                    parts[5].trim().parse::<f64>(),
                ) else {
                    continue;
                };
                bars.push(KlineBar {
                    seq: 0,
                    ts_open: ts,
                    open,
                    high,
                    low,
                    close,
                    volume,
                    closed: true,
                });
            }
            if bars.is_empty() {
                last_error = Some("parse".into());
                continue;
            }
            // 最新棒是否形成中
            let duration_ms = (timeframe_to_seconds(timeframe).unwrap_or(900) * 1000) as f64;
            if let Some(last) = bars.last_mut() {
                let elapsed = now_ms as f64 - last.ts_open;
                last.closed = elapsed >= duration_ms;
            }
            bars.reverse(); // → 新→旧
            bars.truncate(n + 1);
            rebase(&mut bars);
            return Ok(bars);
        }
    }
    Err(LocalizedError::new("error.market_fetch").arg(
        "detail",
        last_error.unwrap_or_else(|| "eastmoney unreachable".into()),
    ))
}

async fn fetch_one(
    client: &reqwest::Client,
    kind: &str,
    symbol: &str,
    exchange: &str,
    timeframe: &str,
    n: usize,
) -> KfResult<Vec<KlineBar>> {
    let now = super::records::now_ms();
    match kind {
        "yfinance" => fetch_yfinance(client, symbol, timeframe, n, now).await,
        "tradingview" => fetch_tradingview(symbol, exchange, timeframe, n, now).await,
        "mt5" => fetch_mt5(symbol, timeframe, n, now).await,
        "eastmoney" => fetch_eastmoney(client, symbol, timeframe, n, now).await,
        _ => Err(LocalizedError::new("error.market_source").arg("source", kind)),
    }
}

/// 品种在某数据源上的可解析形态（None = 该源不认识此品种，跳过）。
fn symbol_for_source(kind: &str, symbol: &str, exchange: &str) -> Option<(String, String)> {
    let trimmed = symbol.trim();
    if trimmed.is_empty() {
        return None;
    }
    match kind {
        "eastmoney" => {
            // 离线别名表认识，或输入含非 ASCII（中文品种名，在线搜索兜底）
            if em_secid_candidates(trimmed).is_empty() && !trimmed.is_ascii() {
                Some((trimmed.to_string(), String::new()))
            } else if em_secid_candidates(trimmed).is_empty() {
                None
            } else {
                Some((trimmed.to_string(), String::new()))
            }
        }
        "yfinance" => {
            let lower = trimmed.to_ascii_lowercase();
            let mapped = match lower.as_str() {
                "xauusd" | "xau" | "gold" | "黄金" | "伦敦金" | "现货黄金" => {
                    "GC=F".into()
                }
                "xagusd" | "xag" | "silver" | "白银" | "现货白银" => "SI=F".into(),
                other => {
                    let code = other
                        .strip_prefix("sh")
                        .or_else(|| other.strip_prefix("sz"))
                        .unwrap_or(other);
                    if code.len() == 6 && code.chars().all(|ch| ch.is_ascii_digit()) {
                        if code.starts_with('6') || code.starts_with('9') {
                            format!("{code}.SS")
                        } else {
                            format!("{code}.SZ")
                        }
                    } else {
                        trimmed.to_string()
                    }
                }
            };
            Some((mapped, String::new()))
        }
        "tradingview" => {
            let lower = trimmed.to_ascii_lowercase();
            let (symbol, tv_exchange) = if matches!(
                lower.as_str(),
                "xauusd" | "xau" | "gold" | "黄金" | "伦敦金" | "现货黄金"
            ) {
                ("XAUUSD".to_string(), "OANDA".to_string())
            } else if lower.starts_with("sh") && lower.len() == 8 {
                (trimmed[2..].to_string(), "SSE".to_string())
            } else if lower.starts_with("sz") && lower.len() == 8 {
                (trimmed[2..].to_string(), "SZSE".to_string())
            } else if trimmed.len() == 6 && trimmed.chars().all(|ch| ch.is_ascii_digit()) {
                // 裸 A 股代码按首位判断交易所：6/9 沪，其余深
                if trimmed.starts_with('6') || trimmed.starts_with('9') {
                    (trimmed.to_string(), "SSE".to_string())
                } else {
                    (trimmed.to_string(), "SZSE".to_string())
                }
            } else {
                (trimmed.to_string(), exchange.trim().to_string())
            };
            Some((symbol, tv_exchange))
        }
        _ => None,
    }
}

/// 跨源兜底顺序：主源失败后按此顺序改写品种重试，保证默认品种总有源可用。
/// 东方财富优先（大陆直连、无墙），其次 TradingView，最后 Yahoo。
fn fallback_plan(
    primary: &str,
    symbol: &str,
    exchange: &str,
) -> Vec<(&'static str, String, String)> {
    let mut plan: Vec<(&'static str, String, String)> = Vec::new();
    for kind in ["eastmoney", "tradingview", "yfinance"] {
        if kind == primary {
            continue;
        }
        if let Some((mapped_symbol, mapped_exchange)) = symbol_for_source(kind, symbol, exchange) {
            plan.push((kind, mapped_symbol, mapped_exchange));
        }
    }
    plan
}

/// 统一拉取入口（带跨源兜底）：返回 (实际数据源, 棒序列)。
pub async fn fetch_bars_resolved(
    client: &reqwest::Client,
    kind: &str,
    symbol: &str,
    exchange: &str,
    timeframe: &str,
    n: usize,
) -> KfResult<(String, Vec<KlineBar>)> {
    let primary_error = match fetch_one(client, kind, symbol, exchange, timeframe, n).await {
        Ok(bars) if !bars.is_empty() => return Ok((kind.to_string(), bars)),
        Ok(_) => LocalizedError::new("error.market_empty"),
        Err(error) => error,
    };
    for (fallback_kind, fallback_symbol, fallback_exchange) in fallback_plan(kind, symbol, exchange)
    {
        if let Ok(bars) = fetch_one(
            client,
            fallback_kind,
            &fallback_symbol,
            &fallback_exchange,
            timeframe,
            n,
        )
        .await
            && !bars.is_empty()
        {
            return Ok((fallback_kind.to_string(), bars));
        }
    }
    Err(primary_error)
}

/// 统一拉取入口。
pub async fn fetch_bars(
    client: &reqwest::Client,
    kind: &str,
    symbol: &str,
    exchange: &str,
    timeframe: &str,
    n: usize,
) -> KfResult<Vec<KlineBar>> {
    fetch_bars_resolved(client, kind, symbol, exchange, timeframe, n)
        .await
        .map(|(_, bars)| bars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_split_and_reassemble() {
        let payload = "{\"m\":\"du\",\"p\":[1,2]}";
        let wire = tv_frame(payload);
        assert!(wire.starts_with("~m~"));
        let frames = split_frames(&wire);
        assert_eq!(frames, vec![payload.to_string()]);
        // 多帧拼接
        let two = format!("{}{}", tv_frame("h123"), tv_frame(payload));
        assert_eq!(
            split_frames(&two),
            vec!["h123".to_string(), payload.to_string()]
        );
    }

    #[test]
    fn rebasing_assigns_zero_to_forming_head() {
        let mut bars = vec![
            KlineBar {
                seq: 9,
                ts_open: 30.0,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 0.0,
                closed: false,
            },
            KlineBar {
                seq: 9,
                ts_open: 20.0,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 0.0,
                closed: true,
            },
            KlineBar {
                seq: 9,
                ts_open: 10.0,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 0.0,
                closed: true,
            },
        ];
        rebase(&mut bars);
        assert_eq!(bars[0].seq, 0);
        assert_eq!(bars[1].seq, 1);
        assert_eq!(bars[2].seq, 2);
    }

    #[test]
    fn em_secid_aliases_cover_international_metals_and_passthrough() {
        // 国际金属别名 → 上金所现货
        assert_eq!(
            em_secid_candidates("XAUUSD"),
            vec!["118.AU9999".to_string()]
        );
        assert_eq!(em_secid_candidates("黄金"), vec!["118.AU9999".to_string()]);
        assert_eq!(em_secid_candidates("白银"), vec!["118.AG9999".to_string()]);
        // 指数别名
        assert_eq!(
            em_secid_candidates("上证指数"),
            vec!["1.000001".to_string()]
        );
        // 完整 secid 直传
        assert_eq!(
            em_secid_candidates("118.AU9999"),
            vec!["118.AU9999".to_string()]
        );
        // A 股：沪/深前缀与 6 位裸代码
        assert_eq!(
            em_secid_candidates("sh600519"),
            vec!["1.600519".to_string()]
        );
        assert_eq!(
            em_secid_candidates("sz000001"),
            vec!["0.000001".to_string()]
        );
        assert_eq!(em_secid_candidates("600519"), vec!["1.600519".to_string()]);
        // 未知代码无法解析
        assert!(em_secid_candidates("AAPL").is_empty());
        assert!(em_secid_candidates("").is_empty());
    }

    #[test]
    fn suggest_parsing_takes_first_cny_secid_and_rejects_noise() {
        // 真实响应形态（贵州茅台）：QuoteID 即 secid
        let payload: Value = serde_json::from_str(
            r#"{"QuotationCodeTable":{"Data":[
                {"Code":"600519","Name":"贵州茅台","QuoteID":"1.600519","SecurityType":"1","MktNum":"1"},
                {"Code":"019547","Name":"某国债","QuoteID":"1.019547","SecurityType":"16"}
            ],"Status":0}}"#,
        )
        .unwrap();
        assert_eq!(parse_suggest_secid(&payload).as_deref(), Some("1.600519"));

        // 债券在前：按 SecurityType 白名单跳过，取 A 股
        let noisy: Value = serde_json::from_str(
            r#"{"QuotationCodeTable":{"Data":[
                {"QuoteID":"0.100706","SecurityType":"16"},
                {"QuoteID":"0.000858","SecurityType":"1"}
            ]}}"#,
        )
        .unwrap();
        assert_eq!(parse_suggest_secid(&noisy).as_deref(), Some("0.000858"));

        // 指数（上证指数 ST=5）与 ETF（ST=8）也在白名单内
        let index: Value = serde_json::from_str(
            r#"{"QuotationCodeTable":{"Data":[{"QuoteID":"1.000001","SecurityType":"5"}]}}"#,
        )
        .unwrap();
        assert_eq!(parse_suggest_secid(&index).as_deref(), Some("1.000001"));

        // 港股/无数据/全债券 → None
        let hk: Value = serde_json::from_str(
            r#"{"QuotationCodeTable":{"Data":[{"QuoteID":"116.00700","SecurityType":"1"}]}}"#,
        )
        .unwrap();
        assert!(parse_suggest_secid(&hk).is_none());
        let empty: Value = serde_json::from_str(r#"{"QuotationCodeTable":{}}"#).unwrap();
        assert!(parse_suggest_secid(&empty).is_none());
    }

    #[test]
    fn query_encoding_percent_encodes_chinese_and_keeps_codes() {
        assert_eq!(encode_query("600519"), "600519");
        assert_eq!(encode_query("XAUUSD"), "XAUUSD");
        // "贵州茅台" UTF-8 字节逐位百分号编码
        assert_eq!(
            encode_query("贵州茅台"),
            "%E8%B4%B5%E5%B7%9E%E8%8C%85%E5%8F%B0"
        );
        assert_eq!(encode_query("sh 600519"), "sh%20600519");
    }

    #[test]
    fn fallback_plan_maps_symbols_per_source_and_skips_primary() {
        // 默认 TradingView 失败：东方财富（别名）→ Yahoo（GC=F）
        let plan = fallback_plan("tradingview", "XAUUSD", "OANDA");
        let kinds: Vec<&str> = plan.iter().map(|(kind, _, _)| *kind).collect();
        assert_eq!(kinds, vec!["eastmoney", "yfinance"]);
        assert_eq!(plan[0].1, "XAUUSD"); // 东财吃别名
        assert_eq!(plan[1].1, "GC=F"); // Yahoo 走 COMEX 金

        // 主源东方财富失败：TradingView（OANDA:XAUUSD）→ Yahoo
        let plan = fallback_plan("eastmoney", "XAUUSD", "OANDA");
        let kinds: Vec<&str> = plan.iter().map(|(kind, _, _)| *kind).collect();
        assert_eq!(kinds, vec!["tradingview", "yfinance"]);
        assert_eq!(
            (plan[0].1.as_str(), plan[0].2.as_str()),
            ("XAUUSD", "OANDA")
        );

        // 美股代码只有 Yahoo/TV 认识，东方财富被跳过
        let plan = fallback_plan("yfinance", "AAPL", "");
        let kinds: Vec<&str> = plan.iter().map(|(kind, _, _)| *kind).collect();
        assert_eq!(kinds, vec!["tradingview"]);

        // A 股：Yahoo 后缀 .SS/.SZ，TV 交易所 SSE/SZSE
        let plan = fallback_plan("eastmoney", "600519", "");
        assert_eq!(plan[1].1, "600519.SS");
        assert_eq!(plan[0].2, "SSE");
        let plan = fallback_plan("eastmoney", "sz000001", "");
        assert_eq!(plan[1].1, "000001.SZ");
        assert_eq!(plan[0].2, "SZSE");
    }
}
