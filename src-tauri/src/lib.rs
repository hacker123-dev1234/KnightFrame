pub mod agent_loop;
mod browser;
mod credentials;
mod error;
pub mod headless;
pub mod market;
pub mod plugin;
mod plugin_studio;
pub mod project;
pub mod provider;
mod runtime;
mod session;
mod settings;
pub mod skill;
mod state;
mod task;
pub mod tools;
mod types;

use crate::{
    error::KfResult,
    state::AppState,
    types::{BootstrapPayload, BrowserSnapshot},
};
use std::{collections::BTreeMap, sync::Arc};
use tauri::{AppHandle, Manager};

#[tauri::command]
async fn kf_app_bootstrap(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> KfResult<BootstrapPayload> {
    let configured = state.settings.read().providers.clone();
    if let Some(profiles) = provider::sync_catalog_metadata(&state.client, &configured).await {
        let mut next = state.settings.read().clone();
        next.providers = profiles;
        let _ = settings::persist(&app, &next);
        *state.settings.write() = next;
    }
    let models = provider::configured_catalog(&state.settings.read().providers);
    provider::install_catalog(&state, &models);
    let sessions: Vec<_> = state.sessions.read().values().cloned().collect();
    let project = state.active_project.read().as_ref().and_then(|root| {
        state
            .projects
            .read()
            .get(root)
            .map(|value| value.snapshot.clone())
    });
    Ok(BootstrapPayload {
        settings: state.settings.read().clone(),
        providers: models,
        provider_templates: provider::PROVIDER_TEMPLATES.to_vec(),
        sessions,
        active_session_id: None,
        project,
        browser: BrowserSnapshot {
            available: true,
            open: false,
            url: None,
            title: None,
            can_go_back: false,
            can_go_forward: false,
            loading: false,
            active_tab_id: None,
            tabs: Vec::new(),
        },
        features: BTreeMap::from([
            ("browser".into(), true),
            ("projectIndex".into(), true),
            ("projectManifest".into(), true),
            ("usageLedger".into(), true),
            ("skillCatalog".into(), true),
            ("skillLocalRouter".into(), true),
            ("skillOpt".into(), true),
            ("marketAnalysis".into(), true),
            ("miniAssistant".into(), false),
        ]),
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            let settings = settings::load(app.handle());
            app.manage(AppState::new(settings));
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.manage(std::sync::Arc::new(market::MarketState::new(&config_dir)));
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "plugin-studio" && matches!(event, tauri::WindowEvent::Destroyed) {
                plugin_studio::stop_dsh_preview();
            }
        })
        .invoke_handler(tauri::generate_handler![
            kf_app_bootstrap,
            settings::kf_settings_update,
            settings::kf_session_model_update,
            provider::kf_provider_probe,
            skill::kf_skill_catalog,
            skill::kf_skill_set_enabled,
            session::kf_session_create,
            session::kf_session_rename,
            session::kf_session_delete,
            session::kf_session_send,
            session::kf_session_stop,
            task::kf_task_command,
            project::kf_project_open,
            project::kf_project_query,
            project::kf_project_graph,
            tools::kf_tool_read,
            tools::kf_tool_edit,
            tools::kf_tool_search,
            tools::kf_tool_run,
            browser::kf_browser_command,
            market::kf_market_settings_get,
            market::kf_market_settings_update,
            market::kf_market_fetch,
            market::kf_market_subscribe,
            market::kf_market_unsubscribe,
            market::kf_market_analyze,
            market::kf_market_stop_analysis,
            market::kf_market_chat_send,
            market::kf_market_chat_stop,
            market::kf_market_records,
            market::kf_market_record_load,
            plugin::kf_plugin_studio_validate,
            plugin::kf_plugin_studio_export_preview,
            browser::kf_browser_rect,
            plugin_studio::kf_plugin_studio_open,
            plugin_studio::kf_plugin_studio_bootstrap,
            plugin_studio::kf_plugin_studio_ready,
            plugin_studio::kf_plugin_studio_dsh_start,
            plugin_studio::kf_plugin_studio_dsh_stop,
            plugin_studio::kf_plugin_studio_ui_relay,
            plugin_studio::kf_plugin_studio_export,
            plugin_studio::kf_plugin_studio_ask,
        ])
        .run(tauri::generate_context!())
        .expect("error.run_tauri");
}

#[cfg(all(test, windows))]
mod runtime_smoke_tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn agent_run_waits_for_a_long_process() {
        let directory = tempfile::tempdir().expect("temporary workspace");
        let state = AppState::new(Default::default());
        let root = directory.path().display().to_string();
        let result = tools::run_for_agent(
            &state,
            &root,
            "powershell.exe".into(),
            vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                "Start-Sleep -Milliseconds 1250; Write-Output long-task-ready".into(),
            ],
            &CancellationToken::new(),
        )
        .await
        .expect("long-running command should not have a fixed timeout");

        assert_eq!(result.exit_code, Some(0));
        assert!(result.elapsed_ms >= 1_000);
        assert!(result.stdout.contains("long-task-ready"));
    }
}
