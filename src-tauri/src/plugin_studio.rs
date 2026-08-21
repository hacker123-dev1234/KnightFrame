use crate::{
    error::{KfResult, LocalizedError},
    plugin::{self, StudioRequest, StudioTarget},
    types::Ack,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Mutex, OnceLock, mpsc},
    time::Duration,
};
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, window::Color,
};

const STUDIO_LABEL: &str = "plugin-studio";
const MAIN_LABEL: &str = "main";
const STUDIO_EVENT: &str = "kf://plugin-studio-request";
const STUDIO_URL: &str = "studio.html";
const STUDIO_BACKGROUND: Color = Color(7, 7, 7, 255);
const DSH_LOOPBACK_PREFIX: &str = "http://127.0.0.1:";

fn dsh_child() -> &'static Mutex<Option<Child>> {
    static CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
    CHILD.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshPreviewStatus {
    available: bool,
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

fn dsh_cli() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = std::env::var_os("KF_DSH_ROOT") {
        roots.push(PathBuf::from(root));
    }
    if let Ok(current) = std::env::current_dir() {
        roots.push(current.join("deepseek-harness-master"));
        if let Some(parent) = current.parent() {
            roots.push(parent.join("deepseek-harness-master"));
        }
    }
    // 打包部署：exe 旁边（或上一级）内置的 DSH 构建
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        roots.push(exe_dir.join("deepseek-harness-master"));
        if let Some(parent) = exe_dir.parent() {
            roots.push(parent.join("deepseek-harness-master"));
        }
    }
    roots.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
            .parent()
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
            .join("deepseek-harness-master"),
    );
    roots
        .into_iter()
        .map(|root| root.join("apps/cli/lib/bin.js"))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn stop_dsh_preview() {
    if let Ok(mut slot) = dsh_child().lock()
        && let Some(mut child) = slot.take()
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[tauri::command]
pub fn kf_plugin_studio_dsh_stop() -> Ack {
    stop_dsh_preview();
    Ack { ok: true }
}

/// "dsh web: http://127.0.0.1:PORT (LAN: …)" —— 只取第一个空白分隔的
/// URL token；带 LAN 后缀的整行不是合法 iframe src（DSH 不加载的根因）。
fn dsh_url_token(line: &str) -> Option<String> {
    line.strip_prefix("dsh web: ")?
        .split_whitespace()
        .next()
        .map(str::to_owned)
}

#[tauri::command]
pub fn kf_plugin_studio_dsh_start() -> KfResult<DshPreviewStatus> {
    stop_dsh_preview();
    let Some(cli) = dsh_cli() else {
        return Ok(DshPreviewStatus {
            available: false,
            running: false,
            url: None,
            reason: Some("dsh-build-missing".into()),
        });
    };
    let profile = std::env::temp_dir().join("knightframe-studio-dsh-home");
    std::fs::create_dir_all(&profile)
        .map_err(|error| LocalizedError::new("error.plugin_studio_window").arg("detail", error))?;
    // cwd 指向 DSH 仓库根：分层 env / 工作区知识按仓库根解析
    let repo_root = cli
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mut command = Command::new("node.exe");
    command
        .arg(&cli)
        .args(["web", "--port", "0"])
        .current_dir(&repo_root)
        .env("DSH_HOME", profile)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|error| LocalizedError::new("error.plugin_studio_window").arg("detail", error))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        LocalizedError::new("error.plugin_studio_window").arg("detail", "dsh-stdout-missing")
    })?;
    let stderr = child.stderr.take();
    enum DshLine {
        Url(String),
        Diag(String),
    }
    let (sender, receiver) = mpsc::sync_channel::<DshLine>(16);
    let stdout_sender = sender.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(url) = dsh_url_token(&line) {
                let _ = stdout_sender.send(DshLine::Url(url));
                return;
            }
            let _ = stdout_sender.send(DshLine::Diag(line));
        }
    });
    if let Some(stderr) = stderr {
        let stderr_sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = stderr_sender.send(DshLine::Diag(line));
            }
        });
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut first_diagnostic: Option<String> = None;
    let url = loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break None;
        }
        match receiver.recv_timeout(remaining) {
            Ok(DshLine::Url(url)) if url.starts_with(DSH_LOOPBACK_PREFIX) => break Some(url),
            Ok(DshLine::Url(_)) => continue, // 非 loopback URL 忽略
            Ok(DshLine::Diag(line)) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() && first_diagnostic.is_none() {
                    first_diagnostic = Some(trimmed.to_string());
                }
            }
            Err(_) => break None,
        }
    };
    let Some(url) = url else {
        let _ = child.kill();
        let _ = child.wait();
        // 进程提前退出（依赖缺失/构建缺失）与超时区分开，带上首条诊断
        let exited = child
            .try_wait()
            .ok()
            .flatten()
            .is_some_and(|status| !status.success());
        let reason = if exited {
            format!(
                "dsh-exited: {}",
                first_diagnostic.unwrap_or_else(|| "process exited".into())
            )
        } else {
            "dsh-start-timeout".into()
        };
        return Ok(DshPreviewStatus {
            available: true,
            running: false,
            url: None,
            reason: Some(reason),
        });
    };
    *dsh_child().lock().map_err(|_| {
        LocalizedError::new("error.plugin_studio_window").arg("detail", "dsh-child-lock")
    })? = Some(child);
    Ok(DshPreviewStatus {
        available: true,
        running: true,
        url: Some(url),
        reason: None,
    })
}

/// 插件预览按钮驱动的原生页面白名单（与前端 Page 联合对齐）。
pub const STUDIO_UI_PAGES: &[&str] = &[
    "workspace",
    "settings",
    "browser",
    "market",
    "graph",
    "studio",
];

/// 预览覆盖层的按钮 → 主窗口原生 UI：切换页面 / 打开插件工坊。
/// 工坊窗口与主窗口分属两个 webview，经后端事件转发实现"KF 与原生
/// UI 层互动"。
#[tauri::command]
pub fn kf_plugin_studio_ui_relay(app: AppHandle, page: String) -> KfResult<Ack> {
    if !STUDIO_UI_PAGES.contains(&page.as_str()) {
        return Err(LocalizedError::new("error.plugin_ui_page").arg("page", page));
    }
    app.emit_to(
        MAIN_LABEL,
        "kf://runtime",
        crate::types::RuntimeEvent::new("ui.page", serde_json::json!({"page": page})),
    )
    .map_err(|error| LocalizedError::new("error.plugin_studio_window").arg("detail", error))?;
    Ok(Ack { ok: true })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioBootstrap {
    locale: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StudioAskPayload {
    pub studio: StudioRequest,
    pub content: String,
    pub requirement: String,
    #[serde(default)]
    pub selected_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudioDispatch {
    content: String,
    target: StudioTarget,
    plugin_id: String,
    component_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_id: Option<String>,
    requirement: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioExportReceipt {
    pub ok: bool,
    pub path: String,
}

fn validated_dispatch(request: &StudioAskPayload) -> KfResult<StudioDispatch> {
    let validation = plugin::kf_plugin_studio_validate(request.studio.clone())?;
    let content = request.content.trim();
    let requirement = request.requirement.trim();
    if content.is_empty() || requirement.is_empty() {
        return Err(LocalizedError::new("error.plugin_studio_request"));
    }
    if let Some(selected_id) = request.selected_id.as_deref()
        && !validation
            .manifest
            .ui
            .iter()
            .any(|component| component.id() == selected_id)
    {
        return Err(LocalizedError::new("error.plugin_ui_id").arg("id", selected_id));
    }
    Ok(StudioDispatch {
        content: content.to_owned(),
        target: validation.target,
        plugin_id: validation.manifest.id,
        component_count: validation.manifest.ui.len(),
        selected_id: request.selected_id.clone(),
        requirement: requirement.to_owned(),
    })
}

#[tauri::command]
pub async fn kf_plugin_studio_open(app: AppHandle) -> KfResult<Ack> {
    if let Some(window) = app.get_webview_window(STUDIO_LABEL) {
        if window.is_visible().unwrap_or(false) {
            window.set_focus().map_err(|error| {
                LocalizedError::new("error.plugin_studio_window").arg("detail", error)
            })?;
            return Ok(Ack { ok: true });
        }
        window.close().map_err(|error| {
            LocalizedError::new("error.plugin_studio_window").arg("detail", error)
        })?;
    }

    // 工坊尽量做大：按主显示器 88% 开窗（内置 KF/DSH 预览不缩水），
    // 拿不到显示器时回退 1680×1000。
    let (width, height) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let size = monitor.size();
            let scale = monitor.scale_factor().max(0.01);
            (
                ((size.width as f64 / scale) * 0.88)
                    .round()
                    .clamp(1180.0, 2200.0),
                ((size.height as f64 / scale) * 0.88)
                    .round()
                    .clamp(760.0, 1400.0),
            )
        })
        .unwrap_or((1680.0, 1000.0));
    WebviewWindowBuilder::new(&app, STUDIO_LABEL, WebviewUrl::App(STUDIO_URL.into()))
        .background_color(STUDIO_BACKGROUND)
        .title("KnightFrame Plugin Studio Beta")
        .decorations(false)
        .visible(false)
        .inner_size(width, height)
        .min_inner_size(1280.0, 800.0)
        .center()
        .build()
        .map_err(|error| LocalizedError::new("error.plugin_studio_window").arg("detail", error))?;
    Ok(Ack { ok: true })
}

#[tauri::command]
pub fn kf_plugin_studio_bootstrap(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> StudioBootstrap {
    StudioBootstrap {
        locale: match state.settings.read().locale.as_str() {
            "en-US" => "en-US".into(),
            _ => "zh-CN".into(),
        },
    }
}

#[tauri::command]
pub fn kf_plugin_studio_ready(window: WebviewWindow) -> KfResult<Ack> {
    if window.label() != STUDIO_LABEL {
        return Err(LocalizedError::new("error.plugin_studio_window")
            .arg("detail", "unexpected-ready-source"));
    }
    window
        .show()
        .and_then(|_| window.set_focus())
        .map_err(|error| LocalizedError::new("error.plugin_studio_window").arg("detail", error))?;
    Ok(Ack { ok: true })
}

#[tauri::command]
pub fn kf_plugin_studio_ask(app: AppHandle, request: StudioAskPayload) -> KfResult<Ack> {
    let dispatch = validated_dispatch(&request)?;
    app.emit_to(MAIN_LABEL, STUDIO_EVENT, dispatch)
        .map_err(|error| LocalizedError::new("error.plugin_studio_window").arg("detail", error))?;
    if let Some(main) = app.get_webview_window(MAIN_LABEL) {
        let _ = main.show();
        let _ = main.unminimize();
        let _ = main.set_focus();
    }
    Ok(Ack { ok: true })
}

fn export_root(path: &str) -> KfResult<PathBuf> {
    let root = PathBuf::from(path);
    if !root.is_absolute() || !root.is_dir() {
        return Err(LocalizedError::new("error.plugin_studio_export_root"));
    }
    Ok(root)
}

fn export_directory(root: &Path, id: &str, target: StudioTarget) -> KfResult<PathBuf> {
    let suffix = match target {
        StudioTarget::Knightframe => "knightframe",
        StudioTarget::Dsh => "dsh",
    };
    let base = format!("{id}-{suffix}");
    for index in 1..=999_u16 {
        let name = if index == 1 {
            base.clone()
        } else {
            format!("{base}-{index}")
        };
        let candidate = root.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(LocalizedError::new("error.plugin_studio_export").arg("detail", "name-exhausted"))
}

async fn write_export(root: &Path, preview: &plugin::StudioExportPreview) -> KfResult<PathBuf> {
    let manifest: plugin::PluginManifest = serde_json::from_str(&preview.manifest_json)
        .map_err(|error| LocalizedError::new("error.plugin_manifest_json").arg("detail", error))?;
    let dsh_runtime_json = serde_json::to_string_pretty(&preview.dsh_runtime)
        .map_err(|error| LocalizedError::new("error.plugin_studio_export").arg("detail", error))?;
    let directory = export_directory(root, &manifest.id, preview.target)?;
    tokio::fs::create_dir(&directory)
        .await
        .map_err(|error| LocalizedError::new("error.plugin_studio_export").arg("detail", error))?;

    let files = [
        ("knightframe.plugin.json", preview.manifest_json.as_str()),
        ("cordis.yml", preview.cordis_yaml.as_str()),
        (
            "client-contribution.json",
            preview.client_contribution_json.as_str(),
        ),
        ("dsh-client-code.js", preview.dsh_client_code.as_str()),
        (
            "cordis-define-arguments.json",
            preview.dsh_define_arguments_json.as_str(),
        ),
        ("dsh-runtime.json", dsh_runtime_json.as_str()),
        ("adapter-package.txt", preview.adapter_package.as_str()),
    ];
    for (name, content) in files {
        if let Err(error) = tokio::fs::write(directory.join(name), content).await {
            let _ = tokio::fs::remove_dir_all(&directory).await;
            return Err(LocalizedError::new("error.plugin_studio_export").arg("detail", error));
        }
    }
    Ok(directory)
}

#[tauri::command]
pub async fn kf_plugin_studio_export(
    request: StudioRequest,
    output_dir: String,
) -> KfResult<StudioExportReceipt> {
    let root = export_root(&output_dir)?;
    let preview = plugin::kf_plugin_studio_export_preview(request)?;
    let path = write_export(&root, &preview).await?;
    Ok(StudioExportReceipt {
        ok: true,
        path: path.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PixelViewport;
    use std::future::Future;

    fn valid_request() -> StudioRequest {
        StudioRequest {
            manifest_json: include_str!("../tests/fixtures/plugins/valid-plugin.json").into(),
            target: StudioTarget::Dsh,
            viewport: PixelViewport::default(),
        }
    }

    #[test]
    fn ask_rejects_empty_content_after_validating_the_layout() {
        let error = validated_dispatch(&StudioAskPayload {
            studio: valid_request(),
            content: "  ".into(),
            requirement: "  ".into(),
            selected_id: None,
        })
        .expect_err("empty request");
        assert_eq!(error.key, "error.plugin_studio_request");
    }

    #[test]
    fn studio_uses_a_dedicated_packaged_entry() {
        assert_eq!(STUDIO_URL, "studio.html");
        assert!(!STUDIO_URL.contains('?'));
    }

    #[test]
    fn dsh_url_line_yields_a_single_valid_token() {
        // 带 LAN 后缀：只取第一个 token（此前整行塞进 iframe 导致 DSH 预览黑屏）
        assert_eq!(
            dsh_url_token("dsh web: http://127.0.0.1:4567 (LAN: http://192.168.1.5:4567)"),
            Some("http://127.0.0.1:4567".to_string())
        );
        assert_eq!(
            dsh_url_token("dsh web: http://127.0.0.1:52909"),
            Some("http://127.0.0.1:52909".to_string())
        );
        assert_eq!(dsh_url_token("some other log line"), None);
        assert_eq!(dsh_url_token(""), None);
    }

    #[test]
    fn ui_relay_whitelist_covers_native_pages_and_studio() {
        for page in [
            "workspace",
            "settings",
            "browser",
            "market",
            "graph",
            "studio",
        ] {
            assert!(STUDIO_UI_PAGES.contains(&page));
        }
        assert!(!STUDIO_UI_PAGES.contains(&"about:blank"));
        assert!(!STUDIO_UI_PAGES.contains(&""));
    }

    #[test]
    fn studio_window_creation_command_stays_async() {
        fn assert_async_command<F, Fut>(_: F)
        where
            F: Fn(AppHandle) -> Fut,
            Fut: Future<Output = KfResult<Ack>>,
        {
        }

        assert_async_command(kf_plugin_studio_open);
    }

    #[tokio::test]
    async fn export_writes_a_complete_adapter_bundle_without_overwriting() {
        let root = tempfile::tempdir().expect("export root");
        let preview =
            plugin::kf_plugin_studio_export_preview(valid_request()).expect("valid Studio preview");
        let first = write_export(root.path(), &preview)
            .await
            .expect("first export");
        let second = write_export(root.path(), &preview)
            .await
            .expect("second export");

        assert_ne!(first, second);
        for path in [&first, &second] {
            assert!(path.join("knightframe.plugin.json").is_file());
            assert!(path.join("cordis.yml").is_file());
            assert!(path.join("client-contribution.json").is_file());
            assert!(path.join("dsh-client-code.js").is_file());
            assert!(path.join("cordis-define-arguments.json").is_file());
            let runtime: serde_json::Value = serde_json::from_str(
                &tokio::fs::read_to_string(path.join("dsh-runtime.json"))
                    .await
                    .expect("DSH runtime metadata"),
            )
            .expect("valid DSH runtime JSON");
            assert_eq!(
                runtime["hostRunnerPackage"],
                plugin::DSH_HOST_RUNNER_PACKAGE
            );
            assert_eq!(
                runtime["clientRunnerPackage"],
                plugin::DSH_CLIENT_RUNNER_PACKAGE
            );
            assert_eq!(runtime["requiresClientApproval"], true);
            assert_eq!(runtime["processLocal"], true);
            assert!(path.join("adapter-package.txt").is_file());
        }
    }
}
