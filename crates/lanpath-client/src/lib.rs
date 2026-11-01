//! lanpath-client — 桌面客户端核心库。
//!
//! 提供客户端配置（[`config`]）、分享链接解析（[`share_link`]）、Clash→sing-box 节点转换
//! （[`node_convert`]）、通用订阅管理（[`subscription`]）、三方配置片段导入
//! （[`import`]）、Profile 模板与复写（[`profile`]）、核心配置合成（[`core_config`]）、
//! 系统代理（[`sysproxy`]）、核心运行器（[`runner`]）、核心引擎桥（[`core_engine`]）、
//! MITM 构建（[`mitm`]）与运行状态编排（[`state`]）。桌面支持 sing-box/Mihomo，
//! Android 桥保持 sing-box。

pub mod backup;
pub mod config;
pub mod config_slices;
pub mod connections;
pub mod core_config;
pub mod core_engine;
pub mod core_management;
pub mod cores;
pub mod diagnose;
pub mod dns_probe;
pub mod http_exec;
pub mod import;
pub mod local_override;
pub mod logs;
#[cfg(feature = "mitm")]
pub mod mitm;
pub mod node_convert;
pub mod port_guard;
pub mod privilege;
pub mod profile;
pub mod proxies;
pub mod remote;
mod request_network;
pub use request_network::request_client_builder;
mod request_probe;
pub use request_probe::test_github_connectivity;
pub mod ruleset_manager;
pub mod runner;
pub mod settings;
pub mod share_link;
pub mod state;
pub mod stats;
pub mod subscription;
pub mod sysproxy;
pub mod validation;

pub use config::*;
pub use connections::*;
pub use core_config::*;
pub use core_engine::*;
pub use cores::*;
pub use http_exec::*;
pub use import::*;
pub use local_override::*;
#[cfg(feature = "mitm")]
pub use mitm::*;
pub use node_convert::*;
pub use privilege::*;
pub use profile::*;
pub use proxies::*;
pub use remote::*;
pub use runner::*;
pub use share_link::*;
pub use state::*;
pub use stats::*;
pub use subscription::*;
pub use sysproxy::*;
pub use validation::*;

/// 归一化 GitHub 资源 URL：`github.com/<owner>/<repo>/{blob,raw}/<branch>/<path>` →
/// `raw.githubusercontent.com/<owner>/<repo>/<branch>/<path>`；其他 URL 原样返回。
///
/// 供订阅拉取 / 远程资源拉取 / 导入脚本 URL / 远端嗅探在进入 HTTP 请求前统一调用，
/// 避免 GitHub 网页端 blob 链接被当作原始文件拉取。
pub fn normalize_resource_url(url: &str) -> String {
    let Some(rest) = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
    else {
        return url.to_string();
    };
    let mut parts = rest.splitn(5, '/');
    let (Some(owner), Some(repo), Some(kind), Some(branch)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return url.to_string();
    };
    if owner.is_empty() || repo.is_empty() || branch.is_empty() {
        return url.to_string();
    }
    match kind {
        "blob" | "raw" => {
            let path = parts.next().unwrap_or("");
            let suffix = if path.is_empty() {
                String::new()
            } else {
                format!("/{path}")
            };
            format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}{suffix}")
        }
        _ => url.to_string(),
    }
}

/// GitHub 相关域名列表（含 `www.github.com`、`api.github.com` 与各类
/// `*.githubusercontent.com` 子域）。
const GITHUB_HOSTS: [&str; 8] = [
    "github.com",
    "www.github.com",
    "api.github.com",
    "raw.githubusercontent.com",
    "gist.github.com",
    "gist.githubusercontent.com",
    "codeload.github.com",
    "objects.githubusercontent.com",
];

/// 判断 URL 的 host 是否属于 GitHub 域名（忽略 scheme，支持带端口）。
///
/// 用于远程资源拉取时决定是否应用 GitHub 代理前缀 / 失败时提示。
pub fn is_github_url(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.split(':').next().unwrap_or(host);
    GITHUB_HOSTS.contains(&host)
}

/// 为 GitHub URL 拼接代理前缀：prefix 为空或非 GitHub URL 时原样返回；
/// 否则返回 `{prefix}/{url}`（prefix 尾部的 `/` 会被去重）。
pub fn apply_github_proxy_prefix(url: &str, prefix: &str) -> String {
    let prefix = prefix.trim();
    if prefix.is_empty() || !is_github_url(url) {
        return url.to_string();
    }
    format!("{}/{}", prefix.trim_end_matches('/'), url)
}

/// 读取响应体字节；非 2xx 视为失败（沿用既有 fetch 语义）。
async fn read_body_bytes(resp: reqwest::Response, url: &str) -> Result<bytes::Bytes, String> {
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("remote fetch returned HTTP {status} ({url})"));
    }
    resp.bytes()
        .await
        .map_err(|e| format!("failed to read remote body ({url}): {e}"))
}

/// 拉取单个 URL 的二进制内容；请求层错误（connect/timeout 等）重试一次，非 2xx 视为失败。
async fn fetch_url_bytes_with_retry(
    client: &reqwest::Client,
    url: &str,
    guard: Option<&(dyn Fn() -> lanpath_common::LanpathResult<()> + Send + Sync)>,
) -> Result<bytes::Bytes, String> {
    let check = || match guard {
        Some(guard) => guard().map_err(|_| "owned resource endpoint is unavailable".to_owned()),
        None => Ok(()),
    };
    check()?;
    match client.get(url).send().await {
        Ok(resp) => read_body_bytes(resp, url).await,
        // 请求层错误（connect/timeout 等）重试一次；仍失败以最后一次错误为准。
        Err(_) => {
            check()?;
            match client.get(url).send().await {
                Ok(resp) => read_body_bytes(resp, url).await,
                Err(e) => Err(format!("remote fetch failed ({url}): {e}")),
            }
        }
    }
}

/// 远程资源二进制拉取（Hub 订阅 / 脚本 / 图标共用入口）。
///
/// - 从 `data_dir/client.json` 读取请求策略；首装使用默认值，配置损坏/迁移失败返回错误
/// - URL 先经 [`normalize_resource_url`] 归一化，GitHub URL 再经
///   [`apply_github_proxy_prefix`] 拼接配置的代理前缀
/// - 订阅与远程资源共用不使用 / 系统 / 核心 / 自定义代理策略
/// - 请求层错误重试一次；最终失败且原始 URL 是 GitHub URL 时，错误信息追加
///   「设置 → GitHub 访问」的处理提示
pub async fn fetch_resource_bytes(
    data_dir: &std::path::Path,
    url: &str,
    timeout: std::time::Duration,
) -> lanpath_common::LanpathResult<bytes::Bytes> {
    let cfg = config::ClientConfig::load_for_requests(data_dir)?;
    fetch_resource_bytes_with_config(&cfg, url, timeout).await
}

// Share the exact fetch pipeline with the saved-policy connectivity probe.
async fn fetch_resource_bytes_with_config(
    cfg: &ClientConfig,
    url: &str,
    timeout: std::time::Duration,
) -> lanpath_common::LanpathResult<bytes::Bytes> {
    fetch_resource_bytes_with_guard(cfg, url, timeout, None).await
}

type ResourceRequestGuard =
    std::sync::Arc<dyn Fn() -> lanpath_common::LanpathResult<()> + Send + Sync>;

async fn fetch_resource_bytes_with_guard(
    cfg: &ClientConfig,
    url: &str,
    timeout: std::time::Duration,
    guard: Option<ResourceRequestGuard>,
) -> lanpath_common::LanpathResult<bytes::Bytes> {
    use lanpath_common::LanpathError;

    let is_github = is_github_url(url);
    let request_url =
        apply_github_proxy_prefix(&normalize_resource_url(url), &cfg.github_proxy_prefix);
    let mut builder = request_network::request_client_builder(cfg)?.timeout(timeout);
    if let Some(guard) = guard.clone() {
        builder = builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if guard().is_err() {
                attempt.error("owned resource endpoint is unavailable")
            } else if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else {
                attempt.follow()
            }
        }));
    }
    let client = builder
        .build()
        .map_err(|_| LanpathError::Client("resource request client could not be created".into()))?;

    match fetch_url_bytes_with_retry(&client, &request_url, guard.as_deref()).await {
        Ok(bytes) => Ok(bytes),
        Err(e) => {
            if is_github {
                Err(LanpathError::Client(format!(
                    "{e}（GitHub 请求失败：请检查「设置 → 网络请求」或「GitHub 访问」中的镜像前缀）"
                )))
            } else {
                Err(LanpathError::Client(e))
            }
        }
    }
}

/// 解析 HTTP-date（RFC 7231 IMF-fixdate，如 `Sun, 06 Nov 1994 08:49:37 GMT`）
/// 为 Unix 秒；RFC 2822 兼容（chrono `parse_from_rfc2822` 覆盖 `GMT` 命名时区）。
fn parse_http_date(value: &str) -> Result<u64, String> {
    chrono::DateTime::parse_from_rfc2822(value)
        .map(|dt| dt.timestamp().max(0) as u64)
        .map_err(|e| e.to_string())
}

/// 读取 `Last-Modified` 响应头（无则 `None`）；非 2xx 视为失败。
fn read_last_modified(resp: reqwest::Response, url: &str) -> Result<Option<u64>, String> {
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("remote HEAD returned HTTP {status} ({url})"));
    }
    let Some(value) = resp.headers().get(reqwest::header::LAST_MODIFIED) else {
        return Ok(None);
    };
    let text = value
        .to_str()
        .map_err(|e| format!("invalid Last-Modified header ({url}): {e}"))?;
    parse_http_date(text)
        .map(Some)
        .map_err(|e| format!("invalid Last-Modified date ({url}): {e}"))
}

/// 通过 HEAD 请求读取远程资源的 `Last-Modified`（HTTP-date → Unix 秒）。
///
/// URL 归一化 / GitHub 代理前缀 / 本地代理策略与 [`fetch_resource_bytes`] 一致；
/// 请求层错误重试一次。响应无 `Last-Modified` 头返回 `Ok(None)`；非 2xx、
/// 非法日期头或网络错误均返回 `Err`（调用方按失败处理）。
pub async fn fetch_resource_last_modified(
    data_dir: &std::path::Path,
    url: &str,
    timeout: std::time::Duration,
) -> lanpath_common::LanpathResult<Option<u64>> {
    use lanpath_common::LanpathError;

    let is_github = is_github_url(url);
    let cfg = config::ClientConfig::load_for_requests(data_dir)?;
    let request_url =
        apply_github_proxy_prefix(&normalize_resource_url(url), &cfg.github_proxy_prefix);
    let client = request_network::build_client(timeout, &cfg)?;

    let result = match client.head(&request_url).send().await {
        Ok(resp) => read_last_modified(resp, &request_url),
        // 请求层错误（connect/timeout 等）重试一次；仍失败以最后一次错误为准。
        Err(_) => match client.head(&request_url).send().await {
            Ok(resp) => read_last_modified(resp, &request_url),
            Err(e) => Err(format!("remote HEAD failed ({request_url}): {e}")),
        },
    };

    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            if is_github {
                Err(LanpathError::Client(format!(
                    "{e}（GitHub 请求失败：请检查「设置 → 网络请求」或「GitHub 访问」中的镜像前缀）"
                )))
            } else {
                Err(LanpathError::Client(e))
            }
        }
    }
}

/// 远程资源文本拉取（Hub 订阅 / 脚本 / 嗅探共用入口），语义与
/// [`fetch_resource_bytes`] 一致（GitHub 代理前缀 / 请求代理策略 / 重试 / 提示）。
///
/// 返回体按 UTF-8 解码；非 UTF-8 内容视为失败。
pub async fn fetch_resource_text(
    data_dir: &std::path::Path,
    url: &str,
    timeout: std::time::Duration,
) -> lanpath_common::LanpathResult<String> {
    use lanpath_common::LanpathError;

    let bytes = fetch_resource_bytes(data_dir, url, timeout).await?;
    String::from_utf8(bytes.to_vec()).map_err(|e| {
        LanpathError::Client(format!("remote fetch returned non-UTF-8 body ({url}): {e}"))
    })
}

#[cfg(test)]
mod resource_fetch_tests;
