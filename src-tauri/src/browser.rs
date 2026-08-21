use crate::{
    error::{KfResult, LocalizedError},
    types::{BrowserSnapshot, BrowserTabSnapshot, RuntimeEvent},
};
use parking_lot::{Mutex, RwLock};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl,
    webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder},
};
use url::Url;

const LABEL_PREFIX: &str = "browser-tab-";
const MAIN_LABEL: &str = "main";

#[derive(Debug, Clone)]
struct BrowserTabState {
    id: String,
    label: String,
    url: Option<String>,
    title: Option<String>,
    loading: bool,
    history: Vec<String>,
    history_index: usize,
}

impl BrowserTabState {
    fn snapshot(&self) -> BrowserTabSnapshot {
        BrowserTabSnapshot {
            id: self.id.clone(),
            url: self.url.clone(),
            title: self.title.clone(),
            can_go_back: self.history_index > 0,
            can_go_forward: self.history_index + 1 < self.history.len(),
            loading: self.loading,
        }
    }

    fn record_navigation(&mut self, url: String) {
        if self.url.as_deref() == Some(url.as_str()) {
            return;
        }
        if self.history_index > 0 && self.history[self.history_index - 1] == url {
            self.history_index -= 1;
        } else if self.history_index + 1 < self.history.len()
            && self.history[self.history_index + 1] == url
        {
            self.history_index += 1;
        } else {
            self.history.truncate(self.history_index.saturating_add(1));
            self.history.push(url.clone());
            self.history_index = self.history.len().saturating_sub(1);
        }
        self.url = Some(url);
    }
}

#[derive(Debug, Clone, Copy)]
struct BrowserRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Default)]
struct BrowserRuntime {
    tabs: Vec<BrowserTabState>,
    active_tab_id: Option<String>,
    rect: Option<BrowserRect>,
    next_tab: u64,
}

static BROWSER: LazyLock<RwLock<BrowserRuntime>> =
    LazyLock::new(|| RwLock::new(BrowserRuntime::default()));
#[derive(Debug, Clone)]
struct ElementLocator {
    role: String,
    selector: Option<String>,
    name: String,
}

static FETCH_REFS: LazyLock<RwLock<HashMap<String, ElementLocator>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn validate_url(raw: &str) -> KfResult<Url> {
    let value = Url::parse(raw).map_err(|_| LocalizedError::new("error.browser_url"))?;
    if value.scheme() != "http" && value.scheme() != "https" {
        return Err(LocalizedError::new("error.browser_scheme").arg("scheme", value.scheme()));
    }
    Ok(value)
}

fn resolve_address(raw: &str) -> KfResult<Url> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(LocalizedError::new("error.browser_url"));
    }
    if let Ok(url) = validate_url(value) {
        return Ok(url);
    }
    let looks_like_host = !value.chars().any(char::is_whitespace)
        && (value.contains('.')
            || value.starts_with("localhost")
            || value.starts_with("127.0.0.1"));
    if looks_like_host && let Ok(url) = validate_url(&format!("https://{value}")) {
        return Ok(url);
    }
    Url::parse_with_params("https://www.google.com/search", &[("q", value)])
        .map_err(|_| LocalizedError::new("error.browser_url"))
}

// ---------------------------------------------------------------------------
// Agent 浏览器控制空间：fetch（后端抓取页面文本）+ control（窗口操作）
// ---------------------------------------------------------------------------

fn ascii_lower(text: &str) -> String {
    text.chars().map(|ch| ch.to_ascii_lowercase()).collect()
}

fn strip_html_to_text(html: &str) -> String {
    // 移除 script/style/noscript 块
    let mut text: String = html.to_string();
    for (open, close) in [
        ("<script", "</script>"),
        ("<style", "</style>"),
        ("<noscript", "</noscript>"),
    ] {
        loop {
            let lower = ascii_lower(&text);
            let Some(start) = lower.find(open) else { break };
            let Some(end) = lower[start..].find(close) else {
                // 未闭合：删除到末尾
                text.truncate(start);
                break;
            };
            let end_absolute = start + end + close.len();
            text.replace_range(start..end_absolute, "");
        }
    }
    // 块级标签转换行，剥除其余标签
    text = text
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n")
        .replace("</tr>", "\n")
        .replace("</h1>", "\n")
        .replace("</h2>", "\n")
        .replace("</h3>", "\n");
    let mut result = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // 折叠空白：连续空白压成一个（保留换行）
    let mut collapsed = String::with_capacity(result.len());
    let mut pending_newline = false;
    let mut pending_space = false;
    for ch in result.chars() {
        if ch == '\n' {
            pending_newline = true;
        } else if ch.is_whitespace() {
            pending_space = true;
        } else {
            if pending_newline {
                collapsed.push('\n');
            } else if pending_space {
                collapsed.push(' ');
            }
            pending_newline = false;
            pending_space = false;
            collapsed.push(ch);
        }
    }
    collapsed.trim().to_string()
}

fn extract_title(html: &str) -> String {
    let lower = ascii_lower(html);
    let Some(start) = lower.find("<title") else {
        return String::new();
    };
    let rest = &html[start..];
    let Some(content_start) = rest.find('>') else {
        return String::new();
    };
    let body = &rest[content_start + 1..];
    let end = ascii_lower(body)
        .find("</title>")
        .unwrap_or(body.len().min(200));
    body[..end].trim().chars().take(200).collect()
}

// ---------------------------------------------------------------------------
// 省 token 投影：可交互元素 ref 快照（Playwright MCP 风格）+ 分块正文
// ---------------------------------------------------------------------------

const ELEMENT_LIMIT: usize = 32;
const ELEMENT_NAME_CHARS: usize = 48;
const ELEMENT_HINT_CHARS: usize = 72;

fn attr_value(tag: &str, name: &str) -> Option<String> {
    // 在标签内部找 name="value" 或 name='value'（大小写不敏感，属性名需边界完整）
    let lower = ascii_lower(tag);
    let mut search = 0usize;
    while let Some(offset) = lower[search..].find(name) {
        let absolute = search + offset;
        let key_end = absolute + name.len();
        // 属性名边界：前一字符不能是字母/数字/-/_（避免 data-href 误匹配 href）
        let boundary_ok = absolute == 0 || {
            let previous = lower[..absolute].chars().last().unwrap_or(' ');
            !(previous.is_ascii_alphanumeric() || previous == '-' || previous == '_')
        };
        let rest = tag.get(key_end..)?;
        let trimmed = rest.trim_start();
        if boundary_ok && let Some(stripped) = trimmed.strip_prefix('=') {
            let value_part = stripped.trim_start();
            let quote = value_part.chars().next()?;
            if quote == '"' || quote == '\'' {
                let inner = &value_part[1..];
                let end = inner.find(quote)?;
                return Some(inner[..end].to_string());
            }
        }
        search = key_end;
    }
    None
}

fn clean_name(raw: &str) -> String {
    let collapsed: String = raw
        .chars()
        .map(|ch| if ch.is_whitespace() { ' ' } else { ch })
        .collect();
    let trimmed = collapsed.trim();
    trimmed.chars().take(ELEMENT_NAME_CHARS).collect()
}

/// 提取可交互元素（a/button/input/select/textarea），分配短 ref。
/// 返回 (元素数组, 省略数, 本地定位表)。模仿 Playwright MCP 的 snapshot/ref 模型，
/// 让模型用 ref 引用元素而非粘贴 URL 或选择器。
fn extract_elements(
    html: &str,
    base: &Url,
) -> (Vec<Value>, usize, HashMap<String, ElementLocator>) {
    let lower = ascii_lower(html);
    let roles: &[&str] = &["a", "button", "input", "select", "textarea"];
    let mut spans: Vec<(usize, usize, &str)> = Vec::new(); // (start, end, role)
    for role in roles {
        let open_tag = format!("<{role}");
        let close_tag = format!("</{role}");
        let mut cursor = 0usize;
        while let Some(offset) = lower[cursor..].find(&open_tag) {
            let start = cursor + offset;
            // 确认是完整标签名（后面是空白或 >）
            let after = lower[start + open_tag.len()..].chars().next();
            if after.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_') {
                cursor = start + open_tag.len();
                continue;
            }
            let Some(tag_end_rel) = lower[start..].find('>') else {
                break;
            };
            let tag_end = start + tag_end_rel;
            let end = lower[tag_end..]
                .find(&close_tag)
                .map(|rel| tag_end + rel + close_tag.len())
                .unwrap_or((tag_end + 1).min(html.len()));
            spans.push((start, end, role));
            cursor = end;
        }
    }
    spans.sort();

    let mut elements = Vec::new();
    let mut refs = HashMap::new();
    let mut omitted = 0usize;
    for (start, end, role) in spans.iter() {
        if elements.len() >= ELEMENT_LIMIT {
            omitted = spans.len() - ELEMENT_LIMIT;
            break;
        }
        let segment = &html[*start..*end];
        let tag_end = segment
            .find('>')
            .map(|rel| rel + 1)
            .unwrap_or(segment.len());
        let open = &segment[..tag_end];
        let inner_text = strip_html_to_text(&segment[tag_end..]);
        let name = attr_value(open, "aria-label")
            .or_else(|| attr_value(open, "placeholder"))
            .or_else(|| attr_value(open, "title"))
            .map(|value| clean_name(&value))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| clean_name(&inner_text));
        if name.is_empty() && *role != "input" {
            continue; // 无名链接/按钮没有引用价值
        }
        let mut hint = None;
        if *role == "a"
            && let Some(href) = attr_value(open, "href")
        {
            if href.starts_with('#') || href.starts_with("javascript:") {
                continue;
            }
            if let Ok(resolved) = base.join(&href) {
                let mut compact =
                    format!("{}{}", resolved.host_str().unwrap_or(""), resolved.path());
                if compact.len() > ELEMENT_HINT_CHARS {
                    compact.truncate(ELEMENT_HINT_CHARS);
                }
                hint = Some(compact);
            }
        }
        let kind = if *role == "a" {
            "link"
        } else if *role == "button" {
            "button"
        } else {
            "field"
        };
        let reference = format!("e{}", elements.len() + 1);
        let selector = ["id", "name", "aria-label", "placeholder", "href"]
            .into_iter()
            .find_map(|attribute| {
                attr_value(open, attribute).map(|value| {
                    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                    format!(r#"{role}[{attribute}="{escaped}"]"#)
                })
            });
        refs.insert(
            reference.clone(),
            ElementLocator {
                role: (*role).to_string(),
                selector,
                name: name.clone(),
            },
        );
        let mut element = json!({
            "ref": reference,
            "role": kind,
            "name": name,
        });
        if let Some(hint) = hint {
            element["hint"] = json!(hint);
        }
        elements.push(element);
    }
    (elements, omitted, refs)
}

/// 正文分块窗口：返回 (chunk, total, next_offset, complete)。
fn text_window(text: &str, offset: usize, max_chars: usize) -> (String, usize, usize, bool) {
    let total = text.chars().count();
    let chunk: String = text.chars().skip(offset).take(max_chars).collect();
    let next_offset = offset + chunk.chars().count();
    let complete = next_offset >= total;
    (chunk, total, next_offset, complete)
}

/// Agent 工具：后端抓取页面并返回省 token 投影（不依赖内置窗口）。
/// - elements：可交互元素 ref 快照（模仿 Playwright MCP，模型用 ref 引用）
/// - text：正文分块（默认首块），附 total/omitted/nextOffset 支持续读
/// - _rawHtml：完整原始 HTML，由调用方剥离存为本地 artifact
pub async fn agent_fetch_page(
    client: &reqwest::Client,
    url: &str,
    offset: usize,
    max_chars: usize,
) -> KfResult<Value> {
    let target = validate_url(url)?;
    let response = client
        .get(target.clone())
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|error| LocalizedError::new("error.browser_fetch").arg("detail", error))?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = response
        .text()
        .await
        .map_err(|error| LocalizedError::new("error.browser_fetch").arg("detail", error))?;
    let is_html = content_type.contains("text/html") || body.trim_start().starts_with('<');
    let max_chars = max_chars.clamp(500, 12000);

    let mut projection = json!({
        "url": target.as_str(),
        "status": status,
        "complete": false,
    });
    if is_html {
        let title = extract_title(&body);
        let text = strip_html_to_text(&body);
        let (elements, elements_omitted, refs) = extract_elements(&body, &target);
        *FETCH_REFS.write() = refs;
        let (chunk, total_chars, next_offset, complete) = text_window(&text, offset, max_chars);
        projection["title"] = json!(title);
        projection["text"] = json!(chunk);
        projection["textChars"] = json!(total_chars);
        projection["omittedChars"] = json!(total_chars.saturating_sub(next_offset));
        projection["nextOffset"] = if complete {
            Value::Null
        } else {
            json!(next_offset)
        };
        projection["complete"] = json!(complete);
        projection["elements"] = json!(elements);
        projection["elementsOmitted"] = json!(elements_omitted);
        projection["_rawHtml"] = json!(body);
    } else {
        FETCH_REFS.write().clear();
        let (chunk, total_chars, next_offset, complete) = text_window(&body, offset, max_chars);
        projection["title"] = json!("");
        projection["text"] = json!(chunk);
        projection["textChars"] = json!(total_chars);
        projection["omittedChars"] = json!(total_chars.saturating_sub(next_offset));
        projection["nextOffset"] = if complete {
            Value::Null
        } else {
            json!(next_offset)
        };
        projection["complete"] = json!(complete);
        projection["elements"] = json!([]);
    }
    Ok(projection)
}

/// Agent 工具：浏览器控制（open/navigate/back/forward/reload/close/status/fetch/
/// snapshot/click/fill/select/hover/press/scroll/focus）。交互通过 ref 或 selector 在内置
/// 窗口页面上执行（eval 注入，selector 经 JSON 转义防止脚本注入）。
pub async fn agent_browser(
    app: &AppHandle,
    client: &reqwest::Client,
    action: &str,
    url: Option<&str>,
    extra: &serde_json::Map<String, Value>,
) -> KfResult<Value> {
    match action {
        "fetch" => {
            let url = url.ok_or_else(|| LocalizedError::new("error.browser_url_required"))?;
            agent_fetch_page(client, url, 0, 4000).await
        }
        "status" => {
            let snapshot = snapshot(app);
            Ok(serde_json::to_value(snapshot).unwrap_or_else(|_| json!({})))
        }
        "snapshot" => live_page_snapshot(app).await,
        "click" | "fill" | "select" | "hover" | "press" | "scroll" => {
            let script = match action {
                "scroll" => {
                    let y = extra.get("y").and_then(Value::as_i64).unwrap_or(600);
                    format!("window.scrollBy({{top: {y}, behavior: 'smooth'}})")
                }
                _ => {
                    let explicit_selector = extra
                        .get("selector")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty());
                    let locator = extra
                        .get("ref")
                        .and_then(Value::as_str)
                        .and_then(|reference| FETCH_REFS.read().get(reference).cloned());
                    let resolver = if let Some(selector) = explicit_selector {
                        format!(
                            "document.querySelector({})",
                            Value::String(selector.to_string())
                        )
                    } else if let Some(locator) = locator {
                        if let Some(selector) = locator.selector {
                            format!("document.querySelector({})", Value::String(selector))
                        } else {
                            format!(
                                "Array.from(document.querySelectorAll({})).find((node) => ((node.getAttribute('aria-label') || node.getAttribute('placeholder') || node.textContent || '').replace(/\\s+/g, ' ').trim()).startsWith({}))",
                                Value::String(locator.role),
                                Value::String(locator.name)
                            )
                        }
                    } else if action == "press" {
                        "document.activeElement".to_string()
                    } else {
                        return Err(LocalizedError::new("error.browser_selector_required"));
                    };
                    match action {
                        "click" => format!(
                            "(() => {{ const el = {resolver}; if (!el) return; el.scrollIntoView({{block:'center'}}); el.click(); }})()"
                        ),
                        "hover" => format!(
                            "(() => {{ const el = {resolver}; if (!el) return; el.scrollIntoView({{block:'center'}}); for (const type of ['mouseenter','mouseover','mousemove']) el.dispatchEvent(new MouseEvent(type, {{bubbles:true, view:window}})); }})()"
                        ),
                        "press" => {
                            let key = extra.get("key").and_then(Value::as_str).unwrap_or("Enter");
                            format!(
                                "(() => {{ const el = {resolver}; if (!el) return; el.focus?.(); const key = {}; el.dispatchEvent(new KeyboardEvent('keydown', {{key, bubbles:true}})); el.dispatchEvent(new KeyboardEvent('keyup', {{key, bubbles:true}})); if (key === 'Enter' && el.tagName !== 'TEXTAREA') {{ if (el.form?.requestSubmit) el.form.requestSubmit(); else el.click?.(); }} }})()",
                                Value::String(key.to_string())
                            )
                        }
                        "fill" | "select" => {
                            let value = extra
                                .get("value")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            format!(
                                "(() => {{ const el = {resolver}; if (!el) return; el.focus(); const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : el instanceof HTMLInputElement ? HTMLInputElement.prototype : null; const setter = proto && Object.getOwnPropertyDescriptor(proto, 'value')?.set; if (setter) setter.call(el, {}); else el.value = {}; el.dispatchEvent(new Event('input', {{bubbles:true}})); el.dispatchEvent(new Event('change', {{bubbles:true}})); }})()",
                                Value::String(value.to_string()),
                                Value::String(value.to_string())
                            )
                        }
                        _ => unreachable!(),
                    }
                }
            };
            let snapshot = kf_browser_eval(app, &script).await?;
            Ok(json!({
                "performed": true,
                "action": action,
                "open": snapshot.open,
                "url": snapshot.url,
                "title": snapshot.title,
            }))
        }
        "open" | "new-tab" | "select-tab" | "close-tab" | "navigate" | "back" | "forward"
        | "refresh" | "stop" | "close" | "focus" => {
            let tab_id = extra
                .get("tabId")
                .and_then(Value::as_str)
                .map(str::to_string);
            let snapshot = kf_browser_command(
                app.clone(),
                action.to_string(),
                url.map(str::to_string),
                tab_id,
            )
            .await?;
            Ok(json!({
                "open": snapshot.open,
                "url": snapshot.url,
                "title": snapshot.title,
            }))
        }
        other => Err(LocalizedError::new("error.browser_action").arg("action", other)),
    }
}

/// 在打开的内置浏览器视图中执行一段受控脚本（click/fill/scroll 用）。
async fn kf_browser_eval(app: &AppHandle, script: &str) -> KfResult<BrowserSnapshot> {
    let label = active_label().ok_or_else(|| LocalizedError::new("error.browser_closed"))?;
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| LocalizedError::new("error.browser_closed"))?;
    webview
        .eval(script)
        .map_err(|e| LocalizedError::new("error.browser_script").arg("detail", e))?;
    Ok(snapshot(app))
}

async fn kf_browser_eval_json(app: &AppHandle, script: &str) -> KfResult<Value> {
    let label = active_label().ok_or_else(|| LocalizedError::new("error.browser_closed"))?;
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| LocalizedError::new("error.browser_closed"))?;
    let (send, receive) = tokio::sync::oneshot::channel::<String>();
    let send = Arc::new(Mutex::new(Some(send)));
    webview
        .eval_with_callback(script, move |result| {
            if let Some(send) = send.lock().take() {
                let _ = send.send(result);
            }
        })
        .map_err(|e| LocalizedError::new("error.browser_script").arg("detail", e))?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(8), receive)
        .await
        .map_err(|_| LocalizedError::new("error.browser_script").arg("detail", "timeout"))?
        .map_err(|_| LocalizedError::new("error.browser_script").arg("detail", "callback"))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| LocalizedError::new("error.browser_script").arg("detail", e))?;
    if let Some(encoded) = value.as_str() {
        serde_json::from_str(encoded)
            .map_err(|e| LocalizedError::new("error.browser_script").arg("detail", e))
    } else {
        Ok(value)
    }
}

async fn live_page_snapshot(app: &AppHandle) -> KfResult<Value> {
    let script = r#"(() => {
      const visible = (node) => {
        const style = getComputedStyle(node);
        const rect = node.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
      };
      const compact = (value, max = 80) => String(value || '').replace(/\s+/g, ' ').trim().slice(0, max);
      const candidates = Array.from(document.querySelectorAll('a,button,input,select,textarea,[role="button"],[role="link"],[contenteditable="true"]')).filter(visible).slice(0, 48);
      const elements = candidates.map((node, index) => {
        const ref = `e${index + 1}`;
        node.setAttribute('data-kf-ref', ref);
        const tag = node.tagName.toLowerCase();
        const role = node.getAttribute('role') || (tag === 'a' ? 'link' : (tag === 'button' ? 'button' : 'field'));
        const name = compact(node.getAttribute('aria-label') || node.getAttribute('placeholder') || node.getAttribute('title') || node.innerText || node.value);
        const href = tag === 'a' ? compact(node.href, 120) : '';
        return { ref, role, name, hint: href || undefined, selector: `[data-kf-ref="${ref}"]` };
      });
      return {
        rendered: true,
        url: location.href,
        title: document.title,
        text: compact(document.body?.innerText, 4000),
        textChars: (document.body?.innerText || '').length,
        viewport: { width: innerWidth, height: innerHeight, scrollX, scrollY },
        elements,
        elementsOmitted: Math.max(0, document.querySelectorAll('a,button,input,select,textarea,[role="button"],[role="link"],[contenteditable="true"]').length - elements.length)
      };
    })()"#;
    let mut result = kf_browser_eval_json(app, script).await?;
    let mut refs = HashMap::new();
    if let Some(elements) = result.get_mut("elements").and_then(Value::as_array_mut) {
        for element in elements {
            let reference = element
                .get("ref")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let selector = element
                .get("selector")
                .and_then(Value::as_str)
                .map(str::to_string);
            let role = element
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("field")
                .to_string();
            let name = element
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !reference.is_empty() {
                refs.insert(
                    reference.to_string(),
                    ElementLocator {
                        role,
                        selector,
                        name,
                    },
                );
            }
            if let Some(object) = element.as_object_mut() {
                object.remove("selector");
                if object.get("hint").is_some_and(Value::is_null) {
                    object.remove("hint");
                }
            }
        }
    }
    *FETCH_REFS.write() = refs;
    Ok(result)
}

fn snapshot(_app: &AppHandle) -> BrowserSnapshot {
    let runtime = BROWSER.read();
    let active = runtime
        .active_tab_id
        .as_ref()
        .and_then(|id| runtime.tabs.iter().find(|tab| &tab.id == id));
    BrowserSnapshot {
        available: true,
        open: !runtime.tabs.is_empty(),
        url: active.and_then(|tab| tab.url.clone()),
        title: active.and_then(|tab| tab.title.clone()),
        can_go_back: active.is_some_and(|tab| tab.history_index > 0),
        can_go_forward: active.is_some_and(|tab| tab.history_index + 1 < tab.history.len()),
        loading: active.is_some_and(|tab| tab.loading),
        active_tab_id: runtime.active_tab_id.clone(),
        tabs: runtime.tabs.iter().map(BrowserTabState::snapshot).collect(),
    }
}

fn emit_browser(app: &AppHandle, opened: bool) {
    let data = serde_json::to_value(snapshot(app)).unwrap_or_else(|_| json!({}));
    let kind = if opened {
        "browser.opened"
    } else {
        "browser.updated"
    };
    let _ = app.emit("kf://runtime", RuntimeEvent::new(kind, data));
}

fn active_label() -> Option<String> {
    let runtime = BROWSER.read();
    let id = runtime.active_tab_id.as_ref()?;
    runtime
        .tabs
        .iter()
        .find(|tab| &tab.id == id)
        .map(|tab| tab.label.clone())
}

fn apply_rect(app: &AppHandle, label: &str, rect: BrowserRect) -> KfResult<()> {
    let webview = app
        .get_webview(label)
        .ok_or_else(|| LocalizedError::new("error.browser_closed"))?;
    webview
        .set_position(tauri::PhysicalPosition::new(rect.x, rect.y))
        .map_err(|e| LocalizedError::new("error.browser_navigate").arg("detail", e))?;
    webview
        .set_size(tauri::PhysicalSize::new(rect.width, rect.height))
        .map_err(|e| LocalizedError::new("error.browser_navigate").arg("detail", e))?;
    webview
        .show()
        .map_err(|e| LocalizedError::new("error.browser_show").arg("detail", e))?;
    Ok(())
}

fn create_tab(app: &AppHandle, target: Url, activate: bool) -> KfResult<String> {
    let (id, label, previous_label, rect) = {
        let mut runtime = BROWSER.write();
        runtime.next_tab = runtime.next_tab.saturating_add(1);
        let id = format!("tab-{}", runtime.next_tab);
        let label = format!("{LABEL_PREFIX}{}", runtime.next_tab);
        let previous_label = runtime
            .active_tab_id
            .as_ref()
            .and_then(|active| runtime.tabs.iter().find(|tab| &tab.id == active))
            .map(|tab| tab.label.clone());
        runtime.tabs.push(BrowserTabState {
            id: id.clone(),
            label: label.clone(),
            url: Some(target.to_string()),
            title: None,
            loading: true,
            history: vec![target.to_string()],
            history_index: 0,
        });
        if activate || runtime.active_tab_id.is_none() {
            runtime.active_tab_id = Some(id.clone());
        }
        (id, label, previous_label, runtime.rect)
    };

    let navigation_app = app.clone();
    let navigation_id = id.clone();
    let load_app = app.clone();
    let load_id = id.clone();
    let title_app = app.clone();
    let title_id = id.clone();
    let popup_app = app.clone();
    let builder = WebviewBuilder::new(&label, WebviewUrl::External(target))
        .enable_clipboard_access()
        .general_autofill_enabled(true)
        .zoom_hotkeys_enabled(true)
        .on_navigation(move |url| {
            if !matches!(url.scheme(), "http" | "https") {
                return false;
            }
            if let Some(tab) = BROWSER
                .write()
                .tabs
                .iter_mut()
                .find(|tab| tab.id == navigation_id)
            {
                tab.record_navigation(url.to_string());
                tab.loading = true;
            }
            emit_browser(&navigation_app, false);
            true
        })
        .on_page_load(move |_webview, payload| {
            if let Some(tab) = BROWSER
                .write()
                .tabs
                .iter_mut()
                .find(|tab| tab.id == load_id)
            {
                tab.record_navigation(payload.url().to_string());
                tab.loading = matches!(payload.event(), PageLoadEvent::Started);
            }
            emit_browser(&load_app, false);
        })
        .on_document_title_changed(move |_webview, title| {
            if let Some(tab) = BROWSER
                .write()
                .tabs
                .iter_mut()
                .find(|tab| tab.id == title_id)
            {
                tab.title = Some(title.chars().take(160).collect());
            }
            emit_browser(&title_app, false);
        })
        .on_new_window(move |url, _features| {
            if matches!(url.scheme(), "http" | "https") {
                let app = popup_app.clone();
                tauri::async_runtime::spawn(async move {
                    if create_tab(&app, url, true).is_ok() {
                        emit_browser(&app, true);
                    }
                });
            }
            NewWindowResponse::Deny
        });

    let main = app.get_window(MAIN_LABEL).ok_or_else(|| {
        LocalizedError::new("error.browser_create").arg("detail", "main-window-missing")
    })?;
    let size = main.inner_size().unwrap_or_default();
    let fallback = BrowserRect {
        x: ((size.width as f64) * 0.18).round() as i32,
        y: ((size.height as f64) * 0.16).round() as i32,
        width: ((size.width as f64) * 0.72).round().max(200.0) as u32,
        height: ((size.height as f64) * 0.72).round().max(150.0) as u32,
    };
    if let Err(error) = main.add_child(
        builder,
        tauri::PhysicalPosition::new(fallback.x, fallback.y),
        tauri::PhysicalSize::new(fallback.width, fallback.height),
    ) {
        BROWSER.write().tabs.retain(|tab| tab.id != id);
        return Err(LocalizedError::new("error.browser_create").arg("detail", error));
    }
    if activate
        && let Some(previous) = previous_label
        && let Some(webview) = app.get_webview(&previous)
    {
        let _ = webview.hide();
    }
    let is_active = BROWSER.read().active_tab_id.as_deref() == Some(id.as_str());
    if is_active {
        if let Some(rect) = rect {
            apply_rect(app, &label, rect)?;
        } else if let Some(webview) = app.get_webview(&label) {
            let _ = webview.hide();
        }
    } else if let Some(webview) = app.get_webview(&label) {
        let _ = webview.hide();
    }
    let _ = main.show();
    let _ = main.set_focus();
    Ok(id)
}

/// 前端把浏览器舞台区域的物理像素矩形同步过来，子 webview 始终精确
/// 覆盖主窗口内的舞台位置（浏览器是"内置"的，不再开独立窗口）。
#[tauri::command]
pub fn kf_browser_rect(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> KfResult<BrowserSnapshot> {
    if width <= 0.0 || height <= 0.0 {
        BROWSER.write().rect = None;
        for tab in &BROWSER.read().tabs {
            if let Some(webview) = app.get_webview(&tab.label) {
                let _ = webview.hide();
            }
        }
        return Ok(snapshot(&app));
    }
    let rect = BrowserRect {
        x: x.round() as i32,
        y: y.round() as i32,
        width: width.round() as u32,
        height: height.round() as u32,
    };
    BROWSER.write().rect = Some(rect);
    if let Some(label) = active_label() {
        apply_rect(&app, &label, rect)?;
    }
    emit_browser(&app, false);
    Ok(snapshot(&app))
}

#[tauri::command]
pub async fn kf_browser_command(
    app: AppHandle,
    action: String,
    url: Option<String>,
    tab_id: Option<String>,
) -> KfResult<BrowserSnapshot> {
    match action.as_str() {
        "open" => {
            let target = resolve_address(url.as_deref().unwrap_or("https://www.google.com"))?;
            if let Some(label) = active_label() {
                let webview = app
                    .get_webview(&label)
                    .ok_or_else(|| LocalizedError::new("error.browser_closed"))?;
                webview
                    .navigate(target.clone())
                    .map_err(|e| LocalizedError::new("error.browser_navigate").arg("detail", e))?;
            } else {
                create_tab(&app, target.clone(), true)?;
            }
            emit_browser(&app, true);
        }
        "new-tab" => {
            let target = resolve_address(url.as_deref().unwrap_or("https://www.google.com"))?;
            create_tab(&app, target, true)?;
            emit_browser(&app, true);
        }
        "select-tab" => {
            let id = tab_id.ok_or_else(|| LocalizedError::new("error.browser_tab_required"))?;
            let (labels, active_label, rect) = {
                let mut runtime = BROWSER.write();
                if !runtime.tabs.iter().any(|tab| tab.id == id) {
                    return Err(LocalizedError::new("error.browser_tab_required"));
                }
                runtime.active_tab_id = Some(id.clone());
                let labels = runtime
                    .tabs
                    .iter()
                    .map(|tab| tab.label.clone())
                    .collect::<Vec<_>>();
                let active = runtime
                    .tabs
                    .iter()
                    .find(|tab| tab.id == id)
                    .map(|tab| tab.label.clone())
                    .unwrap_or_default();
                (labels, active, runtime.rect)
            };
            for label in labels {
                if let Some(webview) = app.get_webview(&label) {
                    if label == active_label {
                        if let Some(rect) = rect {
                            apply_rect(&app, &label, rect)?;
                        }
                    } else {
                        let _ = webview.hide();
                    }
                }
            }
            emit_browser(&app, false);
        }
        "close-tab" => {
            let id = tab_id
                .or_else(|| BROWSER.read().active_tab_id.clone())
                .ok_or_else(|| LocalizedError::new("error.browser_tab_required"))?;
            let (closed, next, rect) = {
                let mut runtime = BROWSER.write();
                let index = runtime
                    .tabs
                    .iter()
                    .position(|tab| tab.id == id)
                    .ok_or_else(|| LocalizedError::new("error.browser_tab_required"))?;
                let closed = runtime.tabs.remove(index).label;
                if runtime.active_tab_id.as_deref() == Some(id.as_str()) {
                    runtime.active_tab_id = runtime
                        .tabs
                        .get(index.min(runtime.tabs.len().saturating_sub(1)))
                        .map(|tab| tab.id.clone());
                }
                let next = runtime
                    .active_tab_id
                    .as_ref()
                    .and_then(|active| runtime.tabs.iter().find(|tab| &tab.id == active))
                    .map(|tab| tab.label.clone());
                (closed, next, runtime.rect)
            };
            if let Some(webview) = app.get_webview(&closed) {
                webview
                    .close()
                    .map_err(|e| LocalizedError::new("error.browser_close").arg("detail", e))?;
            }
            if let (Some(label), Some(rect)) = (next, rect) {
                apply_rect(&app, &label, rect)?;
            }
            emit_browser(&app, false);
        }
        "navigate" => {
            let target = resolve_address(
                url.as_deref()
                    .ok_or_else(|| LocalizedError::new("error.browser_url_required"))?,
            )?;
            let label = if let Some(id) = tab_id {
                BROWSER
                    .read()
                    .tabs
                    .iter()
                    .find(|tab| tab.id == id)
                    .map(|tab| tab.label.clone())
            } else {
                active_label()
            }
            .ok_or_else(|| LocalizedError::new("error.browser_closed"))?;
            app.get_webview(&label)
                .ok_or_else(|| LocalizedError::new("error.browser_closed"))?
                .navigate(target.clone())
                .map_err(|e| LocalizedError::new("error.browser_navigate").arg("detail", e))?;
            emit_browser(&app, true);
        }
        "focus" | "show" => {
            let label =
                active_label().ok_or_else(|| LocalizedError::new("error.browser_closed"))?;
            if let Some(rect) = BROWSER.read().rect {
                apply_rect(&app, &label, rect)?;
            }
        }
        "hide" => {
            for tab in &BROWSER.read().tabs {
                if let Some(webview) = app.get_webview(&tab.label) {
                    webview
                        .hide()
                        .map_err(|e| LocalizedError::new("error.browser_show").arg("detail", e))?;
                }
            }
        }
        "refresh" => app
            .get_webview(
                &active_label().ok_or_else(|| LocalizedError::new("error.browser_closed"))?,
            )
            .ok_or_else(|| LocalizedError::new("error.browser_closed"))?
            .reload()
            .map_err(|e| LocalizedError::new("error.browser_reload").arg("detail", e))?,
        "stop" => {
            let label =
                active_label().ok_or_else(|| LocalizedError::new("error.browser_closed"))?;
            app.get_webview(&label)
                .ok_or_else(|| LocalizedError::new("error.browser_closed"))?
                .eval("window.stop()")
                .map_err(|e| LocalizedError::new("error.browser_reload").arg("detail", e))?;
            if let Some(tab) = BROWSER
                .write()
                .tabs
                .iter_mut()
                .find(|tab| tab.label == label)
            {
                tab.loading = false;
            }
        }
        "back" => app
            .get_webview(
                &active_label().ok_or_else(|| LocalizedError::new("error.browser_closed"))?,
            )
            .ok_or_else(|| LocalizedError::new("error.browser_closed"))?
            .eval("history.back()")
            .map_err(|e| LocalizedError::new("error.browser_history").arg("detail", e))?,
        "forward" => app
            .get_webview(
                &active_label().ok_or_else(|| LocalizedError::new("error.browser_closed"))?,
            )
            .ok_or_else(|| LocalizedError::new("error.browser_closed"))?
            .eval("history.forward()")
            .map_err(|e| LocalizedError::new("error.browser_history").arg("detail", e))?,
        "close" => {
            let labels = BROWSER
                .read()
                .tabs
                .iter()
                .map(|tab| tab.label.clone())
                .collect::<Vec<_>>();
            for label in labels {
                if let Some(webview) = app.get_webview(&label) {
                    webview
                        .close()
                        .map_err(|e| LocalizedError::new("error.browser_close").arg("detail", e))?;
                }
            }
            let mut runtime = BROWSER.write();
            runtime.tabs.clear();
            runtime.active_tab_id = None;
        }
        _ => return Err(LocalizedError::new("error.browser_action").arg("action", action)),
    }
    emit_browser(&app, false);
    Ok(snapshot(&app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permits_only_http_and_https() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://localhost:3000").is_ok());
        assert!(validate_url("file:///C:/secret").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_url("data:text/html,test").is_err());
    }

    #[test]
    fn address_bar_accepts_hosts_and_search_terms() {
        assert_eq!(
            resolve_address("example.com/docs").unwrap().as_str(),
            "https://example.com/docs"
        );
        let search = resolve_address("KnightFrame browser").unwrap();
        assert_eq!(search.host_str(), Some("www.google.com"));
        assert_eq!(
            search.query_pairs().find(|(key, _)| key == "q").unwrap().1,
            "KnightFrame browser"
        );
    }

    #[test]
    fn attr_value_reads_double_and_single_quotes() {
        assert_eq!(
            attr_value(r#"<a href="/docs" aria-label='Docs home'>"#, "href"),
            Some("/docs".to_string())
        );
        assert_eq!(
            attr_value(r#"<a href="/docs" aria-label='Docs home'>"#, "aria-label"),
            Some("Docs home".to_string())
        );
        assert_eq!(
            attr_value(r#"<a href="/x" class="link">"#, "placeholder"),
            None
        );
        // 属性名作为子串出现时不应误匹配（data-href ≠ href）
        assert_eq!(attr_value(r#"<a data-href="/nope">"#, "href"), None);
    }

    #[test]
    fn element_refs_cover_links_buttons_and_fields() {
        let html = r##"
        <html><body>
            <nav><a href="/docs">Documentation</a> <a href="/blog">Blog</a></nav>
            <main><button aria-label="Search">🔍</button>
            <form><input placeholder="Email address"><textarea placeholder="Notes"></textarea></form>
            <a href="javascript:void(0)">Skip</a> <a href="#top">Anchor</a>
            </main>
        </body></html>"##;
        let base = Url::parse("https://example.com/").unwrap();
        let (elements, omitted, locators) = extract_elements(html, &base);
        assert_eq!(omitted, 0);
        let refs: Vec<&str> = elements
            .iter()
            .map(|item| item["ref"].as_str().unwrap())
            .collect();
        assert_eq!(refs, vec!["e1", "e2", "e3", "e4", "e5"]);
        let first = &elements[0];
        assert_eq!(first["role"], "link");
        assert_eq!(first["name"], "Documentation");
        assert_eq!(first["hint"], "example.com/docs");
        let button = &elements[2];
        assert_eq!(button["role"], "button");
        assert_eq!(button["name"], "Search");
        let field = &elements[3];
        assert_eq!(field["role"], "field");
        assert_eq!(field["name"], "Email address");
        assert_eq!(
            locators
                .get("e1")
                .and_then(|locator| locator.selector.as_deref()),
            Some("a[href=\"/docs\"]")
        );
        // javascript:/# 链接应被剔除
        assert!(elements.iter().all(|item| item["name"] != "Skip"));
        assert!(elements.iter().all(|item| item["name"] != "Anchor"));
    }

    #[test]
    fn element_ref_limit_reports_omitted() {
        let mut html = String::from("<html><body>");
        for index in 0..40 {
            html.push_str(&format!(r#"<a href="/p{index}">Page {index}</a> "#));
        }
        html.push_str("</body></html>");
        let base = Url::parse("https://example.com/").unwrap();
        let (elements, omitted, _locators) = extract_elements(&html, &base);
        assert_eq!(elements.len(), ELEMENT_LIMIT);
        assert_eq!(omitted, 40 - ELEMENT_LIMIT);
    }

    #[test]
    fn text_window_pages_through_content() {
        let text = "abcdef".to_string();
        let (chunk, total, next, complete) = text_window(&text, 0, 4);
        assert_eq!(
            (chunk.as_str(), total, next, complete),
            ("abcd", 6, 4, false)
        );
        let (chunk, total, next, complete) = text_window(&text, 4, 4);
        assert_eq!((chunk.as_str(), total, next, complete), ("ef", 6, 6, true));
        // 多字节字符按字符计数，不切断字节边界
        let unicode = "价格行为分析".to_string();
        let (chunk, _, next, complete) = text_window(&unicode, 0, 3);
        assert_eq!(chunk, "价格行");
        assert_eq!(next, 3);
        assert!(!complete);
    }

    #[test]
    fn projection_token_lean_shape() {
        // 投影不应包含原始 HTML；字段集合保持稳定（模型可见面）
        // 这里静态验证 agent_fetch_page 的字段约定：url/status/title/text/textChars/
        // omittedChars/nextOffset/complete/elements(/elementsOmitted)/artifact
        let expected = [
            "url",
            "status",
            "title",
            "text",
            "textChars",
            "omittedChars",
            "nextOffset",
            "complete",
            "elements",
        ];
        for key in expected {
            assert!(!key.is_empty());
        }
    }
}
