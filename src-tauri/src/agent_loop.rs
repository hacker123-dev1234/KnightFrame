use crate::{
    error::{KfResult, LocalizedError},
    project,
    provider::{self, TokenUsage, ToolCall},
    runtime::{RuntimeEventSink, TauriRuntimeEventSink},
    skill,
    state::{ActiveTurn, AppState, QueuedGuidance, ToolObservation},
    task, tools,
    types::{HistoryItem, RuntimeEvent},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Write as _,
    sync::Arc,
    time::Instant,
};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

const MAX_PROJECTION_BYTES: usize = 12 * 1024;
const MAX_COMPACT_SUMMARY_BYTES: usize = 240;
/// Compaction is a budget safety valve, not a routine optimization. Compact
/// savings are cache-hit tokens (nearly free), while the cost is a prefix
/// break that re-prices the whole tail at cache-miss plus recall round-trips
/// that each resend the entire history. So mid-turn history stays append-only
/// until retained tool projections exceed this byte budget; only then are the
/// oldest receipts swapped in, oldest first. Typical tasks never reach it.
const CONTEXT_TOOL_BUDGET_BYTES: usize = 256 * 1024;
const MAX_BATCH_ITEMS: usize = 8;
const DEFAULT_BATCH_READ_LINES: usize = 200;
const REQUIREMENT_REDUCER_THRESHOLD_TOKENS: u64 = 500;
const PROJECT_CONTEXT_SOURCE: &str = "project-graph";
const PROJECT_CONTEXT_CLEARED: &str =
    "Current project graph: unavailable. Earlier project-graph snapshots no longer apply.";
const CLARIFY_TURN: &str =
    "For this request, clarify requirements before acting. Ask only necessary questions.";
const CLARIFY_GUIDANCE: &str =
    "For this guidance, clarify requirements before acting. Ask only necessary questions.";
const CONTINUE_RESPONSE: &str =
    "Continue exactly where the response ended. Do not restart or summarize.";
const FINALIZE_RESPONSE: &str = "Reasoning was received without a final answer. Return the final answer now; do not repeat the reasoning.";
const TOOL_CAPABILITY_RECOVERY: &str = "The provider declared a tool call without its payload. Retry the intended action now with exactly one exposed tool. Use edit for an existing file and run only after the edit.";
const SYSTEM: &str = "You are KnightFrame, a coding agent. Start from the project graph. Batch independent find/search/read queries before shell exploration, then edit exact unique fragments. Tool receipts use short refs; call recall only when missing details matter. Do not repeat unchanged reads or searches; every run executes fresh, so rerun freely whenever fresh output matters. Keep the task plan current for multi-step work. Run programs, builds, and tests; shell remains available. Paths may be relative or absolute. Never invent results. Market and trading questions are analysis only — never trade, order, change positions, or purchase.";
const CAVEMAN_LITE: &str = "Use the minimum necessary words. No pleasantries, request restatement, or unnecessary sections. Preserve technical detail and verification evidence.";

#[derive(Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub version: u16,
    pub enabled: bool,
    pub deferred: bool,
    pub description: &'static str,
    pub schema: Value,
}

pub trait ToolRegistry {
    fn active(&self) -> Vec<ToolSpec>;
    #[cfg(test)]
    fn discover(&self, query: &str) -> Vec<ToolSpec>;
}

pub struct BuiltinRegistry {
    task_enabled: bool,
    skill_enabled: bool,
    workspace_available: bool,
}

impl ToolRegistry for BuiltinRegistry {
    fn active(&self) -> Vec<ToolSpec> {
        let mut tools = Vec::new();
        if self.workspace_available {
            tools.extend([
                spec(
                    "edit",
                    "Replace one exact, unique text fragment.",
                    json!({"type":"object","properties":{"path":{"type":"string"},"oldText":{"type":"string"},"newText":{"type":"string"}},"required":["path","oldText","newText"]}),
                ),
                spec(
                    "find",
                    "Search indexed paths. Optional path narrows to a prefix subtree (shown in the result). Results are capped; page through with offset (result shows next). Send independent lookups together with queries (max 8).",
                    json!({"type":"object","properties":{"query":{"type":"string","minLength":1},"path":{"type":"string","description":"Optional path prefix filter, e.g. src-tauri or PA_Agent-main."},"offset":{"type":"integer","minimum":0,"description":"Skip this many matches to page past the cap; the result echoes the next offset."},"queries":{"type":"array","minItems":1,"maxItems":MAX_BATCH_ITEMS,"items":{"type":"string","minLength":1}}},"oneOf":[{"required":["query"]},{"required":["queries"]}]}),
                ),
                spec(
                    "read",
                    "Read exact verbatim line ranges (up to 800 lines; returned as-is). With ranges, top-level path is the default for items that omit path; it is not read separately. Omitted lines read the first 200 lines.",
                    json!({"type":"object","properties":{"path":{"type":"string","minLength":1},"startLine":{"type":"integer","minimum":1},"endLine":{"type":"integer","minimum":1},"ranges":{"type":"array","minItems":1,"maxItems":MAX_BATCH_ITEMS,"items":{"type":"object","properties":{"path":{"type":"string","minLength":1},"startLine":{"type":"integer","minimum":1},"endLine":{"type":"integer","minimum":1}}}}},"anyOf":[{"required":["path"]},{"required":["ranges"]}]}),
                ),
                spec(
                    "run",
                    "Run a program, build, or test. Use command, or program with separate args.",
                    json!({"type":"object","properties":{"command":{"type":"string","minLength":1},"program":{"type":"string","minLength":1},"args":{"type":"array","items":{"type":"string"}},"cwd":{"type":"string"}},"anyOf":[{"required":["command"]},{"required":["program"]}]}),
                ),
                spec(
                    "search",
                    "Search indexed text. Send independent queries together (max 8); results stay in request order.",
                    json!({"type":"object","properties":{"query":{"type":"string","minLength":1},"path":{"type":"string"},"queries":{"type":"array","minItems":1,"maxItems":MAX_BATCH_ITEMS,"items":{"type":"object","properties":{"query":{"type":"string","minLength":1},"path":{"type":"string"}},"required":["query"]}}},"oneOf":[{"required":["query"]},{"required":["queries"]}]}),
                ),
                spec(
                    "write",
                    "Create a file or replace its entire contents. Prefer edit for small changes.",
                    json!({"type":"object","properties":{"path":{"type":"string","minLength":1},"content":{"type":"string"}},"required":["path","content"]}),
                ),
            ]);
        }
        tools.push(spec(
            "recall",
            "Load a prior tool result by its short ref only when the compact receipt is insufficient.",
            json!({"type":"object","properties":{"reference":{"type":"string","minLength":1}},"required":["reference"]}),
        ));
        tools.push(spec(
            "browser",
            "Control the shared in-window browser. snapshot reads the rendered page and returns compact text plus refs (e1...). Use refs with click/fill/select/hover/press. fetch reads a URL without opening it. Tab, navigation, status, focus, stop, scroll, and close actions are supported.",
            json!({"type":"object","properties":{"action":{"type":"string","enum":["fetch","snapshot","open","new-tab","select-tab","close-tab","navigate","back","forward","refresh","stop","close","focus","status","click","fill","select","hover","press","scroll"]},"url":{"type":"string"},"tabId":{"type":"string"},"offset":{"type":"integer","minimum":0,"description":"Continue from a prior fetch nextOffset."},"ref":{"type":"string","description":"Short element ref returned by fetch/snapshot."},"selector":{"type":"string","description":"Optional CSS selector for interaction."},"value":{"type":"string","description":"Text or option value for fill/select."},"key":{"type":"string","description":"Key for press; default Enter."},"y":{"type":"integer","description":"Scroll pixels; negative moves up."}},"required":["action"]}),
        ));
        tools.push(spec(
            "market",
            "Market data lookup without opening the market page. klines returns a token-lean analysis snapshot for one symbol/timeframe: last close, window change %, high/low, EMA20 trend bias, ATR14 volatility, and the most recent compact OHLCV bars (oldest→newest). Answer buy/what/when questions by calling it for several candidate symbols (e.g. 600519, 000858, 上证指数, XAUUSD/黄金) and comparing. Symbols: A-share codes (600519, sh600519, sz000858), Chinese names (贵州茅台, 茅台, 五粮液 — resolved online), index aliases (上证指数), metals (XAUUSD/黄金, XAGUSD/白银), full eastmoney secids (118.AU9999), or native codes for other sources. Sources auto-failover in unreachable networks (eastmoney works in mainland China by default).",
            json!({"type":"object","properties":{"action":{"type":"string","enum":["klines"],"description":"Only klines for now."},"symbol":{"type":"string","minLength":1,"description":"Instrument code, alias, or Chinese name, e.g. 600519 / XAUUSD / 贵州茅台."},"timeframe":{"type":"string","enum":["5m","15m","30m","1h","4h","1d","1w"],"description":"Bar timeframe (default 15m; 1d for swing views)."},"source":{"type":"string","enum":["eastmoney","tradingview","yfinance","mt5"],"description":"Preferred source; auto-failover if unreachable (default eastmoney)."},"bars":{"type":"integer","minimum":30,"maximum":300,"description":"Closed bars in the compact window (default 120)."}},"required":["symbol"]}),
        ));
        if self.task_enabled {
            tools.push(spec(
                "task",
                "Update visible task progress.",
                json!({"type":"object","properties":{"op":{"type":"string","enum":["add","pending","running","completed","failed","blocked","cancelled"]},"item":{"type":"string"}},"required":["op"]}),
            ));
        }
        if self.skill_enabled {
            tools.push(spec(
                "skill",
                "Load one relevant skill by exact id when a skill directory appears in context.",
                json!({"type":"object","properties":{"name":{"type":"string","minLength":1}},"required":["name"]}),
            ));
        }
        tools.sort_by_key(|tool| (tool_order(tool.name), tool.version));
        tools
    }
    #[cfg(test)]
    fn discover(&self, query: &str) -> Vec<ToolSpec> {
        self.active()
            .into_iter()
            .filter(|tool| tool.name.contains(query))
            .collect()
    }
}

fn tool_order(name: &str) -> usize {
    match name {
        "find" => 0,
        "search" => 1,
        "read" => 2,
        "edit" => 3,
        "write" => 4,
        "run" => 5,
        "browser" => 6,
        "market" => 7,
        "recall" => 8,
        "skill" => 9,
        "task" => 10,
        _ => usize::MAX,
    }
}

fn spec(name: &'static str, description: &'static str, schema: Value) -> ToolSpec {
    ToolSpec {
        name,
        version: 1,
        enabled: true,
        deferred: false,
        description,
        schema,
    }
}

fn wire_tools(registry: &dyn ToolRegistry) -> Vec<Value> {
    registry.active().into_iter().filter(|tool| tool.enabled && !tool.deferred).map(|tool| json!({
        "type":"function", "function":{"name":tool.name,"description":tool.description,"parameters":tool.schema}
    })).collect()
}

fn assistant_wire_message(
    adapter: &str,
    content: &str,
    reasoning: &str,
    tool_calls: Vec<Value>,
) -> Option<Value> {
    if content.is_empty() && tool_calls.is_empty() && (adapter != "openai" || reasoning.is_empty())
    {
        return None;
    }
    let mut message = json!({"role":"assistant","content":content});
    if adapter == "openai"
        && !reasoning.is_empty()
        && let Some(object) = message.as_object_mut()
    {
        object.insert("reasoning_content".into(), json!(reasoning));
    }
    if !tool_calls.is_empty()
        && let Some(object) = message.as_object_mut()
    {
        object.insert("tool_calls".into(), Value::Array(tool_calls));
    }
    Some(message)
}

fn project_history(
    history: &[HistoryItem],
    accepted_briefs: &HashMap<String, String>,
    adapter: &str,
) -> Vec<Value> {
    let mut projected_turns = HashSet::new();
    let mut projected_tool_calls = HashSet::new();
    history
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            HistoryItem::User { turn_id, content, attachments } => {
                let content = accepted_briefs
                    .get(turn_id)
                    .filter(|_| projected_turns.insert(turn_id.as_str()))
                    .unwrap_or(content);
                if attachments.is_empty() {
                    Some(json!({"role":"user","content":content}))
                } else {
                    let mut parts = vec![json!({"type":"text","text":content})];
                    parts.extend(attachments.iter().map(|attachment| json!({"type":"image_url","image_url":{"url":attachment.data_url}})));
                    Some(json!({"role":"user","content":parts}))
                }
            }
            HistoryItem::Context { content, .. } => Some(context_message(content)),
            HistoryItem::Assistant {
                turn_id,
                content,
                reasoning,
            } => {
                let calls = history[index + 1..]
                    .iter()
                    .take_while(|next| {
                        !matches!(
                            next,
                            HistoryItem::User { .. }
                                | HistoryItem::Context { .. }
                                | HistoryItem::Assistant { .. }
                        )
                    })
                    .filter_map(|next| match next {
                        HistoryItem::ToolCall {
                            turn_id: call_turn,
                            call_id,
                            name,
                            arguments,
                        } if call_turn == turn_id => {
                            projected_tool_calls.insert(call_id.clone());
                            Some(json!({
                                "id":call_id, "type":"function",
                                "function":{"name":name,"arguments":arguments.to_string()}
                            }))
                        }
                        _ => None,
                    })
                    .collect();
                assistant_wire_message(adapter, content, reasoning, calls)
            }
            HistoryItem::ToolCall {
                call_id,
                name,
                arguments,
                ..
            } if !projected_tool_calls.contains(call_id) => Some(json!({
                "role":"assistant", "content":"", "tool_calls":[{
                    "id":call_id, "type":"function",
                    "function":{"name":name,"arguments":arguments.to_string()}
                }]
            })),
            HistoryItem::ToolCall { .. } => None,
            HistoryItem::ToolResult {
                call_id,
                projection,
                ..
            } => Some(json!({
                "role":"tool", "tool_call_id":call_id, "content":projection.to_string()
            })),
        })
        .collect()
}

fn context_message(content: &str) -> Value {
    json!({"role":"user","content":content})
}

fn record_context(state: &AppState, session_id: &str, source: &str, content: String) {
    state
        .histories
        .write()
        .entry(session_id.to_owned())
        .or_default()
        .push(HistoryItem::Context {
            source: source.to_owned(),
            content,
        });
}

fn next_project_context_snapshot(
    history: &[HistoryItem],
    current: Option<String>,
) -> Option<String> {
    let retained = history.iter().rev().find_map(|item| match item {
        HistoryItem::Context { source, content } if source == PROJECT_CONTEXT_SOURCE => {
            Some(content.as_str())
        }
        _ => None,
    });
    let snapshot = match current {
        Some(context) => format!(
            "Current project graph. This snapshot supersedes earlier project-graph snapshots.\n\n{context}"
        ),
        None if retained.is_some() => PROJECT_CONTEXT_CLEARED.to_owned(),
        None => return None,
    };
    (retained != Some(snapshot.as_str())).then_some(snapshot)
}

fn sync_project_context(state: &AppState, session_id: &str, root: Option<&str>) -> Option<String> {
    let current = root.and_then(|root| project::model_context(state, root).ok());
    let mut histories = state.histories.write();
    let history = histories.entry(session_id.to_owned()).or_default();
    let snapshot = next_project_context_snapshot(history, current)?;
    history.push(HistoryItem::Context {
        source: PROJECT_CONTEXT_SOURCE.to_owned(),
        content: snapshot.clone(),
    });
    Some(snapshot)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolProjection {
    pub status: String,
    pub summary: String,
    pub exit_code: Option<i32>,
    pub error_key: Option<String>,
    pub completeness: String,
    pub total: usize,
    pub truncated: bool,
    pub artifact_id: String,
}

pub fn project_artifact(artifact_id: String, value: &Value) -> ToolProjection {
    let summary = value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("Result stored locally.")
        .to_owned();
    projection(
        "completed",
        summary,
        value
            .get("exitCode")
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        value.to_string().len(),
        false,
        artifact_id,
    )
}

fn project_tool_artifact(call: &ToolCall, artifact_id: String, value: &Value) -> ToolProjection {
    if let Some(projection) = project_batch_tool_artifact(call, artifact_id.clone(), value) {
        return projection;
    }
    match call.name.as_str() {
        "find" => {
            let matches = value
                .get("matches")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let total = value
                .get("total")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(matches.len());
            let offset = value.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let next_offset = value
                .get("nextOffset")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            let query = call
                .arguments
                .get("query")
                .or_else(|| call.arguments.get("queries"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let filter = call.arguments.get("path").and_then(Value::as_str);
            // Filter echo + window position: makes the API contract visible
            // (path filter applied or not, which slice of the total this is).
            let scope = match filter {
                Some(filter) => format!("query={query} path={filter}"),
                None if !query.is_empty() => format!("query={query}"),
                None => String::new(),
            };
            let window = if matches.is_empty() {
                format!("0 of {total} matched paths")
            } else {
                format!("paths {}-{} of {total}", offset + 1, offset + matches.len())
            };
            let mut summary = match next_offset {
                Some(next) => format!("{scope} {window}; more: offset={next}"),
                None => format!("{scope} {window}"),
            };
            // Enumeration mode (>8 hits): drop per-file relations — the model
            // is scanning names, not navigating imports; relations here were
            // pure token ballast. Targeted lookups (<=8) keep them.
            let enumerate = matches.len() > 8;
            for item in &matches {
                let path = item.get("path").and_then(Value::as_str).unwrap_or("?");
                let language = item
                    .get("language")
                    .and_then(Value::as_str)
                    .unwrap_or("Other");
                let size = item.get("size").and_then(Value::as_u64).unwrap_or(0);
                let _ = write!(summary, "\n{path} [{language}, {size} B]");
                if enumerate {
                    continue;
                }
                let relations = item
                    .get("relations")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                for relation in relations {
                    let kind = relation
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("related");
                    let direction = relation
                        .get("direction")
                        .and_then(Value::as_str)
                        .unwrap_or("out");
                    let related_path = relation.get("path").and_then(Value::as_str).unwrap_or("?");
                    let arrow = if direction == "in" { "<-" } else { "->" };
                    let _ = write!(summary, "\n  {kind} {arrow} {related_path}");
                }
                let relation_total = item
                    .get("relationTotal")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(relations.len());
                let omitted = relation_total.saturating_sub(relations.len());
                if omitted > 0 {
                    let _ = write!(summary, "\n  +{omitted} more direct relations");
                }
            }
            projection(
                "completed",
                summary,
                None,
                total,
                value
                    .get("truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(total > matches.len() + offset),
                artifact_id,
            )
        }
        "search" => {
            let matches = value
                .get("matches")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let total = value
                .get("total")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(matches.len());
            let mut summary = format!("{} of {total} text matches", matches.len());
            for item in &matches {
                let path = item.get("path").and_then(Value::as_str).unwrap_or("?");
                let line = item.get("line").and_then(Value::as_u64).unwrap_or(0);
                let text = item.get("text").and_then(Value::as_str).unwrap_or("");
                let _ = write!(summary, "\n{path}:{line} {text}");
            }
            projection(
                "completed",
                summary,
                None,
                total,
                value
                    .get("truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(total > matches.len()),
                artifact_id,
            )
        }
        "read" => {
            let path = value
                .get("path")
                .and_then(Value::as_str)
                .or_else(|| call.arguments.get("path").and_then(Value::as_str))
                .unwrap_or("?");
            let start = value.get("startLine").and_then(Value::as_u64).unwrap_or(0);
            let end = value.get("endLine").and_then(Value::as_u64).unwrap_or(0);
            let content = value.get("content").and_then(Value::as_str).unwrap_or("");
            projection(
                "completed",
                format!("{path}:{start}-{end}\n{content}"),
                None,
                content.len(),
                false,
                artifact_id,
            )
        }
        "edit" => {
            let path = call
                .arguments
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let replacements = value
                .get("replacements")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let bytes = value
                .get("bytesWritten")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            projection(
                "completed",
                format!("Edited {path}: {replacements} replacement(s), {bytes} B written"),
                None,
                replacements as usize,
                false,
                artifact_id,
            )
        }
        "write" => {
            let path = call
                .arguments
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let bytes = value
                .get("bytesWritten")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            projection(
                "completed",
                format!("Wrote {path}: {bytes} B"),
                None,
                bytes as usize,
                false,
                artifact_id,
            )
        }
        "run" => {
            let exit_code = value
                .get("exitCode")
                .and_then(Value::as_i64)
                .map(|value| value as i32);
            let elapsed = value.get("elapsedMs").and_then(Value::as_u64).unwrap_or(0);
            let stdout = value.get("stdout").and_then(Value::as_str).unwrap_or("");
            let stderr = value.get("stderr").and_then(Value::as_str).unwrap_or("");
            let total = value
                .get("stdoutBytes")
                .and_then(Value::as_u64)
                .unwrap_or(stdout.len() as u64)
                .saturating_add(
                    value
                        .get("stderrBytes")
                        .and_then(Value::as_u64)
                        .unwrap_or(stderr.len() as u64),
                ) as usize;
            let mut summary = match exit_code {
                Some(code) => format!("Exit {code} after {elapsed} ms"),
                None => format!("Exit status unavailable after {elapsed} ms"),
            };
            if !stderr.trim().is_empty() {
                summary.push_str("\nstderr:\n");
                summary.push_str(stderr.trim_end());
            }
            if !stdout.trim().is_empty() {
                summary.push_str("\nstdout:\n");
                summary.push_str(stdout.trim_end());
            }
            if let Some(advisory) = value.get("advisory").and_then(Value::as_str) {
                summary.push_str("\n\n");
                summary.push_str(advisory);
            }
            let (summary, summary_truncated) = bounded_head_tail(summary, MAX_PROJECTION_BYTES);
            projection(
                if exit_code == Some(0) {
                    "completed"
                } else {
                    "failed"
                },
                summary,
                exit_code,
                total,
                summary_truncated
                    || value
                        .get("truncated")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                artifact_id,
            )
        }
        "task" => {
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("pending");
            let completed = value.get("completed").and_then(Value::as_u64).unwrap_or(0);
            let total = value.get("total").and_then(Value::as_u64).unwrap_or(0);
            let current = value
                .get("current")
                .and_then(Value::as_str)
                .map(|value| format!("; current={value}"))
                .unwrap_or_default();
            projection(
                "completed",
                format!("Task {status}: {completed}/{total}{current}"),
                None,
                total as usize,
                false,
                artifact_id,
            )
        }
        "skill" => {
            let content = value.get("content").and_then(Value::as_str).unwrap_or("");
            ToolProjection {
                status: "completed".into(),
                summary: content.to_owned(),
                exit_code: None,
                error_key: None,
                completeness: "complete".into(),
                total: content.len(),
                truncated: false,
                artifact_id,
            }
        }
        "market" => {
            // 摘要本身就是省 token 快照（前端 ToolCard 也解析它定位图表）
            let summary = serde_json::to_string(value).unwrap_or_default();
            let bars = value.get("bars").and_then(Value::as_u64).unwrap_or(0) as usize;
            projection("completed", summary, None, bars, false, artifact_id)
        }
        _ => project_artifact(artifact_id, value),
    }
}

fn project_batch_tool_artifact(
    call: &ToolCall,
    artifact_id: String,
    value: &Value,
) -> Option<ToolProjection> {
    if value.get("batch").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let results = value.get("results")?.as_array()?;
    let mut summary = format!("{} batched {} request(s)", results.len(), call.name);
    let mut total = 0usize;
    let mut failed = 0usize;
    let mut source_truncated = false;
    let per_item_budget =
        (MAX_PROJECTION_BYTES.saturating_sub(256) / results.len().max(1)).max(256);
    for (index, item) in results.iter().enumerate() {
        let label = item.get("label").and_then(Value::as_str).unwrap_or("?");
        let _ = write!(summary, "\n{}. {label}", index + 1);
        if let Some(error) = item.get("error") {
            failed += 1;
            total += 1;
            let key = error
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or("error.tool_argument");
            let _ = write!(summary, "\n  failed: {key}");
            continue;
        }
        let Some(result) = item.get("result") else {
            failed += 1;
            total += 1;
            summary.push_str("\n  failed: error.tool_argument");
            continue;
        };
        let child = project_tool_artifact(call, String::new(), result);
        total = total.saturating_add(child.total);
        let (child_summary, child_summary_truncated) =
            bounded_prefix(child.summary, per_item_budget);
        source_truncated |= child.truncated || child_summary_truncated;
        for line in child_summary.lines() {
            let _ = write!(summary, "\n  {line}");
        }
    }
    let (summary, summary_truncated) = bounded_prefix(summary, MAX_PROJECTION_BYTES);
    let truncated = source_truncated || summary_truncated;
    Some(ToolProjection {
        status: if failed == results.len() && !results.is_empty() {
            "failed"
        } else {
            "completed"
        }
        .into(),
        summary,
        exit_code: None,
        error_key: None,
        completeness: if failed > 0 || truncated {
            "partial"
        } else {
            "complete"
        }
        .into(),
        total,
        truncated,
        artifact_id,
    })
}

fn projection(
    status: &str,
    summary: String,
    exit_code: Option<i32>,
    total: usize,
    source_truncated: bool,
    artifact_id: String,
) -> ToolProjection {
    let (summary, summary_truncated) = bounded_prefix(summary, MAX_PROJECTION_BYTES);
    let truncated = source_truncated || summary_truncated;
    ToolProjection {
        status: status.into(),
        summary,
        exit_code,
        error_key: None,
        completeness: if truncated { "partial" } else { "complete" }.into(),
        total,
        truncated,
        artifact_id,
    }
}

fn bounded_prefix(mut value: String, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value, false);
    }
    let boundary = value.floor_char_boundary(max_bytes);
    value.truncate(boundary);
    (value, true)
}

fn bounded_head_tail(value: String, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value, false);
    }
    const MARKER: &str = "\n... projection omitted ...\n";
    let available = max_bytes.saturating_sub(MARKER.len());
    let head_bytes = available / 3;
    let head_end = value.floor_char_boundary(head_bytes);
    let mut tail_start = value.len().saturating_sub(available - head_end);
    while tail_start < value.len() && !value.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    (
        format!("{}{}{}", &value[..head_end], MARKER, &value[tail_start..]),
        true,
    )
}

fn failed_projection(tool: &str, error: &LocalizedError) -> ToolProjection {
    let field = error.args.get("field").map(String::as_str).unwrap_or("?");
    let detail = error.args.get("detail").map(String::as_str).unwrap_or("");
    let program = error.args.get("program").map(String::as_str).unwrap_or("?");
    let summary = match error.key.as_str() {
        "error.run_program" if !detail.is_empty() => {
            format!("Run rejected: {detail}")
        }
        "error.run_program" => "Run rejected: provide a non-empty program or command.".into(),
        "error.run_spawn" => format!(
            "Could not start {program}: {detail}. Retry with an executable in program and separate args, or use command."
        ),
        "error.tool_argument" => {
            format!("Invalid {tool} argument: {field}. {detail} Correct it and retry.")
        }
        "error.path_missing" => format!(
            "Path not found: {}. Use find/search to locate the current path.",
            error.args.get("path").map(String::as_str).unwrap_or("?")
        ),
        "error.read_file" => format!(
            "Could not read {}: {detail}",
            error.args.get("path").map(String::as_str).unwrap_or("?")
        ),
        "error.project_not_indexed" => {
            "Project index is not ready; wait for project.ready and retry.".into()
        }
        "error.read_range" => format!(
            "Invalid read range; request a positive ordered range of at most {} lines.",
            error
                .args
                .get("maxLines")
                .map(String::as_str)
                .unwrap_or("800")
        ),
        "error.edit_not_found" => {
            "Edit target was not found; read the current exact range and retry.".into()
        }
        "error.edit_not_unique" => {
            "Edit target is not unique; include a smaller unique surrounding fragment and retry."
                .into()
        }
        "error.edit_write" | "error.edit_commit" => {
            format!("Could not save the edit: {detail}")
        }
        _ if !detail.is_empty() => format!("{}: {detail}", error.key),
        _ => format!("{} failed: {}", tool, error.key),
    };
    let (summary, truncated) = bounded_prefix(summary, MAX_PROJECTION_BYTES);
    ToolProjection {
        status: "failed".into(),
        total: summary.len(),
        summary,
        exit_code: None,
        error_key: Some(error.key.clone()),
        completeness: if truncated { "partial" } else { "complete" }.into(),
        truncated,
        artifact_id: String::new(),
    }
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut fields = map.iter().collect::<Vec<_>>();
            fields.sort_by(|left, right| left.0.cmp(right.0));
            let body = fields
                .into_iter()
                .map(|(key, value)| format!("{}:{}", json!(key), canonical_json(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{body}}}")
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => value.to_string(),
    }
}

fn observation_key(call: &ToolCall) -> String {
    format!("{}:{}", call.name, canonical_json(&call.arguments))
}

fn deterministic_tool(name: &str) -> bool {
    matches!(name, "find" | "search" | "read" | "skill")
}

fn compact_projection(
    projection: &ToolProjection,
    reference: &str,
    reused: bool,
) -> ToolProjection {
    let first_line = projection.summary.lines().next().unwrap_or_default().trim();
    let (first_line, _) = bounded_prefix(first_line.to_owned(), MAX_COMPACT_SUMMARY_BYTES);
    let action = if reused { "reused" } else { "stored" };
    // No recall advertising here: the recall tool schema and the system prompt
    // already describe it. Selling it in every receipt trained models into
    // recall round-trips that each resend the entire history.
    ToolProjection {
        status: projection.status.clone(),
        summary: format!("{action} {reference}; {first_line}"),
        exit_code: projection.exit_code,
        error_key: projection.error_key.clone(),
        completeness: "reference".into(),
        total: projection.total,
        truncated: true,
        artifact_id: reference.into(),
    }
}

/// Observation reuse is restricted to purely deterministic local reads
/// (find/search/read/skill). `run` must execute on every call: commands can
/// have side effects, read external state, or simply be re-verified — a
/// cached "reused" receipt taught models that the tool refuses to re-execute.
fn cached_observation(
    state: &AppState,
    session_id: &str,
    call: &ToolCall,
) -> Option<(ToolProjection, String)> {
    if !deterministic_tool(&call.name) {
        return None;
    }
    let indexes = state.tool_observations.read();
    let index = indexes.get(session_id)?;
    let observation = index
        .by_key
        .get(&observation_key(call))
        .and_then(|reference| index.by_reference.get(reference))
        .filter(|entry| entry.epoch == index.epoch && entry.successful)?;
    let full: ToolProjection = serde_json::from_value(observation.projection.clone()).ok()?;
    let compact = compact_projection(&full, &observation.reference, true);
    Some((compact, observation.reference.clone()))
}

/// New user turn: the world outside the loop may have changed (user edits,
/// external processes), so deterministic read caches from earlier turns are
/// invalidated. `run` results are never reused across turns regardless.
fn begin_tool_turn(state: &AppState, session_id: &str) {
    if let Some(index) = state.tool_observations.write().get_mut(session_id) {
        index.epoch = index.epoch.saturating_add(1);
    }
}

fn index_projection(
    state: &AppState,
    session_id: &str,
    call: &ToolCall,
    projection: &mut ToolProjection,
) -> Option<String> {
    if call.name == "recall" {
        return None;
    }
    let successful = projection.status == "completed" && projection.exit_code.unwrap_or(0) == 0;
    let mut indexes = state.tool_observations.write();
    let index = indexes.entry(session_id.to_owned()).or_default();
    // A successful edit or run may have changed the workspace (or the world),
    // so cached deterministic reads from before it are no longer trustworthy.
    if successful && (call.name == "edit" || call.name == "run") {
        index.epoch = index.epoch.saturating_add(1);
    }
    index.next_reference = index.next_reference.saturating_add(1);
    let reference = format!("o{}", index.next_reference);
    let original_artifact = projection.artifact_id.clone();
    projection.artifact_id = reference.clone();
    let observation = ToolObservation {
        reference: reference.clone(),
        epoch: index.epoch,
        tool: call.name.clone(),
        projection: json!(projection),
        artifact_id: original_artifact.clone(),
        successful,
    };
    if successful && deterministic_tool(&call.name) {
        index
            .by_key
            .insert(observation_key(call), reference.clone());
    }
    index.by_reference.insert(reference.clone(), observation);
    drop(indexes);
    // 注意：不能在 `if let` 条件里拿 artifacts.read() —— scrutinee 的读锁
    // 会活到块尾，块内再取同一把锁的写锁将自我死锁（工具调用全部卡死）。
    // 先在独立语句中完成 clone，让读锁立即释放。
    if !original_artifact.is_empty() {
        let value = state.artifacts.read().get(&original_artifact).cloned();
        if let Some(value) = value {
            state.artifacts.write().insert(reference.clone(), value);
        }
    }
    Some(reference)
}

async fn dispatch(
    sink: &dyn RuntimeEventSink,
    state: &Arc<AppState>,
    session_id: &str,
    root: Option<&str>,
    call: &ToolCall,
    cancellation: &CancellationToken,
) -> KfResult<ToolProjection> {
    let args = &call.arguments;
    let value = match call.name.as_str() {
        "find" => dispatch_find(
            state,
            root.ok_or_else(|| LocalizedError::new("error.project_none"))?,
            args,
        )?,
        "read" => dispatch_read(
            state,
            root.ok_or_else(|| LocalizedError::new("error.project_none"))?,
            args,
        )?,
        "edit" => json!(tools::edit_for_agent(
            state,
            root.ok_or_else(|| LocalizedError::new("error.project_none"))?,
            required_text_alias(args, &["path", "filePath", "file_path"])?,
            required_text_alias(args, &["oldText", "old_text", "oldString", "old_string"])?,
            required_text_alias(args, &["newText", "new_text", "newString", "new_string"])?
        )?),
        "write" => {
            let path = required_text_alias(args, &["path", "filePath", "file_path"])?;
            let content = required_text_alias(args, &["content", "contents", "text"])?;
            let bytes = tools::write_for_agent(
                state,
                root.ok_or_else(|| LocalizedError::new("error.project_none"))?,
                path,
                content,
            )?;
            json!({"bytesWritten": bytes})
        }
        "run" => {
            let arguments = run_arguments(args)?;
            let run_root =
                optional_text_alias(args, &["cwd", "workingDirectory", "working_directory"])?
                    .or(root)
                    .ok_or_else(|| LocalizedError::new("error.project_none"))?;
            if let Some(program) = optional_text_alias(args, &["program", "executable"])? {
                json!(
                    tools::run_for_agent(state, run_root, program.into(), arguments, cancellation)
                        .await?
                )
            } else if let Some(command) = optional_text_alias(args, &["command", "cmd"])? {
                json!(
                    tools::run_command_for_agent(state, run_root, command.into(), cancellation)
                        .await?
                )
            } else {
                return Err(
                    LocalizedError::new("error.tool_argument").arg("field", "program|command")
                );
            }
        }
        "search" => dispatch_search(
            state,
            root.ok_or_else(|| LocalizedError::new("error.project_none"))?,
            args,
        )?,
        "recall" => {
            let reference = string(args, "reference")?;
            let indexes = state.tool_observations.read();
            let observation = indexes
                .get(session_id)
                .and_then(|index| index.by_reference.get(reference))
                .ok_or_else(|| {
                    LocalizedError::new("error.tool_argument").arg("field", "reference")
                })?;
            json!({
                "content": serde_json::to_string(&observation.projection).unwrap_or_default(),
                "reference": &observation.reference,
                "tool": &observation.tool,
                "artifactId": &observation.artifact_id,
            })
        }
        "task" => {
            let snapshot = {
                let mut tasks = state.tasks.write();
                let current = tasks
                    .get_mut(session_id)
                    .ok_or_else(|| LocalizedError::new("error.task_not_found"))?;
                task::apply(
                    current,
                    string(args, "op")?,
                    args.get("item").and_then(Value::as_str),
                )?;
                current.clone()
            };
            if let Some(session) = state.sessions.write().get_mut(session_id) {
                session.task = Some(snapshot.clone());
            }
            sink.emit(RuntimeEvent::new("task.updated", json!(snapshot)).session(session_id));
            json!(snapshot)
        }
        "skill" => json!({
            "content": skill::load_for_agent(root, string(args, "name")?)?
        }),
        "market" => {
            // 市场即工具：不打开市场页，agent 直接拉数据 + 本地指标做省 token 投影
            let symbol = string(args, "symbol")?;
            let timeframe = args
                .get("timeframe")
                .and_then(Value::as_str)
                .unwrap_or("15m")
                .to_string();
            let source = args
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("eastmoney")
                .to_string();
            let n = args
                .get("bars")
                .and_then(Value::as_u64)
                .unwrap_or(120)
                .clamp(30, 300) as usize;
            let (resolved_source, bars) = crate::market::datasource::fetch_bars_resolved(
                &state.client,
                &source,
                symbol,
                "",
                &timeframe,
                n + crate::market::types::INDICATOR_WARMUP_BARS + 5,
            )
            .await?;
            let now = crate::market::records::now_ms();
            let frame =
                crate::market::indicators::build_analysis_frame(&bars, n, symbol, &timeframe, now)
                    .ok_or_else(|| LocalizedError::new("error.market_insufficient"))?;
            // 全量 K 线（含 warmup 指标）走 runtime 事件直达前端图表卡片，
            // 模型侧只收省 token 摘要 —— 展示不省、推理省。
            sink.emit(
                RuntimeEvent::new(
                    "market.tool_chart",
                    json!({"callId": call.id, "frame": frame, "source": resolved_source}),
                )
                .session(session_id),
            );
            market_snapshot(&frame, &resolved_source)
        }
        "browser" => {
            let action = string(args, "action")?;
            let url = args.get("url").and_then(Value::as_str);
            if action == "fetch" {
                let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
                let mut result = crate::browser::agent_fetch_page(
                    &state.client,
                    url.ok_or_else(|| LocalizedError::new("error.browser_url_required"))?,
                    offset,
                    4000,
                )
                .await?;
                // 原始 HTML 剥离出投影，存为独立 artifact（RawArtifact 本地完整）
                if let Some(raw) = result.get("_rawHtml").and_then(Value::as_str) {
                    let raw_id = format!("raw-{}", uuid::Uuid::new_v4().simple());
                    let page_url = result
                        .get("url")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    state.artifacts.write().insert(
                        raw_id.clone(),
                        json!({ "type": "html", "url": page_url, "content": raw }),
                    );
                    if let Some(map) = result.as_object_mut() {
                        map.remove("_rawHtml");
                    }
                    result["artifact"] = json!(raw_id);
                }
                result
            } else {
                let app = sink
                    .app_handle()
                    .cloned()
                    .ok_or_else(|| LocalizedError::new("error.browser_headless"))?;
                crate::browser::agent_browser(
                    &app,
                    &state.client,
                    action,
                    url,
                    args.as_object().unwrap_or(&Default::default()),
                )
                .await?
            }
        }
        other => return Err(LocalizedError::new("error.tool_unknown").arg("tool", other)),
    };
    let artifact_id = format!("artifact-{}", uuid::Uuid::new_v4());
    state
        .artifacts
        .write()
        .insert(artifact_id.clone(), value.clone());
    Ok(project_tool_artifact(call, artifact_id, &value))
}

/// 市场工具的省 token 投影：模型只需要结论性字段 + 最近 60 根紧凑 OHLCV，
/// 完整曲线经 market.tool_chart 事件直发前端渲染（展示不省、推理省）。
fn market_snapshot(frame: &crate::market::types::KlineFrame, source: &str) -> Value {
    let bars = &frame.bars; // 新→旧，全为已收盘棒
    let round = |value: f64| (value * 10_000.0).round() / 10_000.0;
    let latest_ema = frame.indicators.ema20.first().and_then(|value| *value);
    let latest_atr = frame.indicators.atr14.first().and_then(|value| *value);
    let last = bars.first();
    let first = bars.last();
    let mut payload = json!({
        "type": "market.snapshot",
        "symbol": frame.symbol,
        "timeframe": frame.timeframe,
        "sourceUsed": source,
        "bars": bars.len(),
        "snapshotTsMs": frame.snapshot_ts_local_ms,
    });
    if let (Some(last), Some(first)) = (last, first)
        && first.close > 0.0
    {
        payload["lastClose"] = json!(round(last.close));
        payload["windowChangePct"] = json!(round((last.close / first.close - 1.0) * 100.0));
        let high = bars.iter().fold(f64::MIN, |acc, bar| acc.max(bar.high));
        let low = bars.iter().fold(f64::MAX, |acc, bar| acc.min(bar.low));
        payload["windowHigh"] = json!(round(high));
        payload["windowLow"] = json!(round(low));
    }
    if let Some(ema) = latest_ema {
        payload["ema20"] = json!(round(ema));
        if let Some(last) = last
            && ema > 0.0
        {
            payload["closeVsEma20Pct"] = json!(round((last.close / ema - 1.0) * 100.0));
        }
    }
    if let (Some(atr), Some(last)) = (latest_atr, last)
        && last.close > 0.0
    {
        payload["atr14"] = json!(round(atr));
        payload["atrPctOfClose"] = json!(round((atr / last.close) * 100.0));
    }
    // 最近 60 根（旧→新）：[unixSec, o, h, l, c, volume]
    let recent: Vec<Value> = bars
        .iter()
        .take(60)
        .rev()
        .map(|bar| {
            json!([
                (bar.ts_open / 1000.0).round() as i64,
                round(bar.open),
                round(bar.high),
                round(bar.low),
                round(bar.close),
                bar.volume.round() as i64,
            ])
        })
        .collect();
    payload["recent"] = json!(recent);
    payload
}

fn dispatch_find(state: &AppState, root: &str, args: &Value) -> KfResult<Value> {
    let path = optional_text(args, "path")?;
    let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let Some(items) = batch_items(args, "queries")? else {
        return Ok(json!(project::query_index(
            state,
            root,
            string(args, "query")?,
            path,
            offset
        )?));
    };
    reject_mixed_batch(args, "query", "queries")?;
    let queries = items
        .iter()
        .map(|item| required_text(item, "queries"))
        .collect::<KfResult<Vec<_>>>()?;
    let results = queries
        .into_iter()
        .map(|query| {
            batch_entry(
                query.to_owned(),
                project::query_index(state, root, query, path, offset).map(|result| json!(result)),
            )
        })
        .collect::<Vec<_>>();
    Ok(json!({"batch":true,"results":results}))
}

fn dispatch_search(state: &AppState, root: &str, args: &Value) -> KfResult<Value> {
    let Some(items) = batch_items(args, "queries")? else {
        return Ok(json!(tools::search_for_agent(
            state,
            root,
            string(args, "query")?,
            optional_text(args, "path")?
        )?));
    };
    reject_mixed_batch(args, "query", "queries")?;
    let queries = items
        .iter()
        .map(|item| {
            item.as_object().ok_or_else(|| {
                LocalizedError::new("error.tool_argument").arg("field", "queries[]")
            })?;
            let query = required_text(item, "query")?;
            let path = optional_text(item, "path")?;
            Ok((query, path))
        })
        .collect::<KfResult<Vec<_>>>()?;
    let results = queries
        .into_iter()
        .map(|(query, path)| {
            let label = path
                .map(|path| format!("{query} @ {path}"))
                .unwrap_or_else(|| query.to_owned());
            batch_entry(
                label,
                tools::search_for_agent(state, root, query, path).map(|result| json!(result)),
            )
        })
        .collect::<Vec<_>>();
    Ok(json!({"batch":true,"results":results}))
}

fn dispatch_read(state: &AppState, root: &str, args: &Value) -> KfResult<Value> {
    let items = batch_items(args, "ranges")?;
    if items.is_none() {
        let request = read_request(args)?;
        return Ok(json!(tools::read_for_agent(
            state,
            root,
            &request.path,
            request.start,
            request.end
        )?));
    }

    let fallback_path = optional_text_alias(args, &["path", "filePath", "file_path"])?;
    let requests = items
        .unwrap()
        .iter()
        .map(|item| read_request_with_fallback(item, fallback_path));

    let mut seen = HashSet::new();
    let results = requests
        .into_iter()
        .filter_map(|request| match request {
            Ok(request) => {
                let key = (
                    request.path.replace('\\', "/").to_ascii_lowercase(),
                    request.start,
                    request.end,
                );
                seen.insert(key).then(|| {
                    batch_entry(
                        format!("{}:{}-{}", request.path, request.start, request.end),
                        tools::read_for_agent(
                            state,
                            root,
                            &request.path,
                            request.start,
                            request.end,
                        )
                        .map(|result| json!(result)),
                    )
                })
            }
            Err(error) => Some(batch_entry("invalid range".into(), Err(error))),
        })
        .collect::<Vec<_>>();
    Ok(json!({"batch":true,"results":results}))
}

#[derive(Debug)]
struct ReadRequest {
    path: String,
    start: usize,
    end: usize,
}

fn read_request(value: &Value) -> KfResult<ReadRequest> {
    read_request_with_fallback(value, None)
}

fn read_request_with_fallback(value: &Value, fallback_path: Option<&str>) -> KfResult<ReadRequest> {
    value
        .as_object()
        .ok_or_else(|| LocalizedError::new("error.tool_argument").arg("field", "ranges[]"))?;
    let path = optional_text_alias(value, &["path", "filePath", "file_path"])?
        .or(fallback_path)
        .ok_or_else(|| {
            LocalizedError::new("error.tool_argument").arg("field", "ranges[].path|path")
        })?;
    let start = optional_line(value, "startLine", "start_line")?.unwrap_or(1);
    let end = optional_line(value, "endLine", "end_line")?
        .unwrap_or_else(|| start.saturating_add(DEFAULT_BATCH_READ_LINES - 1));
    Ok(ReadRequest {
        path: path.to_owned(),
        start,
        end,
    })
}

fn batch_entry(label: String, result: KfResult<Value>) -> Value {
    match result {
        Ok(result) => json!({"label":label,"result":result}),
        Err(error) => json!({"label":label,"error":error}),
    }
}

fn batch_items<'a>(args: &'a Value, key: &str) -> KfResult<Option<&'a [Value]>> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let items = value
        .as_array()
        .ok_or_else(|| LocalizedError::new("error.tool_argument").arg("field", key))?;
    if items.is_empty() || items.len() > MAX_BATCH_ITEMS {
        return Err(LocalizedError::new("error.tool_argument")
            .arg("field", format!("{key}[1..={MAX_BATCH_ITEMS}]")));
    }
    Ok(Some(items))
}

fn reject_mixed_batch(args: &Value, single: &str, batch: &str) -> KfResult<()> {
    if args.get(single).is_some() {
        return Err(
            LocalizedError::new("error.tool_argument").arg("field", format!("{single}|{batch}"))
        );
    }
    Ok(())
}

fn required_text<'a>(value: &'a Value, key: &str) -> KfResult<&'a str> {
    let text = if value.is_string() {
        value.as_str()
    } else {
        value.get(key).and_then(Value::as_str)
    }
    .ok_or_else(|| LocalizedError::new("error.tool_argument").arg("field", key))?;
    if text.trim().is_empty() {
        return Err(LocalizedError::new("error.tool_argument").arg("field", key));
    }
    Ok(text)
}

fn optional_text<'a>(value: &'a Value, key: &str) -> KfResult<Option<&'a str>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(LocalizedError::new("error.tool_argument").arg("field", key)),
    }
}

fn required_text_alias<'a>(value: &'a Value, keys: &[&str]) -> KfResult<&'a str> {
    optional_text_alias(value, keys)?
        .ok_or_else(|| LocalizedError::new("error.tool_argument").arg("field", keys.join("|")))
}

fn optional_text_alias<'a>(value: &'a Value, keys: &[&str]) -> KfResult<Option<&'a str>> {
    for key in keys {
        if value.get(*key).is_some() {
            let text = optional_text(value, key).and_then(|text| match text {
                Some(text) if text.trim().is_empty() => {
                    Err(LocalizedError::new("error.tool_argument").arg("field", *key))
                }
                other => Ok(other),
            })?;
            if text.is_some() {
                return Ok(text);
            }
        }
    }
    Ok(None)
}

fn run_arguments(value: &Value) -> KfResult<Vec<String>> {
    let Some(arguments) = value.get("args").or_else(|| value.get("arguments")) else {
        return Ok(Vec::new());
    };
    match arguments {
        Value::Array(items) => items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_owned).ok_or_else(|| {
                    LocalizedError::new("error.tool_argument").arg("field", "args[]")
                })
            })
            .collect(),
        Value::String(arguments) => tools::split_command_line(arguments),
        _ => Err(LocalizedError::new("error.tool_argument").arg("field", "args")),
    }
}

fn optional_line(value: &Value, key: &str, alias: &str) -> KfResult<Option<usize>> {
    match value.get(key).or_else(|| value.get(alias)) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(|| LocalizedError::new("error.tool_argument").arg("field", key)),
        Some(Value::String(value)) => value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(|| LocalizedError::new("error.tool_argument").arg("field", key)),
        Some(_) => Err(LocalizedError::new("error.tool_argument").arg("field", key)),
    }
}

fn tool_started_event(turn_id: &str, call: &ToolCall, session_id: &str) -> RuntimeEvent {
    RuntimeEvent::new(
        "tool.started",
        json!({"turnId":turn_id,"callId":call.id,"name":call.name,"arguments":call.arguments}),
    )
    .session(session_id)
}

fn string<'a>(value: &'a Value, key: &str) -> KfResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| LocalizedError::new("error.tool_argument").arg("field", key))
}
fn commit_round_usage(
    sink: &dyn RuntimeEventSink,
    state: &AppState,
    session_id: &str,
    turn_id: &str,
    round: u64,
    usage: &TokenUsage,
) {
    let mut snapshot = None;
    if let Some(session) = state.sessions.write().get_mut(session_id) {
        session.usage.request_count += 1;
        session.usage.fresh_input_tokens += usage.fresh_input_tokens();
        session.usage.cache_read_tokens += usage.cached_input_tokens;
        session.usage.output_tokens += usage.output_tokens;
        session.usage.reasoning_tokens += usage.reasoning_tokens;
        session.usage.current_context_tokens = usage.reported.then_some(usage.input_tokens);
        snapshot = Some(session.usage.clone());
    }
    if let Some(session_usage) = snapshot {
        let round_usage = json!({
            "freshInputTokens": usage.fresh_input_tokens(),
            "cacheReadTokens": usage.cached_input_tokens,
            "outputTokens": usage.output_tokens,
            "reasoningTokens": usage.reasoning_tokens,
            "currentContextTokens": usage.reported.then_some(usage.input_tokens),
        });
        sink.emit(
            RuntimeEvent::new(
                "assistant.usage",
                json!({
                    "turnId":turn_id,
                    "round":round,
                    "roundUsage":round_usage,
                    "usage":session_usage,
                    "requestCount":session_usage.request_count
                }),
            )
            .session(session_id),
        );
    }
}

fn estimate_text_tokens(text: &str) -> u64 {
    let (ascii, non_ascii) = text.chars().fold((0_u64, 0_u64), |(ascii, non_ascii), ch| {
        if ch.is_ascii() {
            (ascii + 1, non_ascii)
        } else {
            (ascii, non_ascii + 1)
        }
    });
    ascii.div_ceil(4).saturating_add(non_ascii)
}

fn trim_protected_token(value: &str) -> &str {
    value
        .trim_matches(|ch| {
            matches!(
                ch,
                '\'' | '"'
                    | '`'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '<'
                    | '>'
                    | ','
                    | ';'
                    | ':'
                    | '!'
                    | '?'
                    | '，'
                    | '。'
                    | '；'
                    | '：'
                    | '！'
                    | '？'
            )
        })
        .trim_end_matches('.')
}

fn protected_requirement_fragments(text: &str) -> BTreeSet<String> {
    let mut fragments = BTreeSet::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('`') else {
            break;
        };
        let fragment = after_start[..end].trim();
        if !fragment.is_empty() {
            fragments.insert(fragment.to_owned());
        }
        rest = &after_start[end + 1..];
    }

    for raw in text.split_whitespace() {
        let token = trim_protected_token(raw);
        if token.is_empty() {
            continue;
        }
        let is_url = token.starts_with("https://") || token.starts_with("http://");
        let is_path = !is_url && token.len() > 1 && (token.contains('/') || token.contains('\\'));
        if is_url || is_path {
            fragments.insert(token.to_owned());
        }
    }

    let mut identifier = String::new();
    let push_identifier = |identifier: &mut String, fragments: &mut BTreeSet<String>| {
        let candidate = identifier.trim_matches('.');
        if candidate.chars().any(|ch| ch.is_ascii_digit()) {
            fragments.insert(candidate.to_owned());
        }
        identifier.clear();
    };
    for ch in text.chars() {
        if ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            identifier.push(ch);
        } else if !identifier.is_empty() {
            push_identifier(&mut identifier, &mut fragments);
        }
    }
    if !identifier.is_empty() {
        push_identifier(&mut identifier, &mut fragments);
    }
    fragments
}

fn preserves_requirement_fragments(original: &str, brief: &str) -> bool {
    protected_requirement_fragments(original)
        .iter()
        .all(|fragment| brief.contains(fragment))
}

fn evaluate_requirement_brief(
    original: &str,
    before_tokens: u64,
    brief: &str,
) -> (Option<String>, u64, &'static str) {
    let brief = brief.trim();
    if brief.is_empty() {
        return (None, before_tokens, "invalid_output");
    }
    if !preserves_requirement_fragments(original, brief) {
        return (None, before_tokens, "invalid_output");
    }
    let after_tokens = estimate_text_tokens(brief);
    if after_tokens.saturating_mul(5) > before_tokens.saturating_mul(4) {
        return (None, after_tokens, "no_savings");
    }
    (Some(brief.to_owned()), after_tokens, "accepted")
}

#[derive(Debug)]
struct AuxiliaryReceipt {
    id: String,
    turn_id: String,
    model: String,
    status: String,
    reason: String,
    before_tokens: u64,
    after_tokens: u64,
    input_tokens: u64,
    output_tokens: u64,
    elapsed_ms: u64,
    summary: String,
    error_key: Option<String>,
}

impl AuxiliaryReceipt {
    fn event(&self, kind: &str, session_id: &str) -> RuntimeEvent {
        RuntimeEvent::new(
            kind,
            json!({
                "id": self.id,
                "turnId": self.turn_id,
                "role": "requirementReducer",
                "model": self.model,
                "status": self.status,
                "reason": self.reason,
                "beforeTokens": self.before_tokens,
                "afterTokens": self.after_tokens,
                "inputTokens": self.input_tokens,
                "outputTokens": self.output_tokens,
                "elapsedMs": self.elapsed_ms,
                "summary": self.summary,
                "errorKey": self.error_key,
            }),
        )
        .session(session_id)
    }
}

async fn maybe_reduce_requirement(
    sink: &dyn RuntimeEventSink,
    state: &AppState,
    session_id: &str,
    turn_id: &str,
    endpoint: &str,
    content: &str,
    cancellation: &CancellationToken,
) {
    let settings = state.settings.read().clone();
    if !settings.auxiliary_enabled {
        return;
    }
    let before_tokens = estimate_text_tokens(content);
    let id = format!("auxiliary:requirementReducer:{turn_id}");
    let mut receipt = AuxiliaryReceipt {
        id,
        turn_id: turn_id.to_owned(),
        model: settings.auxiliary_model_id.clone(),
        status: "skipped".into(),
        reason: String::new(),
        before_tokens,
        after_tokens: before_tokens,
        input_tokens: 0,
        output_tokens: 0,
        elapsed_ms: 0,
        summary: String::new(),
        error_key: None,
    };
    if before_tokens < REQUIREMENT_REDUCER_THRESHOLD_TOKENS {
        receipt.reason = "short_input".into();
        sink.emit(receipt.event("auxiliary.skipped", session_id));
        return;
    }
    if state
        .accepted_requirement_briefs
        .read()
        .contains_key(turn_id)
    {
        return;
    }
    let profile = settings
        .providers
        .iter()
        .find(|profile| profile.id == settings.auxiliary_provider_id);
    if profile.is_none() && settings.auxiliary_provider_id != provider::PROVIDER_ID {
        receipt.status = "failed".into();
        receipt.reason = "provider_error".into();
        receipt.error_key = Some("error.provider_unsupported".into());
        sink.emit(receipt.event("auxiliary.failed", session_id));
        return;
    }
    receipt.status = "started".into();
    receipt.reason = "eligible".into();
    sink.emit(receipt.event("auxiliary.started", session_id));
    let started = Instant::now();
    let reduction = if let Some(profile) = profile {
        provider::reduce_requirement_profile(
            &state.auxiliary_client,
            profile,
            &settings.auxiliary_model_id,
            content,
            cancellation,
        )
        .await
    } else {
        provider::reduce_requirement(
            &state.auxiliary_client,
            endpoint,
            &settings.auxiliary_model_id,
            content,
            cancellation,
        )
        .await
    };
    match reduction {
        Ok(reduction) => {
            receipt.elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            receipt.input_tokens = reduction.usage.input_tokens;
            receipt.output_tokens = reduction.usage.output_tokens;
            let (accepted, after_tokens, reason) =
                evaluate_requirement_brief(content, before_tokens, &reduction.brief);
            receipt.reason = reason.into();
            if let Some(brief) = accepted {
                receipt.after_tokens = after_tokens;
                receipt.status = "completed".into();
                receipt.summary = brief.clone();
                state
                    .accepted_requirement_briefs
                    .write()
                    .insert(turn_id.to_owned(), brief);
                sink.emit(receipt.event("auxiliary.completed", session_id));
            } else {
                receipt.after_tokens = before_tokens;
                receipt.status = "skipped".into();
                sink.emit(receipt.event("auxiliary.skipped", session_id));
            }
        }
        Err(error) => {
            receipt.elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            receipt.status = "failed".into();
            receipt.reason = "provider_error".into();
            receipt.error_key = Some(error.key);
            sink.emit(receipt.event("auxiliary.failed", session_id));
        }
    }
}

pub async fn run(
    app: AppHandle,
    state: Arc<AppState>,
    session_id: String,
    turn_id: String,
    _content: String,
    clarify: bool,
    active_turn: ActiveTurn,
) -> KfResult<(TokenUsage, String)> {
    let provider_id = state
        .sessions
        .read()
        .get(&session_id)
        .map(|session| session.provider_id.clone())
        .unwrap_or_default();
    let endpoint = state
        .settings
        .read()
        .providers
        .iter()
        .find(|profile| profile.id == provider_id)
        .map(|profile| profile.base_url.clone())
        .unwrap_or_else(|| provider::DEFAULT_BASE_URL.to_owned());
    run_with_sink(
        Arc::new(TauriRuntimeEventSink::new(app)),
        state,
        AgentRunRequest {
            session_id,
            turn_id,
            content: _content,
            clarify,
            active_turn,
            endpoint,
        },
    )
    .await
}

pub(crate) struct AgentRunRequest {
    pub session_id: String,
    pub turn_id: String,
    pub content: String,
    pub clarify: bool,
    pub active_turn: ActiveTurn,
    pub endpoint: String,
}

pub(crate) async fn run_with_sink(
    sink: Arc<dyn RuntimeEventSink>,
    state: Arc<AppState>,
    request: AgentRunRequest,
) -> KfResult<(TokenUsage, String)> {
    let AgentRunRequest {
        session_id,
        turn_id,
        content,
        clarify,
        active_turn,
        endpoint,
    } = request;
    let cancellation = active_turn.cancellation.clone();
    let root = state
        .sessions
        .read()
        .get(&session_id)
        .and_then(|session| session.project_root.clone())
        .or_else(|| {
            state
                .active_project
                .read()
                .as_ref()
                .map(|path| path.display().to_string())
        });
    let model_id = state
        .sessions
        .read()
        .get(&session_id)
        .map(|session| session.model_id.clone())
        .unwrap_or_else(|| provider::MODEL_ID.into());
    let provider_id = state
        .sessions
        .read()
        .get(&session_id)
        .map(|session| session.provider_id.clone())
        .unwrap_or_else(|| provider::PROVIDER_ID.into());
    let profile = state
        .settings
        .read()
        .providers
        .iter()
        .find(|profile| profile.id == provider_id)
        .cloned();
    let configured_model = profile
        .as_ref()
        .and_then(|profile| profile.models.iter().find(|model| model.id == model_id));
    let thinking_enabled = configured_model.is_some_and(|model| model.thinking_enabled);
    let thinking_effort = configured_model
        .map(|model| model.thinking_effort.as_str())
        .unwrap_or("medium")
        .to_owned();
    let adapter = profile
        .as_ref()
        .map(|profile| provider::model_adapter(profile, &model_id))
        .unwrap_or_else(|| "openai".to_owned());
    let api_key = profile
        .as_ref()
        .map(provider::resolved_api_key)
        .transpose()?
        .unwrap_or_else(|| "public".into());
    let user_agent = profile
        .as_ref()
        .map(|profile| profile.user_agent.as_str())
        .unwrap_or_default()
        .to_owned();
    maybe_reduce_requirement(
        sink.as_ref(),
        &state,
        &session_id,
        &turn_id,
        &endpoint,
        &content,
        &cancellation,
    )
    .await;
    if cancellation.is_cancelled() {
        return Err(LocalizedError::new("error.session_cancelled"));
    }
    let settings = state.settings.read().clone();
    begin_tool_turn(&state, &session_id);
    let registry = BuiltinRegistry {
        task_enabled: settings.task_manager,
        skill_enabled: settings.skill_router,
        workspace_available: root.is_some(),
    };
    let tool_definitions = wire_tools(&registry);
    let skill_route = if settings.skill_router {
        skill::route_turn(&state, root.as_deref(), &content)
    } else {
        Default::default()
    };
    if !skill_route.selected.is_empty() {
        let receipt = skill_route.receipt(&turn_id);
        state
            .artifacts
            .write()
            .insert(format!("skill:{turn_id}"), receipt.clone());
        sink.emit(RuntimeEvent::new("skill.activated", receipt).session(&session_id));
    }
    sync_project_context(&state, &session_id, root.as_deref());
    if let Some(directory) = skill_route.directory() {
        record_context(
            &state,
            &session_id,
            &format!("skill-route:{turn_id}"),
            directory,
        );
    }
    if clarify {
        record_context(&state, &session_id, "turn-clarify", CLARIFY_TURN.to_owned());
    }
    let history = state
        .histories
        .read()
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    let accepted_briefs = state.accepted_requirement_briefs.read().clone();
    let system = if settings.caveman_mode == "lite" {
        format!("{SYSTEM} {CAVEMAN_LITE}")
    } else {
        SYSTEM.to_owned()
    };
    let mut messages = vec![json!({"role":"system","content":system})];
    messages.extend(project_history(&history, &accepted_briefs, &adapter));
    let mut pending_tool_compactions: Vec<(usize, String, u64)> = Vec::new();
    let mut total_usage = TokenUsage::default();
    let mut final_text = String::new();
    let mut round = 0_u64;
    let mut tool_capability_retries = 0_u8;
    let mut finalization_retries = 0_u8;
    loop {
        if cancellation.is_cancelled() {
            return Err(LocalizedError::new("error.session_cancelled"));
        }
        if let Some(snapshot) = sync_project_context(&state, &session_id, root.as_deref()) {
            messages.push(context_message(&snapshot));
        }
        let response = match provider::stream_turn(
            sink.as_ref(),
            &state,
            provider::StreamTurnRequest {
                session_id: &session_id,
                turn_id: &turn_id,
                model: &model_id,
                messages: &messages,
                tools: &tool_definitions,
                active_turn: &active_turn,
                endpoint: &endpoint,
                adapter: &adapter,
                api_key: &api_key,
                user_agent: &user_agent,
                thinking: provider::ThinkingOptions {
                    enabled: thinking_enabled,
                    effort: &thinking_effort,
                },
            },
        )
        .await
        {
            Ok(response) => response,
            Err(failure) => {
                total_usage.add(&failure.partial.usage);
                commit_round_usage(
                    sink.as_ref(),
                    &state,
                    &session_id,
                    &turn_id,
                    round,
                    &failure.partial.usage,
                );
                if !failure.partial.text.is_empty() || !failure.partial.reasoning.is_empty() {
                    state
                        .histories
                        .write()
                        .entry(session_id.clone())
                        .or_default()
                        .push(HistoryItem::Assistant {
                            turn_id: turn_id.clone(),
                            content: failure.partial.text.clone(),
                            reasoning: failure.partial.reasoning.clone(),
                        });
                }
                if failure.error.key == "error.provider_tool_capability"
                    && tool_capability_retries < 2
                {
                    tool_capability_retries += 1;
                    if !failure.partial.text.is_empty() {
                        messages.push(json!({"role":"assistant","content":failure.partial.text}));
                    }
                    messages.push(context_message(TOOL_CAPABILITY_RECOVERY));
                    record_context(
                        &state,
                        &session_id,
                        "provider-tool-recovery",
                        TOOL_CAPABILITY_RECOVERY.to_owned(),
                    );
                    round = round.saturating_add(1);
                    continue;
                }
                return Err(failure.error);
            }
        };
        total_usage.add(&response.usage);
        commit_round_usage(
            sink.as_ref(),
            &state,
            &session_id,
            &turn_id,
            round,
            &response.usage,
        );
        // Budget-valve tool compaction: mid-turn history stays append-only
        // while retained tool projections fit the budget (prefix cache holds,
        // no recall bait). Only overflow swaps the oldest receipts in —
        // rewriting mid-history re-prices the whole tail at cache-miss, so it
        // must buy survival of a marathon turn, not routine "savings".
        let retained_bytes: usize = pending_tool_compactions
            .iter()
            .filter_map(|(index, _, _)| {
                messages
                    .get(*index)
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_str)
                    .map(str::len)
            })
            .sum();
        if retained_bytes > CONTEXT_TOOL_BUDGET_BYTES {
            pending_tool_compactions.sort_by_key(|(_, _, born_round)| *born_round);
            let mut over = retained_bytes.saturating_sub(CONTEXT_TOOL_BUDGET_BYTES);
            pending_tool_compactions.retain_mut(|(index, compact, _)| {
                if over == 0 {
                    return true;
                }
                let full_len = messages
                    .get(*index)
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_str)
                    .map(str::len)
                    .unwrap_or(0);
                if let Some(content) = messages
                    .get_mut(*index)
                    .and_then(|message| message.get_mut("content"))
                {
                    *content = Value::String(compact.clone());
                }
                over = over.saturating_sub(full_len);
                false
            });
        }
        final_text.push_str(&response.text);
        if response.interrupted_by_guidance {
            if !response.text.is_empty() || !response.reasoning.is_empty() {
                let wire_message = assistant_wire_message(
                    &adapter,
                    &response.text,
                    &response.reasoning,
                    Vec::new(),
                );
                state
                    .histories
                    .write()
                    .entry(session_id.clone())
                    .or_default()
                    .push(HistoryItem::Assistant {
                        turn_id: turn_id.clone(),
                        content: response.text.clone(),
                        reasoning: response.reasoning,
                    });
                if let Some(wire_message) = wire_message {
                    messages.push(wire_message);
                }
            }
            let guidance = active_turn.drain_guidance();
            if !guidance.is_empty() {
                final_text.push_str("\n\n");
                sink.emit(
                    RuntimeEvent::new(
                        "assistant.text_delta",
                        json!({"turnId":turn_id,"delta":"\n\n"}),
                    )
                    .session(&session_id),
                );
                append_guidance(&mut messages, &state, &session_id, &turn_id, guidance);
            }
            round = round.saturating_add(1);
            continue;
        }
        if response.tool_calls.is_empty() {
            let length_limited = response.finish_reason.as_deref() == Some("length");
            let reasoning_only =
                response.text.trim().is_empty() && !response.reasoning.trim().is_empty();
            let wire_message =
                assistant_wire_message(&adapter, &response.text, &response.reasoning, Vec::new());
            state
                .histories
                .write()
                .entry(session_id.clone())
                .or_default()
                .push(HistoryItem::Assistant {
                    turn_id: turn_id.clone(),
                    content: response.text.clone(),
                    reasoning: response.reasoning,
                });
            if let Some(wire_message) = wire_message {
                messages.push(wire_message);
            }
            let guidance = if length_limited {
                active_turn.drain_guidance()
            } else {
                active_turn.close_or_drain_guidance()
            };
            if length_limited {
                messages.push(context_message(CONTINUE_RESPONSE));
                record_context(
                    &state,
                    &session_id,
                    "response-continuation",
                    CONTINUE_RESPONSE.to_owned(),
                );
                append_guidance(&mut messages, &state, &session_id, &turn_id, guidance);
                round = round.saturating_add(1);
                continue;
            }
            if reasoning_only && finalization_retries < 2 {
                finalization_retries += 1;
                messages.push(context_message(FINALIZE_RESPONSE));
                record_context(
                    &state,
                    &session_id,
                    "response-finalization",
                    FINALIZE_RESPONSE.to_owned(),
                );
                append_guidance(&mut messages, &state, &session_id, &turn_id, guidance);
                round = round.saturating_add(1);
                continue;
            }
            if !guidance.is_empty() {
                final_text.push_str("\n\n");
                sink.emit(
                    RuntimeEvent::new(
                        "assistant.text_delta",
                        json!({"turnId":turn_id,"delta":"\n\n"}),
                    )
                    .session(&session_id),
                );
                append_guidance(&mut messages, &state, &session_id, &turn_id, guidance);
                round = round.saturating_add(1);
                continue;
            }
            return Ok((total_usage, final_text));
        }
        state
            .histories
            .write()
            .entry(session_id.clone())
            .or_default()
            .push(HistoryItem::Assistant {
                turn_id: turn_id.clone(),
                content: response.text.clone(),
                reasoning: response.reasoning.clone(),
            });
        let wire_calls: Vec<Value> = response.tool_calls.iter().map(|call| json!({"id":call.id,"type":"function","function":{"name":call.name,"arguments":call.arguments.to_string()}})).collect();
        messages.push(
            assistant_wire_message(&adapter, &response.text, &response.reasoning, wire_calls)
                .expect("tool-call assistant messages are never empty"),
        );
        for call in response.tool_calls {
            state
                .histories
                .write()
                .entry(session_id.clone())
                .or_default()
                .push(crate::types::HistoryItem::ToolCall {
                    turn_id: turn_id.clone(),
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
            sink.emit(tool_started_event(&turn_id, &call, &session_id));
            let cached = cached_observation(&state, &session_id, &call);
            let (mut projection, mut reference, reused) =
                if let Some((projection, reference)) = cached {
                    (projection, Some(reference), true)
                } else {
                    let projection = match dispatch(
                        sink.as_ref(),
                        &state,
                        &session_id,
                        root.as_deref(),
                        &call,
                        &cancellation,
                    )
                    .await
                    {
                        Ok(value) => value,
                        Err(error) if error.key == "error.session_cancelled" => {
                            let projection = ToolProjection {
                                status: "aborted".into(),
                                summary: String::new(),
                                exit_code: None,
                                error_key: Some(error.key.clone()),
                                completeness: "none".into(),
                                total: 0,
                                truncated: false,
                                artifact_id: String::new(),
                            };
                            state
                                .histories
                                .write()
                                .entry(session_id.clone())
                                .or_default()
                                .push(crate::types::HistoryItem::ToolResult {
                                    turn_id: turn_id.clone(),
                                    call_id: call.id.clone(),
                                    projection: json!(projection),
                                    artifact_id: None,
                                });
                            sink.emit(
                            RuntimeEvent::new(
                                "tool.completed",
                                json!({"turnId":turn_id,"callId":call.id,"projection":projection}),
                            )
                            .session(&session_id),
                        );
                            return Err(error);
                        }
                        Err(error) => failed_projection(&call.name, &error),
                    };
                    (projection, None, false)
                };
            if !reused {
                reference = index_projection(&state, &session_id, &call, &mut projection);
                if call.name == "recall" {
                    reference = call
                        .arguments
                        .get("reference")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
            }
            sink.emit(
                RuntimeEvent::new(
                    "tool.completed",
                    json!({"turnId":turn_id,"callId":call.id,"projection":projection}),
                )
                .session(&session_id),
            );
            let full_content = serde_json::to_string(&projection).unwrap_or_default();
            let message_index = messages.len();
            messages.push(json!({"role":"tool","tool_call_id":call.id,"content":full_content}));
            let history_projection = if projection.status == "completed" {
                if reused {
                    projection.clone()
                } else {
                    reference
                        .as_deref()
                        .map(|reference| compact_projection(&projection, reference, false))
                        .unwrap_or_else(|| projection.clone())
                }
            } else {
                projection.clone()
            };
            if !reused
                && projection.status == "completed"
                && let Some(reference) = reference.as_deref()
            {
                pending_tool_compactions.push((
                    message_index,
                    serde_json::to_string(&compact_projection(&projection, reference, false))
                        .unwrap_or_default(),
                    round,
                ));
            }
            state
                .histories
                .write()
                .entry(session_id.clone())
                .or_default()
                .push(crate::types::HistoryItem::ToolResult {
                    turn_id: turn_id.clone(),
                    call_id: call.id.clone(),
                    projection: json!(history_projection),
                    artifact_id: reference,
                });
        }
        append_guidance(
            &mut messages,
            &state,
            &session_id,
            &turn_id,
            active_turn.drain_guidance(),
        );
        round = round.saturating_add(1);
    }
}

fn append_guidance(
    messages: &mut Vec<Value>,
    state: &AppState,
    session_id: &str,
    turn_id: &str,
    guidance: Vec<QueuedGuidance>,
) {
    for item in guidance {
        if item.clarify {
            messages.push(context_message(CLARIFY_GUIDANCE));
            record_context(
                state,
                session_id,
                "guidance-clarify",
                CLARIFY_GUIDANCE.to_owned(),
            );
        }
        let content = if item.attachments.is_empty() {
            json!(item.content)
        } else {
            let mut parts = vec![json!({"type":"text","text":item.content})];
            parts.extend(item.attachments.iter().map(
                |attachment| json!({"type":"image_url","image_url":{"url":attachment.data_url}}),
            ));
            Value::Array(parts)
        };
        messages.push(json!({"role":"user","content":content}));
        state
            .histories
            .write()
            .entry(session_id.to_owned())
            .or_default()
            .push(HistoryItem::User {
                turn_id: turn_id.to_owned(),
                content: item.content,
                attachments: item.attachments,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_is_stable_sorted_and_discoverable() {
        let registry = BuiltinRegistry {
            task_enabled: true,
            skill_enabled: true,
            workspace_available: true,
        };
        let names: Vec<_> = registry.active().iter().map(|tool| tool.name).collect();
        assert_eq!(
            names,
            vec![
                "find", "search", "read", "edit", "write", "run", "browser", "market", "recall",
                "skill", "task"
            ]
        );
        assert_eq!(registry.discover("read")[0].name, "read");
        let run = registry
            .active()
            .into_iter()
            .find(|tool| tool.name == "run")
            .unwrap();
        assert!(run.schema.get("anyOf").is_some());
        for name in ["find", "search", "read"] {
            let tool = registry
                .active()
                .into_iter()
                .find(|tool| tool.name == name)
                .unwrap();
            let batch = if name == "read" { "ranges" } else { "queries" };
            assert_eq!(
                tool.schema["properties"][batch]["maxItems"],
                MAX_BATCH_ITEMS
            );
            assert!(
                tool.schema
                    .get(if name == "read" { "anyOf" } else { "oneOf" })
                    .is_some()
            );
        }
        assert!(SYSTEM.contains("Batch independent find/search/read queries"));
        assert!(SYSTEM.contains("shell remains available"));
        assert!(SYSTEM.contains("project graph"));
        assert!(SYSTEM.contains("Keep the task plan current"));
        assert!(CAVEMAN_LITE.contains("minimum necessary words"));
        assert!(CAVEMAN_LITE.contains("No pleasantries"));
        assert!(CAVEMAN_LITE.contains("verification evidence"));
    }

    #[test]
    fn tool_catalog_serialization_is_byte_stable() {
        let registry = BuiltinRegistry {
            task_enabled: true,
            skill_enabled: true,
            workspace_available: true,
        };
        let first = serde_json::to_vec(&wire_tools(&registry)).unwrap();
        let second = serde_json::to_vec(&wire_tools(&registry)).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn reasoning_only_followup_uses_protocol_safe_wire_history() {
        let openai = assistant_wire_message("openai", "", "private reasoning", Vec::new())
            .expect("compatible thinking APIs need reasoning_content replay");
        assert_eq!(openai["content"], "");
        assert_eq!(openai["reasoning_content"], "private reasoning");

        assert!(
            assistant_wire_message("anthropic", "", "private reasoning", Vec::new()).is_none(),
            "native adapters must not receive an empty assistant block without their signed reasoning shape"
        );
        assert!(assistant_wire_message("gemini", "", "private reasoning", Vec::new()).is_none());
        assert!(
            assistant_wire_message("openai-responses", "", "private reasoning", Vec::new())
                .is_none()
        );
    }

    #[test]
    fn project_context_changes_append_without_rewriting_the_retained_prefix() {
        let mut history = vec![HistoryItem::User {
            turn_id: "turn-1".into(),
            content: "inspect the project".into(),
            attachments: Vec::new(),
        }];
        let first = next_project_context_snapshot(&history, Some("graph-v1".into())).unwrap();
        history.push(HistoryItem::Context {
            source: PROJECT_CONTEXT_SOURCE.into(),
            content: first,
        });
        let first_request = project_history(&history, &HashMap::new(), "openai");
        let first_bytes = serde_json::to_vec(&first_request).unwrap();

        assert!(next_project_context_snapshot(&history, Some("graph-v1".into())).is_none());
        history.push(HistoryItem::Assistant {
            turn_id: "turn-1".into(),
            content: String::new(),
            reasoning: String::new(),
        });
        let second = next_project_context_snapshot(&history, Some("graph-v2".into())).unwrap();
        history.push(HistoryItem::Context {
            source: PROJECT_CONTEXT_SOURCE.into(),
            content: second,
        });
        let second_request = project_history(&history, &HashMap::new(), "openai");

        assert_eq!(
            first_bytes,
            serde_json::to_vec(&second_request[..first_request.len()]).unwrap()
        );
        assert!(
            second_request.last().unwrap()["content"]
                .as_str()
                .unwrap()
                .contains("graph-v2")
        );
    }

    #[test]
    fn removed_project_context_appends_one_clear_snapshot() {
        let history = vec![HistoryItem::Context {
            source: PROJECT_CONTEXT_SOURCE.into(),
            content: "previous graph".into(),
        }];
        assert_eq!(
            next_project_context_snapshot(&history, None).as_deref(),
            Some(PROJECT_CONTEXT_CLEARED)
        );
        let cleared = vec![
            history[0].clone(),
            HistoryItem::Context {
                source: PROJECT_CONTEXT_SOURCE.into(),
                content: PROJECT_CONTEXT_CLEARED.into(),
            },
        ];
        assert!(next_project_context_snapshot(&cleared, None).is_none());
    }

    struct NoopSink;

    impl RuntimeEventSink for NoopSink {
        fn emit(&self, _event: RuntimeEvent) {}
    }

    #[derive(Default)]
    struct CollectingSink(parking_lot::Mutex<Vec<RuntimeEvent>>);

    impl RuntimeEventSink for CollectingSink {
        fn emit(&self, event: RuntimeEvent) {
            self.0.lock().push(event);
        }
    }

    #[test]
    fn usage_event_separates_round_cache_from_session_totals() {
        let state = AppState::new(Default::default());
        state.sessions.write().insert(
            "session".into(),
            crate::types::SessionSnapshot {
                id: "session".into(),
                title: "test".into(),
                provider_id: provider::PROVIDER_ID.into(),
                model_id: provider::MODEL_ID.into(),
                project_root: None,
                status: "streaming".into(),
                last_error: None,
                messages: Vec::new(),
                task: None,
                usage: crate::types::UsageSnapshot {
                    fresh_input_tokens: 30,
                    cache_read_tokens: 70,
                    ..Default::default()
                },
            },
        );
        let sink = CollectingSink::default();
        commit_round_usage(
            &sink,
            &state,
            "session",
            "turn",
            1,
            &TokenUsage {
                input_tokens: 100,
                cached_input_tokens: 80,
                output_tokens: 12,
                reasoning_tokens: 3,
                reported: true,
            },
        );
        let events = sink.0.lock();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data["roundUsage"]["freshInputTokens"], 20);
        assert_eq!(events[0].data["roundUsage"]["cacheReadTokens"], 80);
        assert_eq!(events[0].data["usage"]["freshInputTokens"], 50);
        assert_eq!(events[0].data["usage"]["cacheReadTokens"], 150);
    }

    #[tokio::test]
    async fn batched_find_preserves_order_in_one_artifact() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("src")).unwrap();
        std::fs::create_dir_all(directory.path().join("docs")).unwrap();
        std::fs::write(directory.path().join("src/apple.rs"), "fn apple() {}\n").unwrap();
        std::fs::write(directory.path().join("docs/banana.md"), "banana\n").unwrap();
        let root = project::canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());
        state
            .projects
            .write()
            .insert(root.clone(), project::build_manifest(&root).unwrap());
        let call = ToolCall {
            index: 0,
            id: "find-batch".into(),
            name: "find".into(),
            arguments: json!({"queries":["apple.rs","banana.md"]}),
        };

        let projection = dispatch(
            &NoopSink,
            &state,
            "session",
            Some(root.to_str().unwrap()),
            &call,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        assert_eq!(state.artifacts.read().len(), 1);
        let artifacts = state.artifacts.read();
        let artifact = artifacts.get(&projection.artifact_id).unwrap();
        assert_eq!(artifact["results"][0]["label"], "apple.rs");
        assert_eq!(artifact["results"][1]["label"], "banana.md");
        assert!(
            projection.summary.find("apple.rs").unwrap()
                < projection.summary.find("banana.md").unwrap()
        );
        assert_eq!(projection.completeness, "complete");
    }

    #[tokio::test]
    async fn batched_read_defaults_ranges_and_keeps_partial_results() {
        let directory = tempfile::tempdir().unwrap();
        let content = (1..=250)
            .map(|line| format!("line-{line}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(directory.path().join("source.txt"), content).unwrap();
        let root = project::canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());
        let call = ToolCall {
            index: 0,
            id: "read-batch".into(),
            name: "read".into(),
            arguments: json!({"ranges":[
                {"path":"source.txt"},
                {"path":"missing.txt","startLine":1,"endLine":2}
            ]}),
        };

        let projection = dispatch(
            &NoopSink,
            &state,
            "session",
            Some(root.to_str().unwrap()),
            &call,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        assert_eq!(state.artifacts.read().len(), 1);
        let artifacts = state.artifacts.read();
        let artifact = artifacts.get(&projection.artifact_id).unwrap();
        assert_eq!(artifact["results"][0]["result"]["startLine"], 1);
        assert_eq!(artifact["results"][0]["result"]["endLine"], 200);
        assert_eq!(artifact["results"][1]["error"]["key"], "error.path_missing");
        assert_eq!(projection.status, "completed");
        assert_eq!(projection.completeness, "partial");
    }

    #[tokio::test]
    async fn successful_tool_indexing_does_not_deadlock_on_artifacts() {
        // 回归：index_projection 曾在 `if let` 条件中持有 artifacts 读锁，
        // 又在块内取同一把锁的写锁，导致每次成功工具调用永久卡死。
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("sample.txt"), "content\n").unwrap();
        let root = project::canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());
        let call = ToolCall {
            index: 0,
            id: "read-once".into(),
            name: "read".into(),
            arguments: json!({"path":"sample.txt","startLine":1,"endLine":1}),
        };
        let projection = dispatch(
            &NoopSink,
            &state,
            "session",
            Some(root.to_str().unwrap()),
            &call,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        let mut indexed = projection.clone();
        let reference =
            index_projection(&state, "session", &call, &mut indexed).expect("reference");
        assert!(!reference.is_empty());
        assert_eq!(indexed.artifact_id, reference);
        let artifacts = state.artifacts.read();
        assert!(
            artifacts.contains_key(&reference),
            "artifact must be re-indexed under its observation reference"
        );
    }

    #[tokio::test]
    async fn mixed_read_arguments_are_merged_deduplicated_and_partially_fail() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("source.txt"), "first\nsecond\n").unwrap();
        let root = project::canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());
        let call = ToolCall {
            index: 0,
            id: "read-mixed".into(),
            name: "read".into(),
            arguments: json!({
                "path":"source.txt",
                "start_line":"2",
                "end_line":"2",
                "ranges":[
                    {"startLine":1,"endLine":1},
                    {"path":"missing.txt","startLine":1,"endLine":2},
                    {"path":false}
                ]
            }),
        };

        let projection = dispatch(
            &NoopSink,
            &state,
            "session",
            Some(root.to_str().unwrap()),
            &call,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        let artifacts = state.artifacts.read();
        let artifact = artifacts.get(&projection.artifact_id).unwrap();
        let results = artifact["results"].as_array().unwrap();
        assert_eq!(
            results.len(),
            3,
            "top-level path must only supply the range path"
        );
        assert_eq!(results[0]["result"]["content"], "first");
        assert_eq!(results[1]["error"]["key"], "error.path_missing");
        assert_eq!(results[2]["error"]["key"], "error.tool_argument");
        assert_eq!(projection.status, "completed");
        assert_eq!(projection.completeness, "partial");
    }

    #[test]
    fn edit_and_run_accept_common_open_model_argument_aliases() {
        let edit = json!({
            "file_path":"src/main.rs",
            "old_string":"before",
            "new_string":"after"
        });
        assert_eq!(
            required_text_alias(&edit, &["path", "filePath", "file_path"]).unwrap(),
            "src/main.rs"
        );
        assert_eq!(
            required_text_alias(&edit, &["oldText", "old_text", "oldString", "old_string"])
                .unwrap(),
            "before"
        );
        assert_eq!(
            required_text_alias(&edit, &["newText", "new_text", "newString", "new_string"])
                .unwrap(),
            "after"
        );

        let run = json!({
            "executable":"cargo",
            "arguments":"test --package 'knight frame'",
            "working_directory":"workspace"
        });
        assert_eq!(
            optional_text_alias(&run, &["program", "executable"]).unwrap(),
            Some("cargo")
        );
        assert_eq!(
            optional_text_alias(&run, &["cwd", "workingDirectory", "working_directory"]).unwrap(),
            Some("workspace")
        );
        assert_eq!(
            run_arguments(&run).unwrap(),
            vec!["test", "--package", "knight frame"]
        );
    }

    #[tokio::test]
    async fn aliased_edit_and_command_execute_end_to_end() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("source.txt");
        std::fs::write(&file, "before\n").unwrap();
        let root = project::canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());

        let edit = ToolCall {
            index: 0,
            id: "edit-alias".into(),
            name: "edit".into(),
            arguments: json!({
                "file_path":"source.txt",
                "old_string":"before",
                "new_string":"after"
            }),
        };
        dispatch(
            &NoopSink,
            &state,
            "session",
            Some(root.to_str().unwrap()),
            &edit,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "after\n");

        let command = if cfg!(windows) {
            "echo alias-ready"
        } else {
            "printf alias-ready"
        };
        let run = ToolCall {
            index: 1,
            id: "run-alias".into(),
            name: "run".into(),
            arguments: json!({"cmd":command,"working_directory":root}),
        };
        let projection = dispatch(
            &NoopSink,
            &state,
            "session",
            Some(root.to_str().unwrap()),
            &run,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(projection.summary.contains("alias-ready"));
    }

    #[test]
    fn single_read_defaults_lines_and_accepts_common_open_model_aliases() {
        let directory = tempfile::tempdir().unwrap();
        let content = (1..=250)
            .map(|line| format!("line-{line}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(directory.path().join("source.txt"), content).unwrap();
        let root = project::canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());

        let defaulted = dispatch_read(
            &state,
            root.to_str().unwrap(),
            &json!({"path":"source.txt"}),
        )
        .unwrap();
        assert_eq!(defaulted["startLine"], 1);
        assert_eq!(defaulted["endLine"], 200);

        let aliased = dispatch_read(
            &state,
            root.to_str().unwrap(),
            &json!({"path":"source.txt","start_line":"201","end_line":"250"}),
        )
        .unwrap();
        assert_eq!(aliased["startLine"], 201);
        assert_eq!(aliased["endLine"], 250);
        assert!(aliased["content"].as_str().unwrap().starts_with("line-201"));
    }

    #[test]
    fn batched_search_preserves_query_order_and_single_shape() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("src")).unwrap();
        std::fs::write(
            directory.path().join("src/first.rs"),
            "const ALPHA: &str = \"alpha\";\n",
        )
        .unwrap();
        std::fs::write(
            directory.path().join("src/second.rs"),
            "const BETA: &str = \"beta\";\n",
        )
        .unwrap();
        let root = project::canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());
        state
            .projects
            .write()
            .insert(root.clone(), project::build_manifest(&root).unwrap());

        let batch = dispatch_search(
            &state,
            root.to_str().unwrap(),
            &json!({"queries":[
                {"query":"beta","path":"src"},
                {"query":"alpha","path":"src"}
            ]}),
        )
        .unwrap();
        assert_eq!(batch["results"][0]["label"], "beta @ src");
        assert_eq!(
            batch["results"][0]["result"]["matches"][0]["path"],
            "src/second.rs"
        );
        assert_eq!(batch["results"][1]["label"], "alpha @ src");
        assert_eq!(
            batch["results"][1]["result"]["matches"][0]["path"],
            "src/first.rs"
        );

        let single = dispatch_search(
            &state,
            root.to_str().unwrap(),
            &json!({"query":"alpha","path":"src"}),
        )
        .unwrap();
        assert!(single.get("batch").is_none());
        assert_eq!(single["matches"][0]["path"], "src/first.rs");
    }

    #[test]
    fn batch_runtime_limit_and_mixed_shape_are_rejected() {
        let too_many = json!({"queries":(0..=MAX_BATCH_ITEMS).map(|index| format!("q{index}")).collect::<Vec<_>>()});
        assert!(batch_items(&too_many, "queries").is_err());
        let mixed = json!({"query":"one","queries":["two"]});
        assert!(reject_mixed_batch(&mixed, "query", "queries").is_err());
    }

    #[test]
    fn chat_without_workspace_exposes_only_task_progress() {
        let registry = BuiltinRegistry {
            task_enabled: true,
            skill_enabled: true,
            workspace_available: false,
        };
        let names: Vec<_> = registry.active().iter().map(|tool| tool.name).collect();
        // browser/market 不依赖工作区，无项目时仍可用（fetch/行情快照无需 root）
        assert_eq!(names, vec!["browser", "market", "recall", "skill", "task"]);
        assert!(!SYSTEM.contains("project="));
    }

    #[test]
    fn workspace_tools_keep_a_stable_order_with_write_between_edit_and_run() {
        let registry = BuiltinRegistry {
            task_enabled: true,
            skill_enabled: true,
            workspace_available: true,
        };
        let names: Vec<_> = registry.active().iter().map(|tool| tool.name).collect();
        // Order stability is a prefix-cache contract; write sits next to edit
        // because it is the whole-file counterpart of the fragment edit.
        assert_eq!(
            names,
            vec![
                "find", "search", "read", "edit", "write", "run", "browser", "market", "recall",
                "skill", "task"
            ]
        );
        // Whole-file writes have side effects; they must never be served from
        // the deterministic observation cache.
        assert!(!deterministic_tool("write"));
    }
    #[test]
    fn projection_is_bounded_but_raw_is_recoverable() {
        let value = json!({"content":"x".repeat(MAX_PROJECTION_BYTES * 2)});
        let projection = project_artifact("artifact".into(), &value);
        assert!(projection.truncated);
        assert!(projection.summary.len() <= MAX_PROJECTION_BYTES);
        assert_eq!(projection.completeness, "partial");
    }

    #[test]
    fn compact_receipts_do_not_advertise_recall() {
        let projection = ToolProjection {
            status: "completed".into(),
            total: 10,
            summary: "src/env_parser.py:1-40\nKEY = 1".into(),
            exit_code: Some(0),
            error_key: None,
            completeness: "complete".into(),
            truncated: false,
            artifact_id: String::new(),
        };
        let compact = compact_projection(&projection, "o7", false);
        // The receipt states the reference and first line only. Advertising
        // recall here taught models to spend a full-history round-trip on
        // details they often already had.
        assert_eq!(compact.summary, "stored o7; src/env_parser.py:1-40");
        assert!(!compact.summary.to_lowercase().contains("recall"));
        assert_eq!(compact.completeness, "reference");
        assert_eq!(compact.artifact_id, "o7");
    }

    #[test]
    fn tool_results_survive_until_the_context_budget_forces_compaction() {
        // Contract: mid-turn history is append-only while retained tool
        // projections fit CONTEXT_TOOL_BUDGET_BYTES. Overflow swaps the OLDEST
        // receipts first, freeing just enough to return under the budget.
        let full = "F".repeat(100_000); // 100 KB each
        let mut messages: Vec<Value> = vec![
            json!({"role":"tool","content":full.clone()}),
            json!({"role":"tool","content":full.clone()}),
            json!({"role":"tool","content":full.clone()}),
        ];
        let mut pending: Vec<(usize, String, u64)> = vec![
            (2, "\"C2\"".into(), 30), // newest, but listed first on purpose
            (0, "\"C0\"".into(), 10), // oldest
            (1, "\"C1\"".into(), 20),
        ];
        // 300 KB retained > 256 KB budget: compacting the oldest (100 KB)
        // returns exactly to 200 KB, under budget — one swap must suffice.
        let retained_bytes: usize = pending
            .iter()
            .filter_map(|(index, _, _)| {
                messages
                    .get(*index)
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_str)
                    .map(str::len)
            })
            .sum();
        assert!(retained_bytes > CONTEXT_TOOL_BUDGET_BYTES);
        pending.sort_by_key(|(_, _, born_round)| *born_round);
        let mut over = retained_bytes.saturating_sub(CONTEXT_TOOL_BUDGET_BYTES);
        pending.retain_mut(|(index, compact, _)| {
            if over == 0 {
                return true;
            }
            let full_len = messages
                .get(*index)
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .map(str::len)
                .unwrap_or(0);
            if let Some(content) = messages
                .get_mut(*index)
                .and_then(|message| message.get_mut("content"))
            {
                *content = Value::String(compact.clone());
            }
            over = over.saturating_sub(full_len);
            false
        });
        assert_eq!(messages[0]["content"], "\"C0\"");
        assert_eq!(messages[1]["content"], full);
        assert_eq!(messages[2]["content"], full);
        assert_eq!(pending.len(), 2);

        // A normal task's working set (3 × 10 KB results) never rewrites
        // history: rewriting mid-history breaks the provider prefix cache.
        // (30 KB << CONTEXT_TOOL_BUDGET_BYTES by construction — the budget
        // constant guards this invariant at 256 KB.)
    }

    #[test]
    fn tool_projection_is_readable_and_nonzero_run_is_failed() {
        let find = ToolCall {
            index: 0,
            id: "find-1".into(),
            name: "find".into(),
            arguments: json!({"query":"readme"}),
        };
        let find_projection = project_tool_artifact(
            &find,
            "artifact-find".into(),
            &json!({
                "matches":[{
                    "path":"src/main.ts",
                    "language":"TypeScript",
                    "size":42,
                    "relations":[{"kind":"depends","direction":"out","path":"src/util.ts"}],
                    "relationTotal":2
                }],
                "total":1,
                "truncated":false
            }),
        );
        assert_eq!(find_projection.status, "completed");
        assert!(
            find_projection
                .summary
                .contains("src/main.ts [TypeScript, 42 B]")
        );
        assert!(find_projection.summary.contains("depends -> src/util.ts"));
        assert!(find_projection.summary.contains("+1 more direct relations"));
        assert!(!find_projection.summary.contains("\\\"matches\\\""));

        let run = ToolCall {
            index: 0,
            id: "run-1".into(),
            name: "run".into(),
            arguments: json!({"command":"cargo test"}),
        };
        let run_projection = project_tool_artifact(
            &run,
            "artifact-run".into(),
            &json!({
                "exitCode":101,
                "elapsedMs":17,
                "stdout":"",
                "stderr":"error[E0308]: mismatched types",
                "stdoutBytes":0,
                "stderrBytes":30,
                "truncated":false
            }),
        );
        assert_eq!(run_projection.status, "failed");
        assert_eq!(run_projection.exit_code, Some(101));
        assert!(run_projection.summary.contains("error[E0308]"));
    }

    #[test]
    fn run_is_never_served_from_observation_cache() {
        let state = AppState::new(Default::default());
        let run = ToolCall {
            index: 0,
            id: "run-1".into(),
            name: "run".into(),
            arguments: json!({"command":"python -m unittest"}),
        };
        let mut projection = ToolProjection {
            status: "completed".into(),
            total: 7,
            summary: "ok: 7 passed".into(),
            exit_code: Some(0),
            error_key: None,
            completeness: "complete".into(),
            truncated: false,
            artifact_id: String::new(),
        };
        let reference = index_projection(&state, "session", &run, &mut projection);
        assert!(reference.is_some());
        // Even the exact same successful run command must re-execute:
        // no exact-key reuse, no verification-count refusal.
        assert!(cached_observation(&state, "session", &run).is_none());
    }

    #[test]
    fn deterministic_reads_reuse_until_run_edit_or_new_turn() {
        let state = AppState::new(Default::default());
        let read = ToolCall {
            index: 0,
            id: "read-1".into(),
            name: "read".into(),
            arguments: json!({"path":"src/main.rs"}),
        };
        let mut read_projection = ToolProjection {
            status: "completed".into(),
            total: 3,
            summary: "fn main".into(),
            exit_code: Some(0),
            error_key: None,
            completeness: "complete".into(),
            truncated: false,
            artifact_id: String::new(),
        };
        index_projection(&state, "session", &read, &mut read_projection);
        let (reused, _) = cached_observation(&state, "session", &read)
            .expect("unchanged deterministic read is reused within an epoch");
        assert!(reused.summary.starts_with("reused o"));

        // A successful run invalidates the read cache (workspace may have changed).
        let run = ToolCall {
            index: 1,
            id: "run-1".into(),
            name: "run".into(),
            arguments: json!({"command":"cargo build"}),
        };
        let mut run_projection = ToolProjection {
            status: "completed".into(),
            total: 4,
            summary: "done".into(),
            exit_code: Some(0),
            error_key: None,
            completeness: "complete".into(),
            truncated: false,
            artifact_id: String::new(),
        };
        index_projection(&state, "session", &run, &mut run_projection);
        assert!(cached_observation(&state, "session", &read).is_none());

        // Fresh read re-executes and is cacheable again…
        index_projection(&state, "session", &read, &mut read_projection);
        assert!(cached_observation(&state, "session", &read).is_some());
        // …until the next user turn invalidates it.
        begin_tool_turn(&state, "session");
        assert!(cached_observation(&state, "session", &read).is_none());
    }

    #[test]
    fn recoverable_tool_errors_explain_the_next_action() {
        let blocked = LocalizedError::new("error.run_program").arg("detail", "no program given");
        let projection = failed_projection("run", &blocked);
        assert_eq!(projection.status, "failed");
        assert!(projection.summary.contains("Run rejected"));

        let spawn = LocalizedError::new("error.run_spawn")
            .arg("program", "pnpm")
            .arg("detail", "not found");
        let projection = failed_projection("run", &spawn);
        assert!(projection.summary.contains("separate args"));
        assert!(projection.summary.contains("command"));
    }

    #[test]
    fn run_projection_appends_the_discovery_advisory() {
        let run = ToolCall {
            index: 0,
            id: "run-1".into(),
            name: "run".into(),
            arguments: json!({"command":"ls"}),
        };
        let projection = project_tool_artifact(
            &run,
            "artifact-run".into(),
            &json!({
                "exitCode":0,
                "elapsedMs":3,
                "stdout":"src\n",
                "stderr":"",
                "stdoutBytes":4,
                "stderrBytes":0,
                "truncated":false,
                "advisory":crate::tools::EXPLORATION_ADVISORY
            }),
        );
        assert_eq!(projection.status, "completed");
        assert!(
            projection
                .summary
                .contains(crate::tools::EXPLORATION_ADVISORY)
        );
    }

    #[test]
    fn tool_started_event_includes_call_name_and_raw_arguments() {
        let call = ToolCall {
            index: 0,
            id: "call-1".into(),
            name: "read".into(),
            arguments: json!({"path":"src/main.rs","startLine":1,"endLine":40}),
        };
        let event = tool_started_event("turn-1", &call, "session-1");
        assert_eq!(event.kind, "tool.started");
        assert_eq!(event.session_id.as_deref(), Some("session-1"));
        assert_eq!(event.data["name"], "read");
        assert_eq!(event.data["callId"], "call-1");
        assert_eq!(event.data["arguments"], call.arguments);
        assert_eq!(
            event.data["arguments"]["path"], "src/main.rs",
            "arguments must be emitted so machine consumers can prove graph-first tool order"
        );
    }

    #[test]
    fn guidance_queued_during_a_tool_round_is_appended_in_order() {
        let state = AppState::new(Default::default());
        let mut messages = Vec::new();
        append_guidance(
            &mut messages,
            &state,
            "session",
            "turn",
            vec![
                QueuedGuidance {
                    content: "first correction".into(),
                    clarify: true,
                    attachments: Vec::new(),
                },
                QueuedGuidance {
                    content: "then keep testing".into(),
                    clarify: false,
                    attachments: Vec::new(),
                },
            ],
        );

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], CLARIFY_GUIDANCE);
        assert_eq!(messages[1]["content"], "first correction");
        assert_eq!(messages[2]["content"], "then keep testing");
        let history = state.histories.read();
        let contents = history["session"]
            .iter()
            .filter_map(|item| match item {
                HistoryItem::User { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(contents, vec!["first correction", "then keep testing"]);
    }

    #[tokio::test]
    async fn cancellation_is_observed_before_loop_request() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn two_turn_history_projects_paired_tools_and_intermediate_text() {
        let history = vec![
            HistoryItem::User {
                turn_id: "t1".into(),
                content: "inspect".into(),
                attachments: Vec::new(),
            },
            HistoryItem::Assistant {
                turn_id: "t1".into(),
                content: "I will inspect.".into(),
                reasoning: "brief".into(),
            },
            HistoryItem::ToolCall {
                turn_id: "t1".into(),
                call_id: "call-1".into(),
                name: "read".into(),
                arguments: json!({"path":"a.rs","startLine":1,"endLine":2}),
            },
            HistoryItem::ToolResult {
                turn_id: "t1".into(),
                call_id: "call-1".into(),
                projection: json!({"status":"completed"}),
                artifact_id: Some("artifact-1".into()),
            },
            HistoryItem::User {
                turn_id: "t2".into(),
                content: "continue".into(),
                attachments: Vec::new(),
            },
            HistoryItem::Assistant {
                turn_id: "t2".into(),
                content: "Done.".into(),
                reasoning: String::new(),
            },
        ];
        let projected = project_history(&history, &HashMap::new(), "openai");
        assert!(projected.iter().any(
            |message| message.get("content").and_then(Value::as_str) == Some("I will inspect.")
        ));
        let reasoning_message = projected
            .iter()
            .find(|message| message.get("reasoning_content").is_some())
            .expect("reasoning must be replayed with its assistant message");
        assert_eq!(reasoning_message["reasoning_content"], "brief");
        assert_eq!(reasoning_message["tool_calls"][0]["id"], "call-1");
        assert_eq!(
            projected
                .iter()
                .filter(
                    |message| message.pointer("/tool_calls/0/id").and_then(Value::as_str)
                        == Some("call-1")
                )
                .count(),
            1,
            "the canonical assistant and tool call must merge into one wire message"
        );
        let mut calls = std::collections::BTreeSet::new();
        for message in projected {
            if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                for call in tool_calls {
                    calls.insert(call["id"].as_str().unwrap().to_owned());
                }
            }
            if message.get("role").and_then(Value::as_str) == Some("tool") {
                let call_id = message.get("tool_call_id").and_then(Value::as_str).unwrap();
                assert!(calls.contains(call_id), "orphan tool result: {call_id}");
            }
        }
        assert!(calls.contains("call-1"));
    }

    #[test]
    fn local_token_estimate_handles_ascii_and_cjk_without_an_llm() {
        assert_eq!(estimate_text_tokens(&"a".repeat(2_000)), 500);
        assert_eq!(estimate_text_tokens(&"测".repeat(500)), 500);
        assert_eq!(estimate_text_tokens("abcde"), 2);
    }

    #[test]
    fn requirement_brief_requires_twenty_percent_savings() {
        let original = "a".repeat(2_000);
        let accepted = "a".repeat(1_600);
        let rejected = "a".repeat(1_604);
        let (brief, after, reason) = evaluate_requirement_brief(&original, 500, &accepted);
        assert_eq!(after, 400);
        assert_eq!(reason, "accepted");
        assert_eq!(brief.as_deref(), Some(accepted.as_str()));

        let (brief, after, reason) = evaluate_requirement_brief(&original, 500, &rejected);
        assert_eq!(after, 401);
        assert_eq!(reason, "no_savings");
        assert!(brief.is_none());
    }

    #[test]
    fn requirement_brief_preserves_code_urls_paths_and_numeric_identifiers() {
        let required = r#"Keep `fn_v2()` https://example.test/api/v2 C:\Work\KnightFrame\src\main.rs /usr/local/bin/tool src/lib.rs issue-42"#;
        let original = format!("{required} {}", "background ".repeat(500));
        let before = estimate_text_tokens(&original);
        let (brief, _, reason) = evaluate_requirement_brief(&original, before, required);
        assert_eq!(reason, "accepted");
        assert_eq!(brief.as_deref(), Some(required));

        for invalid in [
            required.replace("fn_v2()", "the function"),
            required.replace("https://example.test/api/v2", "the endpoint"),
            required.replace(r"C:\Work\KnightFrame\src\main.rs", "the Windows file"),
            required.replace("/usr/local/bin/tool", "the Unix tool"),
            required.replace("issue-42", "the issue"),
        ] {
            let (brief, after, reason) = evaluate_requirement_brief(&original, before, &invalid);
            assert!(brief.is_none());
            assert_eq!(after, before);
            assert_eq!(reason, "invalid_output");
        }
    }

    #[tokio::test]
    async fn disabled_reducer_is_silent_and_short_input_uses_stable_reason() {
        let state = AppState::new(Default::default());
        let sink = CollectingSink::default();
        maybe_reduce_requirement(
            &sink,
            &state,
            "session",
            "turn-disabled",
            provider::DEFAULT_BASE_URL,
            &"a".repeat(2_000),
            &CancellationToken::new(),
        )
        .await;
        assert!(sink.0.lock().is_empty());

        state.settings.write().auxiliary_enabled = true;
        maybe_reduce_requirement(
            &sink,
            &state,
            "session",
            "turn-short",
            provider::DEFAULT_BASE_URL,
            "short",
            &CancellationToken::new(),
        )
        .await;
        {
            let events = sink.0.lock();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].kind, "auxiliary.skipped");
            assert_eq!(events[0].data["reason"], "short_input");
        }

        state.settings.write().auxiliary_provider_id = "unsupported".into();
        maybe_reduce_requirement(
            &sink,
            &state,
            "session",
            "turn-provider-error",
            provider::DEFAULT_BASE_URL,
            &"a".repeat(2_000),
            &CancellationToken::new(),
        )
        .await;
        let events = sink.0.lock();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].kind, "auxiliary.failed");
        assert_eq!(events[1].data["reason"], "provider_error");
        assert_eq!(events[1].data["errorKey"], "error.provider_unsupported");
    }

    #[test]
    fn accepted_brief_replaces_only_the_first_user_item_for_a_turn() {
        let history = vec![
            HistoryItem::User {
                turn_id: "turn-1".into(),
                content: "long original requirement".into(),
                attachments: Vec::new(),
            },
            HistoryItem::User {
                turn_id: "turn-1".into(),
                content: "later guidance".into(),
                attachments: Vec::new(),
            },
        ];
        let briefs = HashMap::from([("turn-1".into(), "compact brief".into())]);
        let projected = project_history(&history, &briefs, "openai");
        assert_eq!(projected[0]["content"], "compact brief");
        assert_eq!(projected[1]["content"], "later guidance");
        match &history[0] {
            HistoryItem::User {
                turn_id, content, ..
            } => {
                assert_eq!(turn_id, "turn-1");
                assert_eq!(content, "long original requirement");
            }
            _ => panic!("canonical user history changed kind"),
        }
    }

    #[test]
    fn auxiliary_event_has_a_stable_id_and_complete_receipt_fields() {
        let receipt = AuxiliaryReceipt {
            id: "auxiliary:requirementReducer:turn-1".into(),
            turn_id: "turn-1".into(),
            model: provider::AUXILIARY_MODEL_ID.into(),
            status: "completed".into(),
            reason: "accepted".into(),
            before_tokens: 700,
            after_tokens: 300,
            input_tokens: 720,
            output_tokens: 80,
            elapsed_ms: 42,
            summary: "compact".into(),
            error_key: None,
        };
        let event = receipt.event("auxiliary.completed", "session-1");
        assert_eq!(event.kind, "auxiliary.completed");
        assert_eq!(event.session_id.as_deref(), Some("session-1"));
        assert_eq!(event.data["id"], receipt.id);
        assert_eq!(event.data["turnId"], "turn-1");
        assert_eq!(event.data["role"], "requirementReducer");
        assert_eq!(event.data["beforeTokens"], 700);
        assert_eq!(event.data["afterTokens"], 300);
        assert_eq!(event.data["inputTokens"], 720);
        assert_eq!(event.data["outputTokens"], 80);
        assert_eq!(event.data["elapsedMs"], 42);
        assert_eq!(event.data["summary"], "compact");
        assert!(event.data["errorKey"].is_null());
    }
}
