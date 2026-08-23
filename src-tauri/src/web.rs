use crate::{
    browser,
    error::{KfResult, LocalizedError},
};
use serde_json::{Value, json};
use std::{
    sync::{LazyLock, RwLock},
    time::{Duration, Instant},
};
use url::Url;

const PROFILE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchEngine {
    Bing,
    Baidu,
    Google,
    DuckDuckGo,
}

impl SearchEngine {
    fn id(self) -> &'static str {
        match self {
            Self::Bing => "bing",
            Self::Baidu => "baidu",
            Self::Google => "google",
            Self::DuckDuckGo => "duckduckgo",
        }
    }

    fn url(self, query: &str) -> KfResult<Url> {
        let (base, parameter) = match self {
            Self::Bing => ("https://www.bing.com/search", "q"),
            Self::Baidu => ("https://www.baidu.com/s", "wd"),
            Self::Google => ("https://www.google.com/search", "q"),
            Self::DuckDuckGo => ("https://html.duckduckgo.com/html/", "q"),
        };
        Url::parse_with_params(base, [(parameter, query)])
            .map_err(|error| LocalizedError::new("error.web_search_url").arg("detail", error))
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct NetworkProfile {
    google: bool,
    bing: bool,
    baidu: bool,
}

static PROFILE: LazyLock<RwLock<Option<(Instant, NetworkProfile)>>> =
    LazyLock::new(|| RwLock::new(None));

async fn reachable(client: &reqwest::Client, url: &str) -> bool {
    client
        .get(url)
        .timeout(Duration::from_millis(1800))
        .send()
        .await
        .is_ok_and(|response| response.status().is_success() || response.status().is_redirection())
}

async fn network_profile(client: &reqwest::Client) -> NetworkProfile {
    if let Some((checked, profile)) = *PROFILE.read().expect("network profile lock")
        && checked.elapsed() < PROFILE_TTL
    {
        return profile;
    }
    let (google, bing, baidu) = tokio::join!(
        reachable(client, "https://www.google.com/generate_204"),
        reachable(client, "https://www.bing.com/favicon.ico"),
        reachable(client, "https://www.baidu.com/favicon.ico"),
    );
    let profile = NetworkProfile {
        google,
        bing,
        baidu,
    };
    *PROFILE.write().expect("network profile lock") = Some((Instant::now(), profile));
    profile
}

fn parse_engine(value: Option<&str>) -> Option<SearchEngine> {
    match value.unwrap_or("auto").to_ascii_lowercase().as_str() {
        "bing" => Some(SearchEngine::Bing),
        "baidu" => Some(SearchEngine::Baidu),
        "google" => Some(SearchEngine::Google),
        "duckduckgo" | "duck" | "ddg" => Some(SearchEngine::DuckDuckGo),
        _ => None,
    }
}

fn available(engine: SearchEngine, profile: NetworkProfile) -> bool {
    match engine {
        SearchEngine::Bing => profile.bing,
        SearchEngine::Baidu => profile.baidu,
        SearchEngine::Google => profile.google,
        SearchEngine::DuckDuckGo => true,
    }
}

fn route(profile: NetworkProfile, requested: Option<SearchEngine>) -> Vec<SearchEngine> {
    let mut engines = Vec::new();
    if let Some(requested) = requested
        && available(requested, profile)
    {
        engines.push(requested);
    }
    let defaults = if !profile.google && profile.baidu {
        [
            SearchEngine::Baidu,
            SearchEngine::Bing,
            SearchEngine::DuckDuckGo,
            SearchEngine::Google,
        ]
    } else {
        [
            SearchEngine::Bing,
            SearchEngine::Google,
            SearchEngine::DuckDuckGo,
            SearchEngine::Baidu,
        ]
    };
    for engine in defaults {
        if available(engine, profile) && !engines.contains(&engine) {
            engines.push(engine);
        }
    }
    engines
}

fn usable(result: &Value) -> bool {
    let status = result.get("status").and_then(Value::as_u64).unwrap_or(500);
    let text = result.get("text").and_then(Value::as_str).unwrap_or("");
    status < 400 && text.chars().count() >= 80
}

/// Search without opening the embedded browser. Google is only considered
/// after a successful local connectivity probe; mainland-style networks route
/// to Baidu first and retain Bing/DDG fallbacks.
pub async fn search(
    client: &reqwest::Client,
    query: &str,
    requested: Option<&str>,
    offset: usize,
) -> KfResult<Value> {
    let query = query.trim();
    if query.is_empty() {
        return Err(LocalizedError::new("error.tool_argument").arg("field", "query"));
    }
    let profile = network_profile(client).await;
    let requested = parse_engine(requested);
    let mut last_error = None;
    for engine in route(profile, requested) {
        let url = engine.url(query)?;
        match browser::agent_fetch_page(client, url.as_str(), offset, 8000).await {
            Ok(mut result) if usable(&result) => {
                result["action"] = json!("search");
                result["engine"] = json!(engine.id());
                result["query"] = json!(query);
                result["googleAvailable"] = json!(profile.google);
                return Ok(result);
            }
            Ok(result) => {
                last_error = Some(format!(
                    "{} status={} empty={}",
                    engine.id(),
                    result.get("status").and_then(Value::as_u64).unwrap_or(0),
                    result
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .is_empty()
                ));
            }
            Err(error) => last_error = Some(format!("{}: {}", engine.id(), error)),
        }
    }
    Err(LocalizedError::new("error.web_search").arg(
        "detail",
        last_error.unwrap_or_else(|| "no reachable search service".into()),
    ))
}

pub async fn fetch(client: &reqwest::Client, url: &str, offset: usize) -> KfResult<Value> {
    browser::agent_fetch_page(client, url, offset, 8000).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_google_is_never_selected() {
        let engines = route(
            NetworkProfile {
                google: false,
                bing: true,
                baidu: true,
            },
            Some(SearchEngine::Google),
        );
        assert_eq!(engines[0], SearchEngine::Baidu);
        assert!(!engines.contains(&SearchEngine::Google));
    }

    #[test]
    fn global_network_prefers_bing_and_keeps_fallbacks() {
        let engines = route(
            NetworkProfile {
                google: true,
                bing: true,
                baidu: false,
            },
            None,
        );
        assert_eq!(engines[0], SearchEngine::Bing);
        assert!(engines.contains(&SearchEngine::Google));
        assert!(engines.contains(&SearchEngine::DuckDuckGo));
    }
}
