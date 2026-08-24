use crate::{
    error::{KfResult, LocalizedError},
    provider,
    state::AppState,
    types::{SessionSnapshot, SettingsPatch, SettingsSnapshot},
};
use std::{collections::HashSet, fs, sync::Arc};
use tauri::{AppHandle, Manager};

fn settings_path(app: &AppHandle) -> KfResult<std::path::PathBuf> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|e| LocalizedError::new("error.settings_path").arg("detail", e))?;
    fs::create_dir_all(&directory)?;
    Ok(directory.join("settings.json"))
}

pub fn load(app: &AppHandle) -> SettingsSnapshot {
    let mut settings: SettingsSnapshot = settings_path(app)
        .ok()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut migrated = false;
    for profile in &mut settings.providers {
        if !profile.api_key.is_empty() {
            if let Ok(reference) = crate::credentials::write(&profile.id, &profile.api_key) {
                profile.credential_ref = reference;
            }
            profile.api_key.clear();
            migrated = true;
        }
    }
    if migrated {
        let _ = persist(app, &settings);
    }
    settings
}

pub(crate) fn persist(app: &AppHandle, settings: &SettingsSnapshot) -> KfResult<()> {
    let path = settings_path(app)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(settings)
            .map_err(|e| LocalizedError::new("error.settings_encode").arg("detail", e))?,
    )?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn apply_session_model(
    session: &mut SessionSnapshot,
    provider_id: String,
    model_id: String,
    available_models: &HashSet<String>,
) -> KfResult<()> {
    if session.status == "streaming" {
        return Err(LocalizedError::new("error.session_busy"));
    }
    provider::validate_available_model(&provider_id, &model_id, available_models)?;
    session.provider_id = provider_id;
    session.model_id = model_id;
    Ok(())
}

fn validate_auxiliary_selection(
    settings: &SettingsSnapshot,
    available_models: &HashSet<String>,
) -> KfResult<()> {
    provider::validate_available_model(
        &settings.auxiliary_provider_id,
        &settings.auxiliary_model_id,
        available_models,
    )
}

#[tauri::command]
pub fn kf_settings_update(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    patch: SettingsPatch,
) -> KfResult<SettingsSnapshot> {
    let selection_changed = patch.provider_id.is_some() || patch.model_id.is_some();
    let providers_changed = patch.providers.is_some();
    let auxiliary_selection_changed =
        patch.auxiliary_provider_id.is_some() || patch.auxiliary_model_id.is_some();
    let auxiliary_enabling = patch.auxiliary_enabled == Some(true);
    let subagent_selection_changed = patch.subagent_execution_provider_id.is_some()
        || patch.subagent_execution_model_id.is_some();
    let subagent_enabling = patch.subagent_enabled == Some(true);
    let mut next = state.settings.read().clone();
    let previous_credentials = next
        .providers
        .iter()
        .filter_map(|profile| {
            (!profile.credential_ref.is_empty()).then_some(profile.credential_ref.clone())
        })
        .collect::<HashSet<_>>();
    if let Some(locale) = patch.locale {
        if locale != "zh-CN" && locale != "en-US" {
            return Err(LocalizedError::new("error.settings_locale"));
        }
        next.locale = locale;
    }
    if let Some(value) = patch.task_manager {
        next.task_manager = value;
    }
    if let Some(value) = patch.usage_panel {
        next.usage_panel = value;
    }
    if let Some(value) = patch.caveman_mode {
        if value != "lite" && value != "off" {
            return Err(LocalizedError::new("error.settings_caveman"));
        }
        next.caveman_mode = value;
    }
    if let Some(value) = patch.user_avatar {
        if value.is_empty() {
            next.user_avatar = None;
        } else {
            let supported = value.starts_with("data:image/png;base64,")
                || value.starts_with("data:image/jpeg;base64,")
                || value.starts_with("data:image/webp;base64,");
            if !supported || value.len() > 512_000 {
                return Err(LocalizedError::new("error.settings_avatar"));
            }
            next.user_avatar = Some(value);
        }
    }
    if let Some(value) = patch.provider_id {
        next.provider_id = value;
    }
    if let Some(value) = patch.model_id {
        next.model_id = value;
    }
    if let Some(value) = patch.providers {
        let mut value = value;
        for profile in &mut value {
            if !profile.api_key.is_empty() {
                profile.credential_ref = crate::credentials::write(&profile.id, &profile.api_key)?;
                profile.api_key.clear();
            }
        }
        provider::validate_profiles(&value)?;
        next.providers = value;
    }
    if let Some(value) = patch.auxiliary_enabled {
        next.auxiliary_enabled = value;
    }
    if let Some(value) = patch.auxiliary_provider_id {
        next.auxiliary_provider_id = value;
    }
    if let Some(value) = patch.auxiliary_model_id {
        next.auxiliary_model_id = value;
    }
    if let Some(value) = patch.subagent_enabled {
        next.subagent_enabled = value;
    }
    if let Some(value) = patch.subagent_execution_provider_id {
        next.subagent_execution_provider_id = value;
    }
    if let Some(value) = patch.subagent_execution_model_id {
        next.subagent_execution_model_id = value;
    }
    if let Some(value) = patch.subagent_execution_effort {
        if ![
            "lowest", "none", "minimal", "low", "medium", "high", "xhigh", "max",
        ]
        .contains(&value.as_str())
        {
            return Err(LocalizedError::new("error.settings_thinking_effort"));
        }
        next.subagent_execution_effort = value;
    }
    if let Some(value) = patch.skill_router {
        next.skill_router = value;
    }
    if let Some(value) = patch.skill_opt {
        next.skill_opt = value;
    }
    if let Some(value) = patch.memory_enabled {
        next.memory_enabled = value;
    }
    if let Some(value) = patch.ui_scale {
        // 允许范围 0.85–1.30，超出即拒绝（防止 0/负数毁掉界面）
        if !(0.85..=1.30).contains(&value) {
            return Err(LocalizedError::new("error.settings_ui_scale"));
        }
        next.ui_scale = (value * 100.0).round() / 100.0;
    }
    if (selection_changed || providers_changed)
        && !(next.provider_id.is_empty() && next.model_id.is_empty())
    {
        provider::validate_available_model(
            &next.provider_id,
            &next.model_id,
            &provider::available_model_keys(&next.providers),
        )?;
    }
    if auxiliary_selection_changed || auxiliary_enabling {
        validate_auxiliary_selection(&next, &provider::available_model_keys(&next.providers))?;
    }
    if (subagent_selection_changed || subagent_enabling || providers_changed)
        && next.subagent_enabled
        && (!next.subagent_execution_provider_id.is_empty()
            || !next.subagent_execution_model_id.is_empty())
    {
        provider::validate_available_model(
            &next.subagent_execution_provider_id,
            &next.subagent_execution_model_id,
            &provider::available_model_keys(&next.providers),
        )?;
    }
    persist(&app, &next)?;
    if providers_changed {
        provider::install_catalog(&state, &provider::configured_catalog(&next.providers));
        let retained = next
            .providers
            .iter()
            .filter_map(|profile| {
                (!profile.credential_ref.is_empty()).then_some(profile.credential_ref.as_str())
            })
            .collect::<HashSet<_>>();
        for reference in previous_credentials {
            if !retained.contains(reference.as_str()) {
                crate::credentials::delete(&reference);
            }
        }
    }
    *state.settings.write() = next.clone();
    Ok(next)
}

#[tauri::command]
pub fn kf_session_model_update(
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
    provider: String,
    model: String,
) -> KfResult<SessionSnapshot> {
    let available_models = state.available_models.read().clone();
    let snapshot = {
        let mut sessions = state.sessions.write();
        let session = sessions.get_mut(&session_id).ok_or_else(|| {
            LocalizedError::new("error.session_not_found").arg("sessionId", &session_id)
        })?;
        apply_session_model(session, provider, model, &available_models)?;
        session.clone()
    };
    crate::persistence::save(&state)?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::UsageSnapshot;

    fn session(status: &str) -> SessionSnapshot {
        SessionSnapshot {
            id: "session".into(),
            title: "Session".into(),
            provider_id: provider::PROVIDER_ID.into(),
            model_id: provider::MODEL_ID.into(),
            project_root: None,
            status: status.into(),
            last_error: None,
            messages: Vec::new(),
            task: None,
            usage: UsageSnapshot {
                fresh_input_tokens: 0,
                cache_read_tokens: 0,
                output_tokens: 0,
                reasoning_tokens: 0,
                request_count: 0,
                current_context_tokens: None,
            },
        }
    }

    #[test]
    fn model_selection_accepts_only_runtime_catalog_models() {
        let available = HashSet::from([format!("{}\0future-code-9-free", provider::PROVIDER_ID)]);
        assert!(
            provider::validate_available_model(
                provider::PROVIDER_ID,
                "future-code-9-free",
                &available
            )
            .is_ok()
        );
        assert_eq!(
            provider::validate_available_model("other", "future-code-9-free", &available)
                .expect_err("unknown provider must be rejected")
                .key,
            "error.provider_unsupported"
        );
        assert_eq!(
            provider::validate_available_model(provider::PROVIDER_ID, "not-live-free", &available)
                .expect_err("model absent from the live catalog must be rejected")
                .key,
            "error.model_unsupported"
        );
    }

    #[test]
    fn idle_session_switches_to_an_available_model() {
        let mut session = session("idle");
        let available = HashSet::from([format!("{}\0future-code-9-free", provider::PROVIDER_ID)]);

        apply_session_model(
            &mut session,
            provider::PROVIDER_ID.into(),
            "future-code-9-free".into(),
            &available,
        )
        .expect("idle session should switch models");

        assert_eq!(session.provider_id, provider::PROVIDER_ID);
        assert_eq!(session.model_id, "future-code-9-free");
    }

    #[test]
    fn streaming_session_rejects_update_without_mutation() {
        let mut session = session("streaming");
        let original_provider = session.provider_id.clone();
        let original_model = session.model_id.clone();

        let error = apply_session_model(
            &mut session,
            "unsupported-provider".into(),
            "unsupported-model".into(),
            &HashSet::new(),
        )
        .expect_err("streaming session must reject all model updates first");

        assert_eq!(error.key, "error.session_busy");
        assert_eq!(session.provider_id, original_provider);
        assert_eq!(session.model_id, original_model);
    }

    #[test]
    fn auxiliary_selection_uses_the_runtime_catalog() {
        let settings = SettingsSnapshot {
            auxiliary_enabled: true,
            auxiliary_provider_id: provider::PROVIDER_ID.into(),
            auxiliary_model_id: provider::AUXILIARY_MODEL_ID.into(),
            ..Default::default()
        };
        let available = HashSet::from([format!(
            "{}\0{}",
            provider::PROVIDER_ID,
            provider::AUXILIARY_MODEL_ID
        )]);
        validate_auxiliary_selection(&settings, &available)
            .expect("the configured auxiliary model is live");

        let unavailable = SettingsSnapshot {
            auxiliary_model_id: "offline-free".into(),
            ..settings
        };
        assert_eq!(
            validate_auxiliary_selection(&unavailable, &available)
                .expect_err("an absent auxiliary model must be rejected")
                .key,
            "error.model_unsupported"
        );
    }

    #[test]
    fn auxiliary_selection_rejects_an_unknown_provider() {
        let settings = SettingsSnapshot {
            auxiliary_provider_id: "other".into(),
            auxiliary_model_id: provider::AUXILIARY_MODEL_ID.into(),
            ..Default::default()
        };
        let available = HashSet::from([format!(
            "{}\0{}",
            provider::PROVIDER_ID,
            provider::AUXILIARY_MODEL_ID
        )]);
        assert_eq!(
            validate_auxiliary_selection(&settings, &available)
                .expect_err("the reducer must use a supported provider")
                .key,
            "error.provider_unsupported"
        );
    }
}
