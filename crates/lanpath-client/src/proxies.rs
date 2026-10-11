//! Proxy group / node views and Clash API operations.
//!
//! Provides [`clash_get_proxies`], [`clash_select_proxy`], [`clash_test_delay`] and
//! [`replay_group_selections`] for runtime group selection persistence.

use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// View types
// ---------------------------------------------------------------------------

/// Proxy group view (Selector / URLTest / Fallback / LoadBalance …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupView {
    /// Group name (matches Clash API `name`).
    pub name: String,
    /// Group type, e.g. `Selector`, `URLTest`.
    pub group_type: String,
    /// Currently selected member name (for Selector-like groups).
    pub now: String,
    /// Member node names.
    pub members: Vec<String>,
}

/// Proxy node view (individual proxy / relay).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeView {
    /// Node name.
    pub name: String,
    /// Node type, e.g. `Shadowsocks`, `Vmess`, `Trojan`, `Direct` …
    pub node_type: String,
    /// Last measured delay in milliseconds (`None` = untested or timeout).
    pub delay_ms: Option<u16>,
    /// Whether UDP is supported (parsed from `udp` boolean in Clash API).
    pub udp: bool,
}

/// Unified proxy list response: groups and flat nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyList {
    /// All selectable groups.
    pub groups: Vec<GroupView>,
    /// All individual nodes (excluding built-in pseudo-nodes such as DIRECT / REJECT).
    pub nodes: Vec<NodeView>,
}

// ---------------------------------------------------------------------------
// Clash API client helpers
// ---------------------------------------------------------------------------

/// Default delay-test URL used by [`clash_test_delay`] when caller passes `None`.
const DEFAULT_DELAY_TEST_URL: &str = "https://www.gstatic.com/generate_204";

/// Build a `reqwest` client for Clash API: short timeout, no proxy, direct loopback.
fn build_clash_client(timeout_ms: u64) -> LanpathResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .no_proxy()
        .build()
        .map_err(|e| LanpathError::Client(format!("build Clash API client failed: {e}")))
}

/// Attach Bearer auth when `secret` is non-empty.
fn auth_request(request: reqwest::RequestBuilder, secret: &str) -> reqwest::RequestBuilder {
    if secret.is_empty() {
        request
    } else {
        request.bearer_auth(secret)
    }
}

/// Parse the raw `/proxies` JSON into [`ProxyList`].
///
/// Filtering rules:
/// - `GLOBAL` group is skipped.
/// - Built-in pseudo-nodes (`DIRECT`, `REJECT`, `REJECT-DROP`, `PASS`, `COMPATIBLE`, `BLOCK`, `NOAP`)
///   are excluded from the flat `nodes` list.
/// - Any proxy whose `type` equals `Direct`, `Reject`, `Pass`, `Compatible`, `Block`, `NoAP` is
///   also treated as a pseudo-node and filtered out.
fn parse_proxies_response(body: &serde_json::Value) -> LanpathResult<ProxyList> {
    let proxies = body
        .get("proxies")
        .and_then(|p| p.as_object())
        .ok_or_else(|| {
            LanpathError::Client("Clash API /proxies missing 'proxies' object".into())
        })?;

    let mut groups = Vec::new();
    let mut nodes = Vec::new();

    for (name, value) in proxies {
        if name == "GLOBAL" {
            continue;
        }
        let Some(obj) = value.as_object() else {
            continue;
        };

        let proxy_type = obj
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        // Group detection: has `all` array.
        if let Some(all) = obj.get("all").and_then(|a| a.as_array()) {
            let now = obj
                .get("now")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let members: Vec<String> = all
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            groups.push(GroupView {
                name: name.clone(),
                group_type: proxy_type.clone(),
                now,
                members,
            });
        }

        // Node: skip built-in pseudo-nodes.
        if is_pseudo_node(name, &proxy_type) {
            continue;
        }

        let delay_ms = obj
            .get("history")
            .and_then(|h| h.as_array())
            .and_then(|arr| arr.last())
            .and_then(|last| last.get("delay"))
            .and_then(|d| d.as_u64())
            .and_then(|d| u16::try_from(d).ok());

        let udp = obj.get("udp").and_then(|u| u.as_bool()).unwrap_or(false);

        nodes.push(NodeView {
            name: name.clone(),
            node_type: proxy_type,
            delay_ms,
            udp,
        });
    }

    Ok(ProxyList { groups, nodes })
}

/// Built-in pseudo-node names (uppercase) that should not appear in the node list.
const PSEUDO_NODE_NAMES: &[&str] = &[
    "DIRECT",
    "REJECT",
    "REJECT-DROP",
    "PASS",
    "COMPATIBLE",
    "BLOCK",
    "NOAP",
];

/// Built-in pseudo-node types (case-insensitive) that should not appear in the node list.
const PSEUDO_NODE_TYPES: &[&str] = &["Direct", "Reject", "Pass", "Compatible", "Block", "NoAP"];

fn is_pseudo_node(name: &str, node_type: &str) -> bool {
    if PSEUDO_NODE_NAMES
        .iter()
        .any(|&n| n.eq_ignore_ascii_case(name))
    {
        return true;
    }
    if PSEUDO_NODE_TYPES
        .iter()
        .any(|&t| t.eq_ignore_ascii_case(node_type))
    {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Public Clash API operations
// ---------------------------------------------------------------------------

/// Fetch proxy list from Clash API `GET /proxies`.
///
/// Returns [`LanpathError::Client`] with "core not running" semantics when the
/// connection is refused / unreachable.
pub async fn clash_get_proxies(port: u16, secret: &str) -> LanpathResult<ProxyList> {
    let client = build_clash_client(5000)?;
    let request = auth_request(
        client.get(format!("http://127.0.0.1:{port}/proxies")),
        secret,
    );

    let resp = match request.send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() || e.is_timeout() || is_connection_refused(&e) => {
            return Err(LanpathError::Client(format!("core not running: {e}")));
        }
        Err(e) => {
            return Err(LanpathError::Client(format!(
                "Clash API request failed: {e}"
            )));
        }
    };

    if !resp.status().is_success() {
        return Err(LanpathError::Client(format!(
            "Clash API /proxies returned HTTP {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| LanpathError::Client(format!("Clash API /proxies invalid JSON: {e}")))?;

    parse_proxies_response(&body)
}

/// Heuristic: detect connection-refused from reqwest error.
fn is_connection_refused(e: &reqwest::Error) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("connection refused") || msg.contains("refused") || msg.contains("os error 111")
}

/// Select a proxy in a group via Clash API `PUT /proxies/{group}`.
///
/// Body: `{"name": "<name>"}`.
pub async fn clash_select_proxy(
    port: u16,
    secret: &str,
    group: &str,
    name: &str,
) -> LanpathResult<()> {
    let client = build_clash_client(5000)?;
    let request = auth_request(
        client
            .put(format!("http://127.0.0.1:{port}/proxies/{group}"))
            .json(&serde_json::json!({ "name": name })),
        secret,
    );

    let resp = match request.send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() || e.is_timeout() || is_connection_refused(&e) => {
            return Err(LanpathError::Client(format!("core not running: {e}")));
        }
        Err(e) => {
            return Err(LanpathError::Client(format!(
                "Clash API request failed: {e}"
            )));
        }
    };

    if !resp.status().is_success() {
        return Err(LanpathError::Client(format!(
            "Clash API PUT /proxies/{group} returned HTTP {}",
            resp.status()
        )));
    }
    Ok(())
}

/// Test delay of a single proxy via Clash API `GET /proxies/{name}/delay`.
///
/// `url` defaults to [`DEFAULT_DELAY_TEST_URL`]; `timeout_ms` is the query
/// parameter sent to Clash API (also used as the HTTP client timeout).
/// Returns `Ok(None)` when the test fails or times out.
pub async fn clash_test_delay(
    port: u16,
    secret: &str,
    name: &str,
    url: Option<&str>,
    timeout_ms: u64,
) -> LanpathResult<Option<u16>> {
    let test_url = url.unwrap_or(DEFAULT_DELAY_TEST_URL);
    let client = build_clash_client(timeout_ms + 1000)?;
    let request = auth_request(
        client.get(format!(
            "http://127.0.0.1:{port}/proxies/{name}/delay?url={}&timeout={}",
            urlencoding::encode(test_url),
            timeout_ms
        )),
        secret,
    );

    let resp = match request.send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() || e.is_timeout() || is_connection_refused(&e) => {
            return Err(LanpathError::Client(format!("core not running: {e}")));
        }
        Err(e) => {
            tracing::debug!(error = %e, name, "proxy delay test request failed");
            return Ok(None);
        }
    };

    if !resp.status().is_success() {
        tracing::debug!(status = %resp.status(), name, "proxy delay test returned non-2xx");
        return Ok(None);
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(error = %e, name, "proxy delay test invalid JSON");
            return Ok(None);
        }
    };

    let delay = body
        .get("delay")
        .and_then(|d| d.as_u64())
        .and_then(|d| u16::try_from(d).ok());

    Ok(delay)
}

// ---------------------------------------------------------------------------
// Selection persistence helpers
// ---------------------------------------------------------------------------

/// Persist a group selection to [`ClientConfig::group_selections`] and save
/// `client.json`.
///
/// Called by the Tauri command layer after a successful [`clash_select_proxy`].
pub fn persist_group_selection(
    data_dir: &std::path::Path,
    group: &str,
    name: &str,
) -> LanpathResult<()> {
    let mut cfg = crate::config::ClientConfig::load(data_dir)
        .map_err(|e| LanpathError::Client(format!("failed to load config: {e}")))?;
    cfg.group_selections
        .insert(group.to_string(), name.to_string());
    cfg.save()
        .map_err(|e| LanpathError::Client(format!("failed to save config: {e}")))
}

/// Replay persisted group selections after core startup.
///
/// Iterates over [`ClientConfig::group_selections`] and calls
/// [`clash_select_proxy`] for each entry. Individual failures are logged as
/// warnings and do **not** abort the replay.
pub async fn replay_group_selections(port: u16, secret: &str, data_dir: &std::path::Path) {
    let cfg = match crate::config::ClientConfig::load(data_dir) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "skip group-selection replay: config load failed");
            return;
        }
    };

    if cfg.group_selections.is_empty() {
        return;
    }

    for (group, name) in &cfg.group_selections {
        if let Err(e) = clash_select_proxy(port, secret, group, name).await {
            tracing::warn!(group, name, error = %e, "group selection replay failed");
        } else {
            tracing::info!(group, name, "group selection replayed");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
