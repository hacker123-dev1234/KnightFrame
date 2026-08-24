//! 两阶段分析编排器与自由追问会话（移植自 orchestrator/two_stage.py + free_chat.py）。

use super::client::{AiReply, AiUsage, ClientError, StreamCallbacks, stream_chat};
use super::decision_tree::build_stage2_gate_wait_response;
use super::prompts::{build_incremental_stage1, build_stage1, build_stage1_messages, build_stage2};
use super::records::{
    MarketPaths, RecordWriter, build_meta, compute_incremental_delta,
    find_latest_successful_record, now_ms, read_experience, render_experience,
};
use super::retry::{build_retry_feedback, detect_cheat, max_retries_for_category, should_retry};
use super::router::route_strategy_files;
use super::types::{AnalysisRecord, FollowupTurn, KlineFrame, MarketSettings};
use super::validator::{ValidationError, validate_stage1, validate_stage2};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub enum MarketEvent {
    Status(String),
    StageStarted {
        stage: String,
    },
    StreamDelta {
        stage: String,
        kind: &'static str,
        chunk: String,
    },
    StagePrompt {
        stage: String,
        system: String,
        user: String,
    },
    StageDone {
        stage: String,
    },
    StageRetry {
        stage: String,
        attempt: u32,
        message: String,
    },
    FilesReady {
        files: Vec<String>,
    },
}

pub type EventSink<'a> = &'a (dyn Fn(MarketEvent) + Sync);

pub enum AnalysisError {
    Cancelled,
    InsufficientData(String),
    Stage1Failed(ValidationError),
    Stage2Failed(ValidationError),
    Network(String),
    Io(String),
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalysisError::Cancelled => write!(formatter, "已取消"),
            AnalysisError::InsufficientData(detail) => write!(formatter, "数据不足：{detail}"),
            AnalysisError::Stage1Failed(error) => {
                write!(formatter, "阶段一校验失败：{}", error.message)
            }
            AnalysisError::Stage2Failed(error) => {
                write!(formatter, "阶段二校验失败：{}", error.message)
            }
            AnalysisError::Network(detail) => write!(formatter, "网络错误：{detail}"),
            AnalysisError::Io(detail) => write!(formatter, "存储错误：{detail}"),
        }
    }
}

fn empty_record(frame: &KlineFrame, settings: &MarketSettings) -> AnalysisRecord {
    AnalysisRecord {
        meta: build_meta(frame, settings),
        kline_data: frame.bars.clone(),
        htf_text: String::new(),
        stage1_messages: vec![],
        stage1_response: None,
        stage1_diagnosis: None,
        stage2_messages: vec![],
        stage2_response: None,
        stage2_decision: None,
        strategy_files_used: vec![],
        experience_loaded: vec![],
        exception: None,
        usage_total: json!({}),
        _partial_reason: None,
    }
}

fn ai_reply_value(reply: &AiReply) -> Value {
    json!({
        "content": reply.content,
        "reasoning_content": reply.reasoning_content,
        "usage": {
            "prompt_tokens": reply.usage.prompt_tokens,
            "cached_prompt_tokens": reply.usage.cached_prompt_tokens,
            "completion_tokens": reply.usage.completion_tokens,
            "total_tokens": reply.usage.total_tokens,
        },
        "latency_ms": reply.latency_ms,
    })
}

struct ValidateOutcome {
    reply: AiReply,
    value: Value,
    #[allow(dead_code)] // 调试信息保留：校验重试次数
    retries: u32,
}

const RETRY_REPLY_LIMIT: usize = 4 * 1024;

fn bounded_retry_reply(content: &str) -> String {
    if content.len() <= RETRY_REPLY_LIMIT {
        return content.to_owned();
    }
    let head_end = content
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= RETRY_REPLY_LIMIT * 3 / 4)
        .last()
        .unwrap_or(0);
    let tail_start = content
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| *index >= content.len().saturating_sub(RETRY_REPLY_LIMIT / 4))
        .unwrap_or(content.len());
    format!(
        "{}\n...[失败回复已截断]...\n{}",
        &content[..head_end],
        &content[tail_start..]
    )
}

#[allow(clippy::too_many_arguments)] // validation pipeline context, not a config surface
async fn validate_with_retry(
    stage: &str,
    client: &reqwest::Client,
    settings: &MarketSettings,
    messages: Vec<Value>,
    initial_reply: AiReply,
    frame: &KlineFrame,
    stage1: Option<&Value>,
    cancellation: &CancellationToken,
    emit: EventSink<'_>,
) -> Result<ValidateOutcome, AnalysisError> {
    let mut reply = initial_reply;
    let mut messages = messages;
    let retry_base_len = messages.len();
    let mut previous_object: Option<Value> = None;
    let mut attempt = 0u32;
    loop {
        if cancellation.is_cancelled() {
            return Err(AnalysisError::Cancelled);
        }
        let result = if stage == "stage1" {
            validate_stage1(&reply.content, frame, settings)
        } else {
            let stage1 = stage1.expect("stage2 requires stage1");
            validate_stage2(&reply.content, frame, stage1, settings)
        };
        match result {
            Ok(value) => {
                if attempt > 0
                    && let Some(previous) = &previous_object
                {
                    let last_feedback = messages
                        .last()
                        .and_then(|message| message.get("content"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let cheats = detect_cheat(stage, previous, &value, last_feedback);
                    if !cheats.is_empty() {
                        let error = ValidationError {
                            category: 'c',
                            message: format!("重试中篡改不可变字段：{}", cheats.join("; ")),
                            missing_fields: vec![],
                            invalid_fields: vec![],
                        };
                        return Err(if stage == "stage1" {
                            AnalysisError::Stage1Failed(error)
                        } else {
                            AnalysisError::Stage2Failed(error)
                        });
                    }
                }
                return Ok(ValidateOutcome {
                    reply,
                    value,
                    retries: attempt,
                });
            }
            Err(error) => {
                let max_attempts = max_retries_for_category(error.category, &settings.validation);
                let stage_retry_allowed = stage == "stage1" || settings.validation.retry_stage2;
                if !stage_retry_allowed || !should_retry(&error, attempt, &settings.validation) {
                    return Err(if stage == "stage1" {
                        AnalysisError::Stage1Failed(error)
                    } else {
                        AnalysisError::Stage2Failed(error)
                    });
                }
                let feedback =
                    build_retry_feedback(&error, stage, attempt + 1, max_attempts, frame);
                emit(MarketEvent::StageRetry {
                    stage: stage.into(),
                    attempt: attempt + 1,
                    message: error.message.clone(),
                });
                previous_object =
                    serde_json::from_str::<Value>(&super::validator::strip_fences(&reply.content))
                        .ok();
                messages.truncate(retry_base_len);
                messages.push(
                    json!({"role": "assistant", "content": bounded_retry_reply(&reply.content)}),
                );
                messages.push(json!({"role": "user", "content": feedback}));
                let reply_result =
                    stream_chat(client, &settings.provider, &messages, cancellation, None).await;
                reply = match reply_result {
                    Ok(reply) => reply,
                    Err(ClientError::Cancelled) => return Err(AnalysisError::Cancelled),
                    Err(error) => return Err(AnalysisError::Network(error.to_string())),
                };
                attempt += 1;
            }
        }
    }
}

pub struct AnalysisOutcome {
    pub record: AnalysisRecord,
    pub record_id: String,
    pub incremental: bool,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_analysis(
    client: &reqwest::Client,
    settings: &MarketSettings,
    paths: &MarketPaths,
    frame: &KlineFrame,
    force_incremental: bool,
    cancellation: &CancellationToken,
    emit: EventSink<'_>,
) -> Result<AnalysisOutcome, AnalysisError> {
    let writer = RecordWriter::new(paths, &settings.provider.api_key);
    let mut record = empty_record(frame, settings);

    if cancellation.is_cancelled() {
        let _ = writer.save_partial(&record, "user_cancelled");
        return Err(AnalysisError::Cancelled);
    }
    if let Err(reason) = super::decision_nodes::check_preflight_data(frame) {
        let _ = writer.save_partial(&record, "insufficient_data");
        return Err(AnalysisError::InsufficientData(reason));
    }

    // 增量分析判定
    let previous_record =
        find_latest_successful_record(&paths.records_dir(), &frame.symbol, &frame.timeframe);
    let incremental_delta = previous_record
        .as_ref()
        .and_then(|previous| compute_incremental_delta(frame, previous));
    let mut incremental = false;
    let mut new_bar_count = 0usize;
    if let Some(delta) = incremental_delta {
        let threshold = settings.general.incremental_max_new_bars;
        let eligible = force_incremental
            || (threshold > 0 && delta.new_count > 0 && delta.new_count <= threshold as usize);
        if eligible && delta.new_count > 0 {
            incremental = true;
            new_bar_count = delta.new_count;
        }
    }

    // 阶段一
    emit(MarketEvent::Status(if incremental {
        format!("增量分析（新增 {new_bar_count} 根 K 线）…")
    } else {
        "阶段一市场诊断中…".into()
    }));
    emit(MarketEvent::StageStarted {
        stage: "stage1".into(),
    });
    let trend_ctx = super::decision_nodes::compute_trend_context(
        frame,
        &super::decision_nodes::judge_direction(frame).0,
    );
    let assembled = if incremental {
        build_incremental_stage1(
            frame,
            settings,
            previous_record
                .as_ref()
                .expect("incremental requires previous"),
            new_bar_count,
            &trend_ctx,
        )
    } else {
        build_stage1(frame, settings, "", &trend_ctx)
    };
    let stage1_messages = build_stage1_messages(&assembled);
    emit(MarketEvent::StagePrompt {
        stage: "stage1".into(),
        system: assembled.system.clone(),
        user: if incremental {
            stage1_messages
                .last()
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        } else {
            assembled.user.clone()
        },
    });
    record.stage1_messages = stage1_messages.clone();
    let stage1_reply = {
        let on_reasoning = |chunk: &str| {
            emit(MarketEvent::StreamDelta {
                stage: "stage1".into(),
                kind: "reasoning",
                chunk: chunk.to_string(),
            });
        };
        let on_content = |chunk: &str| {
            emit(MarketEvent::StreamDelta {
                stage: "stage1".into(),
                kind: "content",
                chunk: chunk.to_string(),
            });
        };
        let callbacks = StreamCallbacks {
            on_reasoning: &on_reasoning,
            on_content: &on_content,
        };
        let result = stream_chat(
            client,
            &settings.provider,
            &stage1_messages,
            cancellation,
            Some(&callbacks),
        )
        .await;
        match result {
            Ok(reply) => reply,
            Err(ClientError::Cancelled) => {
                let _ = writer.save_partial(&record, "user_cancelled");
                return Err(AnalysisError::Cancelled);
            }
            Err(error) => {
                record.exception = Some(
                    json!({"type": "network", "stage": "stage1", "message": error.to_string()}),
                );
                let _ = writer.save_partial(&record, "stage1_network");
                return Err(AnalysisError::Network(error.to_string()));
            }
        }
    };
    if cancellation.is_cancelled() {
        let _ = writer.save_partial(&record, "user_cancelled");
        return Err(AnalysisError::Cancelled);
    }
    record.stage1_response = Some(ai_reply_value(&stage1_reply));

    let stage1_outcome = match validate_with_retry(
        "stage1",
        client,
        settings,
        stage1_messages.clone(),
        stage1_reply,
        frame,
        None,
        cancellation,
        emit,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(AnalysisError::Cancelled) => {
            let _ = writer.save_partial(&record, "user_cancelled");
            return Err(AnalysisError::Cancelled);
        }
        Err(AnalysisError::Stage1Failed(validation)) => {
            record.exception = Some(json!({
                "type": "validation", "stage": "stage1",
                "category": validation.category.to_string(),
                "message": validation.message,
            }));
            let _ = writer.save_partial(&record, "stage1_validation");
            return Err(AnalysisError::Stage1Failed(validation));
        }
        Err(error) => return Err(error),
    };
    let stage1_json = stage1_outcome.value;
    record.stage1_diagnosis = Some(stage1_json.clone());
    record.usage_total = json!({
        "prompt_tokens": stage1_outcome.reply.usage.prompt_tokens,
        "cached_prompt_tokens": stage1_outcome.reply.usage.cached_prompt_tokens,
        "completion_tokens": stage1_outcome.reply.usage.completion_tokens,
        "total_tokens": stage1_outcome.reply.usage.total_tokens,
    });
    emit(MarketEvent::StageDone {
        stage: "stage1".into(),
    });

    // 策略路由 + 经验库
    let strategy_files = route_strategy_files(&stage1_json);
    record.strategy_files_used = strategy_files.clone();
    let patterns: Vec<String> = stage1_json
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
    let direction = stage1_json
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("neutral");
    let experience_entries = read_experience(
        paths,
        stage1_json
            .get("cycle_position")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        direction,
        &patterns,
        settings,
    );
    record.experience_loaded = experience_entries.clone();
    emit(MarketEvent::FilesReady {
        files: strategy_files.clone(),
    });

    if cancellation.is_cancelled() {
        let _ = writer.save_partial(&record, "user_cancelled");
        return Err(AnalysisError::Cancelled);
    }

    // 阶段二
    emit(MarketEvent::StageStarted {
        stage: "stage2".into(),
    });
    let gate_result = stage1_json
        .get("gate_result")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let stage2_json = if gate_result == "wait" || gate_result == "unknown" {
        emit(MarketEvent::Status("闸门未通过，合成不下单结论…".into()));
        let stub = build_stage2_gate_wait_response(&stage1_json);
        record.stage2_messages = vec![];
        record.stage2_response = None;
        stub
    } else {
        emit(MarketEvent::Status("阶段二交易决策中…".into()));
        let experience_text = render_experience(&experience_entries);
        let stage2_assembled = build_stage2(
            frame,
            &stage1_json,
            &strategy_files,
            settings,
            &experience_text,
        );
        let stage2_messages = vec![
            json!({"role": "system", "content": stage2_assembled.system}),
            json!({"role": "user", "content": stage2_assembled.user}),
        ];
        emit(MarketEvent::StagePrompt {
            stage: "stage2".into(),
            system: stage2_assembled.system.clone(),
            user: stage2_assembled.user.clone(),
        });
        record.stage2_messages = stage2_messages.clone();
        let on_reasoning = |chunk: &str| {
            emit(MarketEvent::StreamDelta {
                stage: "stage2".into(),
                kind: "reasoning",
                chunk: chunk.to_string(),
            });
        };
        let on_content = |chunk: &str| {
            emit(MarketEvent::StreamDelta {
                stage: "stage2".into(),
                kind: "content",
                chunk: chunk.to_string(),
            });
        };
        let callbacks = StreamCallbacks {
            on_reasoning: &on_reasoning,
            on_content: &on_content,
        };
        let reply = match stream_chat(
            client,
            &settings.provider,
            &stage2_messages,
            cancellation,
            Some(&callbacks),
        )
        .await
        {
            Ok(reply) => reply,
            Err(ClientError::Cancelled) => {
                let _ = writer.save_partial(&record, "user_cancelled");
                return Err(AnalysisError::Cancelled);
            }
            Err(error) => {
                record.exception = Some(
                    json!({"type": "network", "stage": "stage2", "message": error.to_string()}),
                );
                let _ = writer.save_partial(&record, "stage2_network");
                return Err(AnalysisError::Network(error.to_string()));
            }
        };
        record.stage2_response = Some(ai_reply_value(&reply));
        if cancellation.is_cancelled() {
            let _ = writer.save_partial(&record, "user_cancelled");
            return Err(AnalysisError::Cancelled);
        }
        match validate_with_retry(
            "stage2",
            client,
            settings,
            stage2_messages,
            reply,
            frame,
            Some(&stage1_json),
            cancellation,
            emit,
        )
        .await
        {
            Ok(outcome) => {
                let mut usage = AiUsage::default();
                usage.merge(&stage1_outcome.reply.usage);
                usage.merge(&outcome.reply.usage);
                record.usage_total = json!({
                    "prompt_tokens": usage.prompt_tokens,
                    "cached_prompt_tokens": usage.cached_prompt_tokens,
                    "completion_tokens": usage.completion_tokens,
                    "total_tokens": usage.total_tokens,
                });
                outcome.value
            }
            Err(AnalysisError::Cancelled) => {
                let _ = writer.save_partial(&record, "user_cancelled");
                return Err(AnalysisError::Cancelled);
            }
            Err(AnalysisError::Stage2Failed(validation)) => {
                record.exception = Some(json!({
                    "type": "validation", "stage": "stage2",
                    "category": validation.category.to_string(),
                    "message": validation.message,
                }));
                let _ = writer.save_partial(&record, "stage2_validation");
                return Err(AnalysisError::Stage2Failed(validation));
            }
            Err(error) => return Err(error),
        }
    };
    record.stage2_decision = Some(stage2_json.clone());
    emit(MarketEvent::StageDone {
        stage: "stage2".into(),
    });

    let record_id = writer.record_id(&record);
    let _ = writer.save_full(&record);
    emit(MarketEvent::Status("分析完成，记录已保存".into()));
    Ok(AnalysisOutcome {
        record,
        record_id,
        incremental,
    })
}

// ---------------------------------------------------------------------------
// 自由追问（Free Chat）
// ---------------------------------------------------------------------------

pub struct FreeChatSession {
    pub record_id: String,
    pub symbol: String,
    pub timeframe: String,
    cached_prefix: Vec<Value>,
    history: Vec<Value>,
    pub turn: u32,
    pub usage_total: AiUsage,
}

impl FreeChatSession {
    pub fn new(record: &AnalysisRecord) -> Self {
        let meta = |field: &str| {
            record
                .meta
                .get(field)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let stable_meta = json!({
            "symbol": meta("symbol"),
            "timeframe": meta("timeframe"),
            "bar_count": record.meta.get("bar_count").cloned().unwrap_or(json!(0)),
            "decision_stance": record.meta.get("decision_stance").cloned().unwrap_or(json!("balanced")),
            "model": record.meta.pointer("/ai_provider/model").cloned().unwrap_or(json!("")),
        });
        // assistant recall：从已校验的 stage2_decision 派生（避免 raw 幻觉循环放大）
        let decision = record
            .stage2_decision
            .clone()
            .and_then(|value| value.get("decision").cloned())
            .unwrap_or(json!({}));
        let k1 = record.kline_data.first();
        let k1_description = k1
            .map(|bar| {
                let range = bar.high - bar.low;
                let body_ratio = if range > 0.0 {
                    format!("{:.0}%", (bar.close - bar.open).abs() / range * 100.0)
                } else {
                    "0%".into()
                };
                format!(
                    "最新已收盘 K1：开 {:.2} 高 {:.2} 低 {:.2} 收 {:.2}，实体占比 {body_ratio}，{}。",
                    bar.open, bar.high, bar.low, bar.close,
                    bar.candle_label()
                )
            })
            .unwrap_or_default();
        let recall = json!({
            "recall": "以下是上次分析的已校验结论摘要，供追问上下文使用。",
            "stage2_decision": decision,
            "k1": k1_description,
        })
        .to_string();
        let cached_prefix = vec![
            json!({"role": "system", "content": "你是价格行为分析的追问助手。基于已完成的两阶段分析上下文回答用户追问；回答使用简体中文；可以引用具体 K 序号与价格；不做真实交易建议之外的扩展。"}),
            json!({"role": "user", "content": format!("上次分析结果 JSON：\n{stable_meta}\n{}", serde_json::to_string(&record.stage1_diagnosis.clone().unwrap_or(json!({}))).unwrap_or_default())}),
            json!({"role": "assistant", "content": recall}),
        ];
        Self {
            record_id: format!(
                "{}_{}_{}",
                record
                    .meta
                    .get("timestamp_local_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(now_ms),
                meta("symbol"),
                meta("timeframe")
            ),
            symbol: meta("symbol"),
            timeframe: meta("timeframe"),
            cached_prefix,
            history: Vec::new(),
            turn: 0,
            usage_total: AiUsage::default(),
        }
    }

    #[allow(clippy::too_many_arguments)] // streaming pipeline context, not a config surface
    pub async fn send(
        &mut self,
        client: &reqwest::Client,
        settings: &MarketSettings,
        writer: &RecordWriter,
        user_text: &str,
        kline_table: &str,
        cancellation: &CancellationToken,
        callbacks: Option<&StreamCallbacks<'_>>,
    ) -> Result<AiReply, ClientError> {
        self.turn += 1;
        let mut messages = self.cached_prefix.clone();
        messages.extend(self.history.iter().cloned());
        messages.push(json!({"role": "user", "content": format!("当前 K 线快照：\n{kline_table}\n\n用户追问：{user_text}")}));
        let reply = stream_chat(
            client,
            &settings.provider,
            &messages,
            cancellation,
            callbacks,
        )
        .await?;
        self.history
            .push(json!({"role": "user", "content": user_text}));
        self.history.push(json!({
            "role": "assistant",
            "content": reply.content,
            "reasoning_content": reply.reasoning_content,
        }));
        self.usage_total.merge(&reply.usage);
        let turn = FollowupTurn {
            turn: self.turn,
            ts_ms: now_ms(),
            user: user_text.to_string(),
            ai_content: reply.content.clone(),
            ai_reasoning: (!reply.reasoning_content.is_empty())
                .then(|| reply.reasoning_content.clone()),
            usage: json!({
                "prompt_tokens": reply.usage.prompt_tokens,
                "cached_prompt_tokens": reply.usage.cached_prompt_tokens,
                "completion_tokens": reply.usage.completion_tokens,
                "total_tokens": reply.usage.total_tokens,
            }),
            cancelled: false,
        };
        let _ = writer.append_followup(&self.record_id, &turn);
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::types::KlineBar;

    #[test]
    fn free_chat_prefix_has_three_seed_messages() {
        let record = AnalysisRecord {
            meta: json!({"timestamp_local_ms": 1, "symbol": "XAUUSD", "timeframe": "15m", "bar_count": 10, "decision_stance": "balanced", "ai_provider": {"model": "m"}}),
            kline_data: vec![KlineBar {
                seq: 1,
                ts_open: 1.0,
                open: 10.0,
                high: 12.0,
                low: 9.0,
                close: 11.0,
                volume: 1.0,
                closed: true,
            }],
            htf_text: String::new(),
            stage1_messages: vec![],
            stage1_response: None,
            stage1_diagnosis: Some(json!({"direction": "bullish"})),
            stage2_messages: vec![],
            stage2_response: None,
            stage2_decision: Some(json!({"decision": {"order_type": "不下单"}})),
            strategy_files_used: vec![],
            experience_loaded: vec![],
            exception: None,
            usage_total: json!({}),
            _partial_reason: None,
        };
        let session = FreeChatSession::new(&record);
        assert_eq!(session.cached_prefix.len(), 3);
        assert_eq!(session.record_id, "1_XAUUSD_15m");
        assert!(
            session.cached_prefix[2]["content"]
                .as_str()
                .unwrap()
                .contains("K1")
        );
    }

    #[test]
    fn retry_reply_is_bounded_without_losing_both_ends() {
        let content = format!("BEGIN{}END", "x".repeat(RETRY_REPLY_LIMIT * 2));
        let bounded = bounded_retry_reply(&content);
        assert!(bounded.len() < content.len());
        assert!(bounded.starts_with("BEGIN"));
        assert!(bounded.ends_with("END"));
        assert!(bounded.contains("失败回复已截断"));
    }
}
