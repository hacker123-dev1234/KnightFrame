use crate::{
    error::{KfResult, LocalizedError},
    state::AppState,
    types::{RuntimeEvent, TaskItem, TaskSnapshot},
};
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

const STATUSES: &[&str] = &[
    "pending",
    "running",
    "completed",
    "failed",
    "blocked",
    "cancelled",
];

pub fn new_task(id: String, title: String) -> TaskSnapshot {
    TaskSnapshot {
        id: id.clone(),
        status: "pending".into(),
        completed: 0,
        total: 1,
        current: Some(title.clone()),
        items: vec![TaskItem {
            id: format!("{id}:1"),
            title,
            detail: None,
            status: "pending".into(),
        }],
    }
}

/// Settle only work that actually started when a turn fails or is cancelled.
/// Pending plan items stay resumable instead of being falsely painted failed.
pub fn settle_after_turn(task: &mut TaskSnapshot, outcome: &str) {
    for item in &mut task.items {
        match outcome {
            "completed" if item.status == "running" || item.status == "pending" => {
                item.status = "completed".into();
            }
            "failed" | "cancelled" if item.status == "running" => {
                item.status = outcome.into();
            }
            _ => {}
        }
    }
    recalculate(task);
}

fn recalculate(task: &mut TaskSnapshot) {
    task.total = task.items.len();
    task.completed = task
        .items
        .iter()
        .filter(|item| item.status == "completed")
        .count();
    task.current = task
        .items
        .iter()
        .find(|item| item.status == "running")
        .or_else(|| task.items.iter().find(|item| item.status == "pending"))
        .map(|item| item.title.clone());
    task.status = if task.items.iter().any(|item| item.status == "running") {
        "running"
    } else if task.total > 0 && task.completed == task.total {
        "completed"
    } else if task.items.iter().any(|item| item.status == "failed") {
        "failed"
    } else if task.items.iter().any(|item| item.status == "blocked") {
        "blocked"
    } else if task.total > 0 && task.items.iter().all(|item| item.status == "cancelled") {
        "cancelled"
    } else {
        "pending"
    }
    .into();
}

pub fn apply(task: &mut TaskSnapshot, op: &str, item: Option<&str>) -> KfResult<()> {
    match op {
        "add" => {
            let title = item
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| LocalizedError::new("error.task_item_required"))?;
            if task.items.len() == 1 && task.items[0].title == "task.session" {
                task.items[0].title = title.trim().into();
                task.items[0].status = "pending".into();
                recalculate(task);
                return Ok(());
            }
            let index = task.items.len() + 1;
            task.items.push(TaskItem {
                id: format!("{}:{index}", task.id),
                title: title.trim().into(),
                detail: None,
                status: "pending".into(),
            });
        }
        status if STATUSES.contains(&status) => {
            let selector = item.ok_or_else(|| LocalizedError::new("error.task_item_required"))?;
            let selected = task
                .items
                .iter_mut()
                .find(|candidate| candidate.id == selector || candidate.title == selector)
                .ok_or_else(|| LocalizedError::new("error.task_not_found").arg("item", selector))?;
            selected.status = status.into();
        }
        _ => return Err(LocalizedError::new("error.task_operation").arg("op", op)),
    }
    recalculate(task);
    Ok(())
}

#[tauri::command]
pub fn kf_task_command(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
    op: String,
    item: Option<String>,
) -> KfResult<TaskSnapshot> {
    let mut tasks = state.tasks.write();
    let task = tasks
        .get_mut(&session_id)
        .ok_or_else(|| LocalizedError::new("error.task_not_found").arg("sessionId", &session_id))?;
    apply(task, &op, item.as_deref())?;
    let snapshot = task.clone();
    let event = RuntimeEvent::new("task.updated", json!(snapshot)).session(session_id);
    let _ = app.emit("kf://runtime", event);
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_state_is_derived_from_items() {
        let mut task = new_task("task".into(), "index".into());
        let id = task.items[0].id.clone();
        apply(&mut task, "running", Some(&id)).unwrap();
        assert_eq!(task.status, "running");
        apply(&mut task, "completed", Some(&id)).unwrap();
        assert_eq!(task.status, "completed");
        assert_eq!(task.completed, 1);
    }

    #[test]
    fn task_cancelled_is_not_reported_as_pending() {
        let mut task = new_task("task".into(), "index".into());
        let id = task.items[0].id.clone();
        apply(&mut task, "cancelled", Some(&id)).unwrap();
        assert_eq!(task.status, "cancelled");
    }

    #[test]
    fn failed_turn_preserves_unstarted_plan_items() {
        let mut task = new_task("task".into(), "current".into());
        apply(&mut task, "add", Some("later")).unwrap();
        let current = task.items[0].id.clone();
        apply(&mut task, "running", Some(&current)).unwrap();
        settle_after_turn(&mut task, "failed");
        assert_eq!(task.items[0].status, "failed");
        assert_eq!(task.items[1].status, "pending");
        assert_eq!(task.current.as_deref(), Some("later"));
    }

    #[test]
    fn cancelled_turn_leaves_pending_work_resumable() {
        let mut task = new_task("task".into(), "current".into());
        apply(&mut task, "add", Some("later")).unwrap();
        let current = task.items[0].id.clone();
        apply(&mut task, "running", Some(&current)).unwrap();
        settle_after_turn(&mut task, "cancelled");
        assert_eq!(task.items[0].status, "cancelled");
        assert_eq!(task.items[1].status, "pending");
    }

    #[test]
    fn first_real_item_replaces_placeholder() {
        let mut task = new_task("task".into(), "task.session".into());
        apply(&mut task, "add", Some("inspect project")).unwrap();
        assert_eq!(task.items.len(), 1);
        assert_eq!(task.items[0].title, "inspect project");
    }
}
