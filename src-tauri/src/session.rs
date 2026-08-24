use crate::{
    agent_loop,
    error::{KfResult, LocalizedError},
    provider,
    state::AppState,
    task,
    types::{
        Ack, MessageAttachment, MessageSnapshot, RuntimeEvent, SessionSnapshot, SettingsSnapshot,
        TurnReceipt, UsageSnapshot,
    },
};
use serde_json::json;
use std::{
    collections::VecDeque,
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn session_title(content: &str) -> String {
    let title: String = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(36)
        .collect();
    if title.is_empty() {
        "session.new".into()
    } else {
        title
    }
}

fn validate_attachments(attachments: &[MessageAttachment]) -> KfResult<()> {
    if attachments.len() > 8 {
        return Err(LocalizedError::new("error.attachments_count"));
    }
    let mut total = 0_u64;
    for attachment in attachments {
        let supported = matches!(
            attachment.mime_type.as_str(),
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        );
        if !supported
            || !attachment
                .data_url
                .starts_with(&format!("data:{};base64,", attachment.mime_type))
            || attachment.size > 20 * 1024 * 1024
        {
            return Err(
                LocalizedError::new("error.attachment_invalid").arg("name", &attachment.name)
            );
        }
        total = total.saturating_add(attachment.size);
    }
    if total > 32 * 1024 * 1024 {
        return Err(LocalizedError::new("error.attachments_size"));
    }
    Ok(())
}

fn resolve_model_selection(
    settings: &SettingsSnapshot,
    provider_id: Option<String>,
    model_id: Option<String>,
) -> (String, String) {
    (
        provider_id.unwrap_or_else(|| settings.provider_id.clone()),
        model_id.unwrap_or_else(|| settings.model_id.clone()),
    )
}

#[tauri::command]
pub fn kf_session_create(
    state: tauri::State<'_, Arc<AppState>>,
    project_root: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> KfResult<SessionSnapshot> {
    create_session(&state, project_root, provider, model)
}

pub(crate) fn create_session(
    state: &AppState,
    project_root: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> KfResult<SessionSnapshot> {
    let settings = state.settings.read().clone();
    let (provider_id, model_id) = resolve_model_selection(&settings, provider, model);
    provider::validate_selection(state, &provider_id, &model_id)?;
    let id = Uuid::new_v4().to_string();
    let task = settings
        .task_manager
        .then(|| task::new_task(format!("task-{id}"), "task.session".into()));
    if let Some(task) = &task {
        state.tasks.write().insert(id.clone(), task.clone());
    }
    let session = SessionSnapshot {
        id: id.clone(),
        title: "session.new".into(),
        provider_id,
        model_id,
        project_root,
        status: "idle".into(),
        last_error: None,
        messages: vec![],
        task,
        usage: UsageSnapshot {
            fresh_input_tokens: 0,
            cache_read_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            request_count: 0,
            current_context_tokens: None,
        },
    };
    state.sessions.write().insert(id, session.clone());
    crate::persistence::save(state)?;
    Ok(session)
}

#[tauri::command]
pub fn kf_session_rename(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
    title: String,
) -> KfResult<SessionSnapshot> {
    let title: String = title.trim().chars().take(64).collect();
    if title.is_empty() {
        return Err(LocalizedError::new("error.session_title_empty"));
    }
    let snapshot = {
        let mut sessions = state.sessions.write();
        let session = sessions.get_mut(&session_id).ok_or_else(|| {
            LocalizedError::new("error.session_not_found").arg("sessionId", &session_id)
        })?;
        session.title = title;
        session.clone()
    };
    crate::persistence::save(&state)?;
    let _ = app.emit(
        "kf://runtime",
        RuntimeEvent::new("session.renamed", json!({"session": snapshot})).session(&session_id),
    );
    Ok(snapshot)
}

#[tauri::command]
pub fn kf_session_delete(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
) -> KfResult<Ack> {
    if let Some(active) = state.active_turns.write().remove(&session_id) {
        active.cancellation.cancel();
    }
    let removed = state.sessions.write().remove(&session_id).is_some();
    state.histories.write().remove(&session_id);
    state.tasks.write().remove(&session_id);
    state.tool_observations.write().remove(&session_id);
    if !removed {
        return Err(LocalizedError::new("error.session_not_found").arg("sessionId", &session_id));
    }
    crate::persistence::save(&state)?;
    let _ = app.emit(
        "kf://runtime",
        RuntimeEvent::new("session.deleted", json!({"sessionId": session_id})).session(&session_id),
    );
    Ok(Ack { ok: true })
}

#[tauri::command]
pub async fn kf_session_send(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
    content: String,
    clarify: Option<bool>,
    attachments: Option<Vec<MessageAttachment>>,
) -> KfResult<TurnReceipt> {
    let attachments = attachments.unwrap_or_default();
    if content.trim().is_empty() && attachments.is_empty() {
        return Err(LocalizedError::new("error.session_empty_message"));
    }
    validate_attachments(&attachments)?;
    let clarify = clarify.unwrap_or(false);
    if let Some(active) = state.active_turns.read().get(&session_id).cloned() {
        if !active.guide(crate::state::QueuedGuidance {
            content: content.clone(),
            clarify,
            attachments: attachments.clone(),
        }) {
            return Err(LocalizedError::new("error.session_busy"));
        }
        let guidance_id = Uuid::new_v4().to_string();
        if let Some(session) = state.sessions.write().get_mut(&session_id) {
            session.messages.push(MessageSnapshot {
                id: guidance_id,
                role: "user".into(),
                content,
                created_at: timestamp(),
                attachments,
            });
        }
        crate::persistence::save(&state)?;
        return Ok(TurnReceipt {
            turn_id: active.turn_id,
        });
    }
    let turn_id = Uuid::new_v4().to_string();
    let turn_task = state.settings.read().task_manager.then(|| {
        task::new_task(
            format!("task-{session_id}-{turn_id}"),
            "task.session".into(),
        )
    });
    let session_snapshot = {
        let mut sessions = state.sessions.write();
        let session = sessions.get_mut(&session_id).ok_or_else(|| {
            LocalizedError::new("error.session_not_found").arg("sessionId", &session_id)
        })?;
        if session.status == "streaming" {
            return Err(LocalizedError::new("error.session_busy"));
        }
        session.status = "streaming".into();
        session.last_error = None;
        session.task = turn_task.clone();
        if session.messages.is_empty() {
            session.title = session_title(&content);
        }
        session.messages.push(MessageSnapshot {
            id: turn_id.clone(),
            role: "user".into(),
            content: content.clone(),
            created_at: timestamp(),
            attachments: attachments.clone(),
        });
        session.clone()
    };
    if let Some(task) = &turn_task {
        state.tasks.write().insert(session_id.clone(), task.clone());
    } else {
        state.tasks.write().remove(&session_id);
    }
    let fallback_title = session_snapshot.title.clone();
    let should_name = session_snapshot.messages.len() == 1;
    let cancellation = CancellationToken::new();
    state.active_turns.write().insert(
        session_id.clone(),
        crate::state::ActiveTurn {
            turn_id: turn_id.clone(),
            cancellation: cancellation.clone(),
            guidance: Arc::new(parking_lot::Mutex::new(VecDeque::new())),
            accepting_guidance: Arc::new(AtomicBool::new(true)),
            guidance_signal: Arc::new(tokio::sync::Notify::new()),
        },
    );
    let guidance = state
        .active_turns
        .read()
        .get(&session_id)
        .expect("active turn was just inserted")
        .clone();
    state
        .histories
        .write()
        .entry(session_id.clone())
        .or_default()
        .push(crate::types::HistoryItem::User {
            turn_id: turn_id.clone(),
            content: content.clone(),
            attachments,
        });
    crate::persistence::save(&state)?;
    let _ = app.emit(
        "kf://runtime",
        RuntimeEvent::new(
            "session.started",
            json!({"turnId": turn_id, "session": session_snapshot}),
        )
        .session(&session_id),
    );
    let app_for_stream = app.clone();
    let state_for_stream = state.inner().clone();
    let session_for_stream = session_id.clone();
    let turn_for_stream = turn_id.clone();
    let content_for_naming = content.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = agent_loop::run(
            app_for_stream.clone(),
            state_for_stream.clone(),
            session_for_stream.clone(),
            turn_for_stream.clone(),
            content,
            clarify,
            guidance,
        )
        .await;
        let owns_generation = state_for_stream
            .active_turns
            .read()
            .get(&session_for_stream)
            .is_some_and(|active| active.turn_id == turn_for_stream);
        if owns_generation {
            state_for_stream
                .active_turns
                .write()
                .remove(&session_for_stream);
        }
        if !owns_generation {
            return;
        }
        if let Some(session) = state_for_stream
            .sessions
            .write()
            .get_mut(&session_for_stream)
        {
            session.status = match &outcome {
                Ok(_) => "idle",
                Err(error) if error.key == "error.session_cancelled" => "idle",
                Err(_) => "failed",
            }
            .into();
            session.last_error = outcome.as_ref().err().cloned();
            if let Ok((_usage, final_text)) = &outcome {
                session.messages.push(MessageSnapshot {
                    id: format!("assistant-{turn_for_stream}"),
                    role: "assistant".into(),
                    content: final_text.clone(),
                    created_at: timestamp(),
                    attachments: Vec::new(),
                });
            }
        }
        // 回合收尾：任务必须随回合落地（completed/failed），否则前端
        // 任务胶囊永远停在"处理当前请求"。默认占位任务直接清掉，
        // agent 建立的真实任务标记终态后通知前端。
        {
            let final_status = match &outcome {
                Ok(_) => "completed",
                Err(error) if error.key == "error.session_cancelled" => "cancelled",
                Err(_) => "failed",
            };
            let (task_snapshot, placeholder_removed) = {
                let mut tasks = state_for_stream.tasks.write();
                let is_placeholder = tasks.get(&session_for_stream).is_some_and(|task| {
                    task.items.len() == 1 && task.items[0].title == "task.session"
                });
                if is_placeholder {
                    tasks.remove(&session_for_stream);
                    (None, true)
                } else {
                    let snapshot = tasks.get_mut(&session_for_stream).map(|task| {
                        crate::task::settle_after_turn(task, final_status);
                        task.clone()
                    });
                    (snapshot, false)
                }
            };
            if placeholder_removed
                && let Some(session) = state_for_stream
                    .sessions
                    .write()
                    .get_mut(&session_for_stream)
            {
                session.task = None;
            }
            if let Some(snapshot) = task_snapshot {
                if let Some(session) = state_for_stream
                    .sessions
                    .write()
                    .get_mut(&session_for_stream)
                {
                    session.task = Some(snapshot.clone());
                }
                let _ = app_for_stream.emit(
                    "kf://runtime",
                    RuntimeEvent::new("task.updated", json!(snapshot)).session(&session_for_stream),
                );
            }
        }
        let _ = crate::persistence::save(&state_for_stream);
        if let Err(error) = outcome {
            let kind = if error.key == "error.session_cancelled" {
                "assistant.cancelled"
            } else {
                "assistant.failed"
            };
            let _ = app_for_stream.emit(
                "kf://runtime",
                RuntimeEvent::new(kind, json!({"turnId": turn_for_stream, "error": error}))
                    .session(&session_for_stream),
            );
        } else {
            let _ = app_for_stream.emit(
                "kf://runtime",
                RuntimeEvent::new("assistant.completed", json!({"turnId": turn_for_stream}))
                    .session(&session_for_stream),
            );
            if should_name {
                let naming_app = app_for_stream.clone();
                let naming_state = state_for_stream.clone();
                let naming_session = session_for_stream.clone();
                let naming_fallback = fallback_title.clone();
                let naming_request = content_for_naming.clone();
                tauri::async_runtime::spawn(async move {
                    let (naming_provider, naming_model) = naming_state
                        .sessions
                        .read()
                        .get(&naming_session)
                        .map(|s| (s.provider_id.clone(), s.model_id.clone()))
                        .unwrap_or_else(|| {
                            (provider::PROVIDER_ID.into(), provider::MODEL_ID.into())
                        });
                    let profile = naming_state
                        .settings
                        .read()
                        .providers
                        .iter()
                        .find(|profile| profile.id == naming_provider)
                        .cloned();
                    let result = if let Some(profile) = profile.as_ref() {
                        provider::summarize_title_profile(
                            &naming_state.client,
                            profile,
                            &naming_model,
                            &naming_request,
                        )
                        .await
                    } else {
                        provider::summarize_title(
                            &naming_state.client,
                            &naming_model,
                            &naming_request,
                        )
                        .await
                    };
                    let Ok((title, usage)) = result else {
                        return;
                    };
                    let mut renamed = None;
                    if let Some(session) = naming_state.sessions.write().get_mut(&naming_session)
                        && session.title == naming_fallback
                    {
                        session.title = title;
                        session.usage.request_count += 1;
                        session.usage.fresh_input_tokens += usage.fresh_input_tokens();
                        session.usage.cache_read_tokens += usage.cached_input_tokens;
                        session.usage.output_tokens += usage.output_tokens;
                        session.usage.reasoning_tokens += usage.reasoning_tokens;
                        renamed = Some(session.clone());
                    }
                    if let Some(session) = renamed {
                        let _ = crate::persistence::save(&naming_state);
                        let _ = naming_app.emit(
                            "kf://runtime",
                            RuntimeEvent::new("session.renamed", json!({"session":session}))
                                .session(naming_session),
                        );
                    }
                });
            }
        }
    });
    Ok(TurnReceipt { turn_id })
}

#[tauri::command]
pub fn kf_session_stop(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
) -> KfResult<Ack> {
    let active = state
        .active_turns
        .read()
        .get(&session_id)
        .cloned()
        .ok_or_else(|| LocalizedError::new("error.session_not_streaming"))?;
    active.accepting_guidance.store(false, Ordering::Release);
    active.cancellation.cancel();
    if let Some(task) = state.tasks.write().get_mut(&session_id) {
        for item in &mut task.items {
            if item.status == "running" || item.status == "pending" {
                item.status = "cancelled".into();
            }
        }
        task.status = "cancelled".into();
        task.current = None;
        let snapshot = task.clone();
        if let Some(session) = state.sessions.write().get_mut(&session_id) {
            session.task = Some(snapshot.clone());
        }
        let _ = app.emit(
            "kf://runtime",
            RuntimeEvent::new("task.updated", json!(snapshot)).session(&session_id),
        );
    }
    crate::persistence::save(&state)?;
    Ok(Ack { ok: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_resolution_uses_the_persisted_selection() {
        let settings = SettingsSnapshot {
            provider_id: provider::PROVIDER_ID.into(),
            model_id: "future-code-9-free".into(),
            ..Default::default()
        };

        let selection = resolve_model_selection(&settings, None, None);

        assert_eq!(selection.0, provider::PROVIDER_ID);
        assert_eq!(selection.1, "future-code-9-free");
    }

    #[test]
    fn explicit_session_selection_overrides_persisted_values() {
        let settings = SettingsSnapshot {
            provider_id: provider::PROVIDER_ID.into(),
            model_id: "persisted-free".into(),
            ..Default::default()
        };

        let selection = resolve_model_selection(
            &settings,
            Some(provider::PROVIDER_ID.into()),
            Some("explicit-free".into()),
        );

        assert_eq!(selection.1, "explicit-free");
    }

    #[test]
    fn image_attachments_are_bounded_and_mime_checked() {
        let valid = MessageAttachment {
            id: "1".into(),
            name: "screen.png".into(),
            mime_type: "image/png".into(),
            data_url: "data:image/png;base64,AA==".into(),
            size: 2,
        };
        assert!(validate_attachments(std::slice::from_ref(&valid)).is_ok());
        let invalid = MessageAttachment {
            mime_type: "text/plain".into(),
            data_url: "data:text/plain;base64,AA==".into(),
            ..valid
        };
        assert_eq!(
            validate_attachments(&[invalid]).unwrap_err().key,
            "error.attachment_invalid"
        );
    }
}
