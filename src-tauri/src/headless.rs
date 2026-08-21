use crate::{
    agent_loop,
    error::{KfResult, LocalizedError},
    provider,
    runtime::RuntimeEventSink,
    state::{ActiveTurn, AppState},
    types::{HistoryItem, RuntimeEvent, UsageSnapshot},
};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Narrow public entry point for the headless CLI.
///
/// Everything else in the crate stays private: `AppState`, the runtime sink
/// trait, session creation, and the agent loop are all reached only through
/// this module.
#[derive(Debug, Clone)]
pub struct HeadlessOptions {
    /// Project directory. Built and indexed before the user turn.
    pub project: PathBuf,
    /// User turn text. Must be non-empty.
    pub prompt: String,
    /// Model id. Defaults to the compatibility test model.
    pub model: Option<String>,
    /// Chat-compatible base URL. Defaults to the compatibility test endpoint.
    pub endpoint: Option<String>,
    /// Ordered JSONL runtime events target. Absent means stdout.
    pub events: Option<PathBuf>,
    /// Machine-readable result record target. Absent means stdout.
    pub result: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadlessToolCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadlessResult {
    pub kind: String,
    pub ok: bool,
    pub answer: String,
    pub usage: UsageSnapshot,
    pub model: String,
    pub project: String,
    pub tools: Vec<HeadlessToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<LocalizedError>,
}

struct JsonlEventSink {
    writer: Mutex<Box<dyn std::io::Write + Send>>,
}

impl JsonlEventSink {
    fn new_file(path: &Path) -> KfResult<Self> {
        let file = std::fs::File::create(path)
            .map_err(|error| LocalizedError::from(error).arg("path", path.display()))?;
        Ok(Self {
            writer: Mutex::new(Box::new(std::io::BufWriter::new(file))),
        })
    }

    fn stdout() -> Self {
        Self {
            writer: Mutex::new(Box::new(std::io::stdout())),
        }
    }
}

impl RuntimeEventSink for JsonlEventSink {
    fn emit(&self, event: RuntimeEvent) {
        let mut writer = self.writer.lock();
        let line = serde_json::to_string(&event).unwrap_or_default();
        let _ = writeln!(writer, "{line}");
        let _ = writer.flush();
    }
}

fn tool_calls_from_history(state: &AppState, session_id: &str) -> Vec<HeadlessToolCall> {
    state
        .histories
        .read()
        .get(session_id)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    HistoryItem::ToolCall {
                        name, arguments, ..
                    } => Some(HeadlessToolCall {
                        name: name.clone(),
                        arguments: arguments.clone(),
                    }),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn write_result(path: &Path, result: &HeadlessResult) -> KfResult<()> {
    let line = serde_json::to_string(result)
        .map_err(|error| LocalizedError::new("error.encode").arg("detail", error))?;
    let mut file = std::fs::File::create(path)
        .map_err(|error| LocalizedError::from(error).arg("path", path.display()))?;
    writeln!(file, "{line}")
        .map_err(|error| LocalizedError::from(error).arg("path", path.display()))?;
    file.flush()
        .map_err(|error| LocalizedError::from(error).arg("path", path.display()))?;
    Ok(())
}

/// Build the project index, create one isolated session, run exactly one turn,
/// and end with a machine-readable result record.
///
/// The turn uses the same `AppState`, `project::build_manifest`,
/// `agent_loop::run_with_sink`, built-in tools, provider streaming, canonical
/// histories, cancellation token, and usage accounting as the Tauri UI. There
/// is no max-turn or timeout. Runtime failures are surfaced in the returned
/// result (with `ok: false` and a structured error) after the events stream has
/// been emitted; callers translate that into a nonzero exit.
pub async fn run(options: HeadlessOptions) -> KfResult<HeadlessResult> {
    let prompt = options.prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err(LocalizedError::new("error.session_empty_message"));
    }

    let state = AppState::new(Default::default());
    let canonical = crate::project::canonical_root(&options.project)?;
    let project = crate::project::build_manifest(&canonical)?;
    let project_snapshot = project.snapshot.clone();
    let project_root = canonical.display().to_string();
    let model = options
        .model
        .clone()
        .unwrap_or_else(|| provider::MODEL_ID.into());
    let endpoint = options
        .endpoint
        .clone()
        .unwrap_or_else(|| provider::DEFAULT_BASE_URL.into());
    state
        .available_models
        .write()
        .insert(format!("{}\0{}", provider::PROVIDER_ID, model));
    state.projects.write().insert(canonical.clone(), project);
    *state.active_project.write() = Some(canonical);

    let sink: Arc<dyn RuntimeEventSink> = match &options.events {
        Some(path) => Arc::new(JsonlEventSink::new_file(path)?),
        None => Arc::new(JsonlEventSink::stdout()),
    };
    sink.emit(RuntimeEvent::new("project.ready", json!(project_snapshot)));

    let session = crate::session::create_session(
        &state,
        Some(project_root.clone()),
        Some(provider::PROVIDER_ID.into()),
        Some(model.clone()),
    )?;
    let session_id = session.id.clone();
    let turn_id = Uuid::new_v4().to_string();
    sink.emit(
        RuntimeEvent::new(
            "session.started",
            json!({"turnId": turn_id, "session": session}),
        )
        .session(&session_id),
    );
    state
        .histories
        .write()
        .entry(session_id.clone())
        .or_default()
        .push(HistoryItem::User {
            turn_id: turn_id.clone(),
            content: prompt.clone(),
            attachments: Vec::new(),
        });

    let active_turn = ActiveTurn {
        turn_id: turn_id.clone(),
        cancellation: CancellationToken::new(),
        guidance: Arc::new(parking_lot::Mutex::new(VecDeque::new())),
        accepting_guidance: Arc::new(AtomicBool::new(true)),
        guidance_signal: Arc::new(Notify::new()),
    };

    let outcome = agent_loop::run_with_sink(
        sink.clone(),
        state.clone(),
        agent_loop::AgentRunRequest {
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            content: prompt,
            clarify: false,
            active_turn,
            endpoint,
        },
    )
    .await;

    let (answer, error) = match outcome {
        Ok((_usage, text)) => {
            sink.emit(
                RuntimeEvent::new("assistant.completed", json!({"turnId": turn_id}))
                    .session(&session_id),
            );
            (text, None)
        }
        Err(error) => {
            let kind = if error.key == "error.session_cancelled" {
                "assistant.cancelled"
            } else {
                "assistant.failed"
            };
            sink.emit(
                RuntimeEvent::new(kind, json!({"turnId": turn_id, "error": error}))
                    .session(&session_id),
            );
            (String::new(), Some(error))
        }
    };

    let usage = state
        .sessions
        .read()
        .get(&session_id)
        .map(|session| session.usage.clone())
        .unwrap_or_default();
    let tools = tool_calls_from_history(&state, &session_id);
    let result = HeadlessResult {
        kind: "headless.result".into(),
        ok: error.is_none(),
        answer,
        usage,
        model,
        project: project_root,
        tools,
        error,
    };

    match &options.result {
        Some(path) => write_result(path, &result)?,
        None => {
            let line = serde_json::to_string(&result)
                .map_err(|error| LocalizedError::new("error.encode").arg("detail", error))?;
            let mut stdout = std::io::stdout();
            let _ = writeln!(stdout, "{line}");
            let _ = stdout.flush();
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonl_sink_truncates_stale_content_and_preserves_emit_order() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        std::fs::write(&path, "stale line\n").unwrap();
        let sink = JsonlEventSink::new_file(&path).unwrap();
        sink.emit(RuntimeEvent::new("first", json!({"n": 1})));
        sink.emit(RuntimeEvent::new("second", json!({"n": 2})));
        drop(sink);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("stale line"));
        let kinds: Vec<String> = content
            .lines()
            .map(|line| {
                let value: Value = serde_json::from_str(line).unwrap();
                value["kind"].as_str().unwrap().to_owned()
            })
            .collect();
        assert_eq!(kinds, vec!["first", "second"]);
    }

    #[test]
    fn canonical_history_extracts_all_tool_calls_in_order() {
        let state = AppState::new(Default::default());
        state.histories.write().insert(
            "s1".into(),
            vec![
                HistoryItem::User {
                    turn_id: "t".into(),
                    content: "go".into(),
                    attachments: Vec::new(),
                },
                HistoryItem::ToolCall {
                    turn_id: "t".into(),
                    call_id: "c1".into(),
                    name: "find".into(),
                    arguments: json!({"query": "main"}),
                },
                HistoryItem::ToolResult {
                    turn_id: "t".into(),
                    call_id: "c1".into(),
                    projection: json!({"status": "completed"}),
                    artifact_id: None,
                },
                HistoryItem::ToolCall {
                    turn_id: "t".into(),
                    call_id: "c2".into(),
                    name: "read".into(),
                    arguments: json!({"path": "src/main.rs", "startLine": 1, "endLine": 2}),
                },
            ],
        );

        let calls = tool_calls_from_history(&state, "s1");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "find");
        assert_eq!(calls[0].arguments["query"], "main");
        assert_eq!(calls[1].name, "read");
        assert_eq!(calls[1].arguments["path"], "src/main.rs");
    }

    #[test]
    fn result_record_serializes_all_contract_fields() {
        let result = HeadlessResult {
            kind: "headless.result".into(),
            ok: true,
            answer: "done".into(),
            usage: UsageSnapshot {
                fresh_input_tokens: 1,
                cache_read_tokens: 2,
                output_tokens: 3,
                reasoning_tokens: 4,
                request_count: 5,
                current_context_tokens: Some(6),
            },
            model: "future-code-9-free".into(),
            project: "C:/proj".into(),
            tools: vec![HeadlessToolCall {
                name: "find".into(),
                arguments: json!({"query": "x"}),
            }],
            error: None,
        };

        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["answer"], "done");
        assert_eq!(value["usage"]["freshInputTokens"], 1);
        assert_eq!(value["usage"]["requestCount"], 5);
        assert_eq!(value["model"], "future-code-9-free");
        assert_eq!(value["project"], "C:/proj");
        assert_eq!(value["tools"][0]["name"], "find");
        assert_eq!(value["tools"][0]["arguments"]["query"], "x");
        assert!(value.get("error").is_none());
    }

    #[test]
    fn failed_result_carries_structured_error_and_no_answer() {
        let result = HeadlessResult {
            kind: "headless.result".into(),
            ok: false,
            answer: String::new(),
            usage: UsageSnapshot {
                fresh_input_tokens: 0,
                cache_read_tokens: 0,
                output_tokens: 0,
                reasoning_tokens: 0,
                request_count: 1,
                current_context_tokens: None,
            },
            model: provider::MODEL_ID.into(),
            project: "C:/proj".into(),
            tools: Vec::new(),
            error: Some(LocalizedError::new("error.provider_status").arg("status", 503)),
        };

        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["key"], "error.provider_status");
        assert_eq!(value["error"]["args"]["status"], "503");
        assert!(value["tools"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_prompt_is_rejected_before_any_work() {
        let options = HeadlessOptions {
            project: "unused".into(),
            prompt: "   ".into(),
            model: None,
            endpoint: None,
            events: None,
            result: None,
        };
        let error = run(options).await.unwrap_err();
        assert_eq!(error.key, "error.session_empty_message");
    }
}
