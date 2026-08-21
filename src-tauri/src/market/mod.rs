//! PA Agent 移植层入口：市场状态、刷新循环、tauri 命令与事件桥接。

pub mod client;
pub mod datasource;
pub mod decision_nodes;
pub mod decision_tree;
pub mod features;
pub mod indicators;
pub mod orchestrator;
pub mod prompts;
pub mod records;
pub mod retry;
pub mod router;
pub mod types;
pub mod validator;

use crate::error::{KfResult, LocalizedError};
use crate::types::RuntimeEvent;
use orchestrator::{FreeChatSession, MarketEvent};
use parking_lot::RwLock;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;
use types::{AnalysisRecord, KlineFrame, MarketSettings};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub source: String,
    pub symbol: String,
    pub exchange: String,
    pub timeframe: String,
    pub n_bars: u32,
    pub interval_ms: u64,
}

pub struct MarketState {
    pub settings: RwLock<MarketSettings>,
    pub paths: records::MarketPaths,
    pub subscription: RwLock<Option<Subscription>>,
    pub refresh_cancel: RwLock<Option<CancellationToken>>,
    pub analysis_cancel: RwLock<Option<CancellationToken>>,
    pub analysis_running: AtomicBool,
    pub chat: tokio::sync::Mutex<Option<FreeChatSession>>,
    pub chat_cancel: RwLock<Option<CancellationToken>>,
    pub chat_running: AtomicBool,
    pub last_frame: RwLock<Option<KlineFrame>>,
    pub chat_record: RwLock<Option<AnalysisRecord>>,
}

impl MarketState {
    pub fn new(config_dir: &std::path::Path) -> Self {
        let paths = records::MarketPaths::new(config_dir);
        let _ = std::fs::create_dir_all(paths.records_dir());
        let _ = std::fs::create_dir_all(paths.experience_dir());
        let settings: MarketSettings = std::fs::read(paths.settings_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            settings: RwLock::new(settings),
            paths,
            subscription: RwLock::new(None),
            refresh_cancel: RwLock::new(None),
            analysis_cancel: RwLock::new(None),
            analysis_running: AtomicBool::new(false),
            chat: tokio::sync::Mutex::new(None),
            chat_cancel: RwLock::new(None),
            chat_running: AtomicBool::new(false),
            last_frame: RwLock::new(None),
            chat_record: RwLock::new(None),
        }
    }

    fn persist_settings(&self) -> KfResult<()> {
        std::fs::create_dir_all(&self.paths.base)?;
        let path = self.paths.settings_path();
        let temporary = path.with_extension("json.tmp");
        let settings = self.settings.read();
        std::fs::write(
            &temporary,
            serde_json::to_vec_pretty(&*settings).unwrap_or_default(),
        )?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }
}

fn emit_market(app: &AppHandle, kind: &str, data: Value) {
    let _ = app.emit("kf://runtime", RuntimeEvent::new(kind, data));
}

/// 前端负载剥离 klineData：图表帧走独立的 frame 字段，记录内的 K 线数组
/// 是最大负载且前端不消费，随 market.done / record_load 全量下发会冻结 UI。
fn record_payload(record: &AnalysisRecord) -> Value {
    let mut value = serde_json::to_value(record).unwrap_or(Value::Null);
    if let Some(map) = value.as_object_mut() {
        map.remove("klineData");
    }
    value
}

fn forward_event(app: &AppHandle, event: MarketEvent) {
    match event {
        MarketEvent::Status(message) => {
            emit_market(app, "market.status", json!({"message": message}))
        }
        MarketEvent::StageStarted { stage } => emit_market(
            app,
            "market.stage",
            json!({"stage": stage, "state": "started"}),
        ),
        MarketEvent::StageDone { stage } => emit_market(
            app,
            "market.stage",
            json!({"stage": stage, "state": "done"}),
        ),
        MarketEvent::StreamDelta { stage, kind, chunk } => emit_market(
            app,
            "market.stream",
            json!({"stage": stage, "kind": kind, "chunk": chunk}),
        ),
        MarketEvent::StagePrompt {
            stage,
            system,
            user,
        } => emit_market(
            app,
            "market.prompt",
            json!({"stage": stage, "system": system, "user": user}),
        ),
        MarketEvent::StageRetry {
            stage,
            attempt,
            message,
        } => emit_market(
            app,
            "market.stage",
            json!({"stage": stage, "state": "retry", "attempt": attempt, "message": message}),
        ),
        MarketEvent::FilesReady { files } => {
            emit_market(app, "market.files", json!({"files": files}))
        }
    }
}

// ---------------------------------------------------------------------------
// 刷新循环
// ---------------------------------------------------------------------------

/// 数据源的人类可读名（自动切换提示用）。
fn source_label(kind: &str) -> String {
    match kind {
        "tradingview" => "TradingView".into(),
        "yfinance" => "Yahoo Finance".into(),
        "mt5" => "MT5".into(),
        "eastmoney" => "东方财富 EastMoney".into(),
        other => other.to_string(),
    }
}

fn spawn_refresh_loop(app: AppHandle, state: Arc<MarketState>, subscription: Subscription) {
    let cancellation = CancellationToken::new();
    *state.refresh_cancel.write() = Some(cancellation.clone());
    let client = reqwest::Client::builder()
        .user_agent("KnightFrame/0.1")
        .build()
        .unwrap_or_default();
    tokio::spawn(async move {
        let mut failures: u32 = 0;
        let mut announced_source: Option<String> = None;
        loop {
            if cancellation.is_cancelled() {
                break;
            }
            let fetch_count = subscription.n_bars as usize + types::INDICATOR_WARMUP_BARS + 5;
            let result = datasource::fetch_bars_resolved(
                &client,
                &subscription.source,
                &subscription.symbol,
                &subscription.exchange,
                &subscription.timeframe,
                fetch_count,
            )
            .await;
            match result {
                Ok((resolved_source, bars)) if !bars.is_empty() => {
                    failures = 0;
                    // 跨源兜底命中：明确告知用户实际生效的数据源
                    if resolved_source != subscription.source
                        && announced_source.as_deref() != Some(resolved_source.as_str())
                    {
                        announced_source = Some(resolved_source.clone());
                        emit_market(
                            &app,
                            "market.status",
                            json!({
                                "message": format!(
                                    "数据源不可达，已自动切换：{}",
                                    source_label(&resolved_source)
                                ),
                            }),
                        );
                    }
                    let now = records::now_ms();
                    if let Some(frame) = indicators::build_live_frame(
                        &bars,
                        subscription.n_bars as usize,
                        &subscription.symbol,
                        &subscription.timeframe,
                        now,
                    ) {
                        *state.last_frame.write() = Some(frame.clone());
                        emit_market(
                            &app,
                            "market.frame",
                            json!({"frame": frame, "source": resolved_source}),
                        );
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    failures += 1;
                    emit_market(
                        &app,
                        "market.status",
                        json!({"message": format!("数据源错误：{}", error.key), "error": true}),
                    );
                    let backoff = (0.5 * (1u64 << failures.min(5)) as f64).min(10.0);
                    tokio::select! {
                        _ = cancellation.cancelled() => break,
                        _ = tokio::time::sleep(std::time::Duration::from_secs_f64(backoff)) => {}
                    }
                    continue;
                }
            }
            tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(subscription.interval_ms.max(250))) => {}
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Tauri 命令
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn kf_market_settings_get(
    state: tauri::State<'_, Arc<MarketState>>,
) -> KfResult<MarketSettings> {
    Ok(state.settings.read().clone())
}

#[tauri::command]
pub async fn kf_market_settings_update(
    state: tauri::State<'_, Arc<MarketState>>,
    settings: MarketSettings,
) -> KfResult<MarketSettings> {
    if ![
        "conservative",
        "balanced",
        "aggressive",
        "extreme_aggressive",
    ]
    .contains(&settings.general.decision_stance.as_str())
    {
        return Err(LocalizedError::new("error.market_stance"));
    }
    if !["strict", "lenient"].contains(&settings.validation.normalization_mode.as_str()) {
        return Err(LocalizedError::new("error.market_validation_mode"));
    }
    if settings.general.analysis_bar_count < 20 || settings.general.analysis_bar_count > 5000 {
        return Err(LocalizedError::new("error.market_bar_count"));
    }
    if !datasource::SOURCE_KINDS.contains(&settings.general.last_data_source.as_str()) {
        return Err(LocalizedError::new("error.market_source")
            .arg("source", settings.general.last_data_source.clone()));
    }
    *state.settings.write() = settings.clone();
    let market_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || market_state.persist_settings())
        .await
        .map_err(|e| LocalizedError::new("error.market_settings_write").arg("detail", e))??;
    Ok(settings)
}

#[tauri::command]
pub async fn kf_market_fetch(
    app: AppHandle,
    state: tauri::State<'_, Arc<MarketState>>,
    source: String,
    symbol: String,
    exchange: Option<String>,
    timeframe: String,
    n_bars: Option<u32>,
) -> KfResult<Value> {
    let exchange = exchange.unwrap_or_default();
    let n_bars = n_bars.unwrap_or_else(|| state.settings.read().general.analysis_bar_count);
    let (resolved_source, bars) = datasource::fetch_bars_resolved(
        &state_client(&app),
        &source,
        &symbol,
        &exchange,
        &timeframe,
        n_bars as usize + types::INDICATOR_WARMUP_BARS + 5,
    )
    .await?;
    if resolved_source != source {
        emit_market(
            &app,
            "market.status",
            json!({
                "message": format!("数据源不可达，已自动切换：{}", source_label(&resolved_source)),
            }),
        );
    }
    let now = records::now_ms();
    let frame = indicators::build_live_frame(&bars, n_bars as usize, &symbol, &timeframe, now)
        .ok_or_else(|| LocalizedError::new("error.market_empty"))?;
    *state.last_frame.write() = Some(frame.clone());
    emit_market(
        &app,
        "market.frame",
        json!({"frame": frame, "source": resolved_source}),
    );
    Ok(json!({"ok": true}))
}

#[tauri::command]
pub async fn kf_market_subscribe(
    app: AppHandle,
    state: tauri::State<'_, Arc<MarketState>>,
    source: String,
    symbol: String,
    exchange: Option<String>,
    timeframe: Option<String>,
) -> KfResult<Value> {
    if let Some(cancel) = state.refresh_cancel.write().take() {
        cancel.cancel();
    }
    let mut settings = state.settings.write();
    settings.general.last_data_source = source.clone();
    settings.general.last_symbol = symbol.clone();
    if let Some(exchange) = exchange.clone() {
        settings.general.last_tradingview_exchange = exchange;
    }
    if let Some(timeframe) = timeframe.clone() {
        settings.general.last_timeframe = timeframe;
    }
    let interval_ms = settings.general.refresh_interval_ms;
    let n_bars = settings.general.analysis_bar_count;
    let _ = state.persist_settings();
    drop(settings);
    let subscription = Subscription {
        source,
        symbol,
        exchange: exchange.unwrap_or_default(),
        timeframe: timeframe.unwrap_or_else(|| "15m".into()),
        n_bars,
        interval_ms,
    };
    spawn_refresh_loop(app, state.inner().clone(), subscription.clone());
    *state.subscription.write() = Some(subscription.clone());
    if let Ok(mut guard) = state.chat.try_lock() {
        *guard = None;
    }
    *state.chat_record.write() = None;
    Ok(json!({"ok": true, "subscription": subscription}))
}

#[tauri::command]
pub fn kf_market_unsubscribe(state: tauri::State<'_, Arc<MarketState>>) -> KfResult<Value> {
    if let Some(cancel) = state.refresh_cancel.write().take() {
        cancel.cancel();
    }
    *state.subscription.write() = None;
    Ok(json!({"ok": true}))
}

#[tauri::command]
pub async fn kf_market_analyze(
    app: AppHandle,
    state: tauri::State<'_, Arc<MarketState>>,
    force_incremental: Option<bool>,
) -> KfResult<Value> {
    let frame = state
        .last_frame
        .read()
        .clone()
        .ok_or_else(|| LocalizedError::new("error.market_no_data"))?;
    let settings = state.settings.read().clone();
    let n = settings.general.analysis_bar_count as usize;
    let analysis_frame = indicators::build_analysis_frame(
        &frame.bars,
        n,
        &frame.symbol,
        &frame.timeframe,
        records::now_ms(),
    )
    .ok_or_else(|| LocalizedError::new("error.market_insufficient"))?;
    if state.analysis_running.swap(true, Ordering::SeqCst) {
        return Err(LocalizedError::new("error.market_analysis_busy"));
    }
    let cancellation = CancellationToken::new();
    *state.analysis_cancel.write() = Some(cancellation.clone());
    let market_state = state.inner().clone();
    let client = state_client(&app);
    let run_settings = market_state.settings.read().clone();
    tokio::spawn(async move {
        let result = orchestrator::run_analysis(
            &client,
            &run_settings,
            &market_state.paths,
            &analysis_frame,
            force_incremental.unwrap_or(false),
            &cancellation,
            &|event| forward_event(&app, event),
        )
        .await;
        market_state.analysis_running.store(false, Ordering::SeqCst);
        market_state.analysis_cancel.write().take();
        match result {
            Ok(outcome) => {
                *market_state.chat_record.write() = Some(outcome.record.clone());
                if let Ok(mut guard) = market_state.chat.try_lock() {
                    *guard = Some(FreeChatSession::new(&outcome.record));
                }
                emit_market(
                    &app,
                    "market.done",
                    json!({
                        "recordId": outcome.record_id,
                        "incremental": outcome.incremental,
                        "record": record_payload(&outcome.record),
                    }),
                );
            }
            Err(error) => {
                emit_market(&app, "market.failed", json!({"message": error.to_string()}));
            }
        }
    });
    Ok(json!({"ok": true}))
}

#[tauri::command]
pub fn kf_market_stop_analysis(state: tauri::State<'_, Arc<MarketState>>) -> KfResult<Value> {
    if let Some(cancel) = state.analysis_cancel.write().take() {
        cancel.cancel();
    }
    Ok(json!({"ok": true}))
}

#[tauri::command]
pub async fn kf_market_chat_send(
    app: AppHandle,
    state: tauri::State<'_, Arc<MarketState>>,
    text: String,
) -> KfResult<Value> {
    if text.trim().is_empty() {
        return Err(LocalizedError::new("error.market_chat_empty"));
    }
    if state.chat_running.swap(true, Ordering::SeqCst) {
        return Err(LocalizedError::new("error.market_chat_busy"));
    }
    let Some(record) = state.chat_record.read().clone() else {
        state.chat_running.store(false, Ordering::SeqCst);
        return Err(LocalizedError::new("error.market_chat_no_session"));
    };
    let settings = state.settings.read().clone();
    let frame = state.last_frame.read().clone();
    let kline_table = frame
        .as_ref()
        .map(|frame| prompts::render_kline_table(frame, 30))
        .unwrap_or_default();
    let cancellation = CancellationToken::new();
    *state.chat_cancel.write() = Some(cancellation.clone());
    let market_state = state.inner().clone();
    let client = state_client(&app);
    let text_for_task = text;
    tokio::spawn(async move {
        let writer = records::RecordWriter::new(&market_state.paths, &settings.provider.api_key);
        let mut session_guard = market_state.chat.lock().await;
        let Some(session) = session_guard.as_mut() else {
            market_state.chat_running.store(false, Ordering::SeqCst);
            emit_market(&app, "market.chat.failed", json!({"message": "会话不存在"}));
            return;
        };
        let _ = &record;
        let on_reasoning = |chunk: &str| {
            emit_market(
                &app,
                "market.chat.delta",
                json!({"kind": "reasoning", "chunk": chunk}),
            );
        };
        let on_content = |chunk: &str| {
            emit_market(
                &app,
                "market.chat.delta",
                json!({"kind": "content", "chunk": chunk}),
            );
        };
        let callbacks = client::StreamCallbacks {
            on_reasoning: &on_reasoning,
            on_content: &on_content,
        };
        let result = session
            .send(
                &client,
                &settings,
                &writer,
                &text_for_task,
                &kline_table,
                &cancellation,
                Some(&callbacks),
            )
            .await;
        market_state.chat_running.store(false, Ordering::SeqCst);
        market_state.chat_cancel.write().take();
        match result {
            Ok(reply) => emit_market(
                &app,
                "market.chat.done",
                json!({"turn": session.turn, "content": reply.content}),
            ),
            Err(error) => emit_market(
                &app,
                "market.chat.failed",
                json!({"message": error.to_string()}),
            ),
        }
    });
    Ok(json!({"ok": true}))
}

#[tauri::command]
pub fn kf_market_chat_stop(state: tauri::State<'_, Arc<MarketState>>) -> KfResult<Value> {
    if let Some(cancel) = state.chat_cancel.write().take() {
        cancel.cancel();
    }
    Ok(json!({"ok": true}))
}

#[tauri::command]
pub async fn kf_market_records(
    state: tauri::State<'_, Arc<MarketState>>,
    limit: Option<usize>,
) -> KfResult<Vec<Value>> {
    let records_dir = state.paths.records_dir();
    let limit = limit.unwrap_or(50);
    let items = tauri::async_runtime::spawn_blocking(move || {
        records::list_records(&records_dir)
            .into_iter()
            .take(limit)
            .filter_map(|path| {
                let summary = records::load_record_summary(&path)?;
                Some(json!({
                    "file": path.file_name().and_then(|name| name.to_str()).unwrap_or(""),
                    "meta": summary.meta,
                    "hasDecision": summary.has_decision,
                    "partial": summary.partial,
                }))
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| LocalizedError::new("error.market_record_decode").arg("detail", e))?;
    Ok(items)
}

#[tauri::command]
pub async fn kf_market_record_load(
    app: AppHandle,
    state: tauri::State<'_, Arc<MarketState>>,
    file: String,
) -> KfResult<Value> {
    let records_dir = state.paths.records_dir();
    let file_name = file.clone();
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        let path = records_dir.join(&file_name);
        if !path.is_file() {
            return Err(LocalizedError::new("error.market_record_missing"));
        }
        let record = records::load_record(&path)
            .ok_or_else(|| LocalizedError::new("error.market_record_decode"))?;
        let symbol = record
            .meta
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or("SYM")
            .to_string();
        let timeframe = record
            .meta
            .get("timeframe")
            .and_then(Value::as_str)
            .unwrap_or("15m")
            .to_string();
        let frame = indicators::frame_from_records(
            &record.kline_data,
            &symbol,
            &timeframe,
            records::now_ms(),
        );
        Ok((record, frame))
    })
    .await
    .map_err(|e| LocalizedError::new("error.market_record_decode").arg("detail", e))??;
    let (record, frame) = loaded;
    *state.chat_record.write() = Some(record.clone());
    if let Ok(mut guard) = state.chat.try_lock() {
        *guard = Some(FreeChatSession::new(&record));
    }
    emit_market(
        &app,
        "market.frame",
        json!({"frame": frame, "source": "record"}),
    );
    emit_market(
        &app,
        "market.done",
        json!({"recordId": file, "incremental": false, "record": record_payload(&record)}),
    );
    Ok(json!({"ok": true}))
}

/// 复用全局 HTTP 客户端（挂在 AppState 上；market 命令独立构建以保证 UA 与超时）。
fn state_client(_app: &AppHandle) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("KnightFrame-Market/0.1")
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default()
}
