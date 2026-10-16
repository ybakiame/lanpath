use std::{error::Error, time::Duration};

use lanpath_common::{LanpathError, LanpathResult};

use super::{FetchResult, parse_subscription_body};

/// Subscription requests use the shared request policy and a session cookie jar.
pub(super) fn build_client(
    timeout: Duration,
    cfg: Option<&crate::ClientConfig>,
) -> LanpathResult<reqwest::Client> {
    crate::request_network::build_client(timeout, cfg.unwrap_or(&crate::ClientConfig::default()))
}

/// Use the saved remote-resource proxy settings for add/refresh/preview/start.
pub async fn fetch_subscription_for_config(
    cfg: &crate::ClientConfig,
    url: &str,
    ua: Option<&str>,
) -> LanpathResult<FetchResult> {
    let client = build_client(Duration::from_secs(30), Some(cfg))?;
    let url = crate::apply_github_proxy_prefix(
        &crate::normalize_resource_url(url),
        &cfg.github_proxy_prefix,
    );
    SubscriptionFetcher::with_client(client)
        .fetch_with_user_agent(&url, ua)
        .await
}

/// Subscription fetcher. Reuse its client for redirects and the bounded retry.
#[derive(Debug, Clone)]
pub struct SubscriptionFetcher {
    client: reqwest::Client,
}

impl Default for SubscriptionFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscriptionFetcher {
    /// Create a session with system proxy discovery and a 15s timeout.
    pub fn new() -> Self {
        Self {
            client: build_client(Duration::from_secs(15), None)
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Inject a custom client, including any custom cookie/proxy policy.
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub async fn fetch(&self, url: &str) -> LanpathResult<FetchResult> {
        self.fetch_with_user_agent(url, None).await
    }

    pub(super) async fn fetch_with_user_agent(
        &self,
        url: &str,
        ua: Option<&str>,
    ) -> LanpathResult<FetchResult> {
        let url = crate::normalize_resource_url(url);
        let ua = ua
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("clash.meta");
        for attempt in 0..2 {
            let response = self
                .client
                .get(&url)
                .header(reqwest::header::USER_AGENT, ua)
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(error) if attempt == 0 && (error.is_connect() || error.is_timeout()) => {
                    continue;
                }
                Err(error) => return Err(request_error(error, &url)),
            };
            let status = response.status();
            let sets_cookie = response.headers().contains_key(reqwest::header::SET_COOKIE);
            let info = super::parse_subscription_userinfo(response.headers());
            let bytes = response.bytes().await.map_err(|e| {
                LanpathError::Client(format!(
                    "failed to read subscription body: {}",
                    e.without_url()
                ))
            })?;
            let text = std::str::from_utf8(&bytes).map_err(|e| {
                LanpathError::Client(format!("subscription body is not UTF-8: {e}"))
            })?;
            // Some sources set a session cookie on an HTML landing response,
            // even with HTTP 200. Retry once immediately with the same cookie jar.
            let html = text
                .trim_start_matches('\u{feff}')
                .trim_start()
                .starts_with('<');
            if attempt == 0 && sets_cookie && html {
                continue;
            }
            if !status.is_success() {
                return Err(LanpathError::Client(format!(
                    "subscription request returned HTTP {status}"
                )));
            }
            if html {
                return Err(LanpathError::Client(
                    "subscription source returned an HTML page instead of subscription data".into(),
                ));
            }
            return parse_subscription_body(text, info);
        }
        Err(LanpathError::Client(
            "subscription request retry exhausted".into(),
        ))
    }
}

/// Expose the transport cause (DNS/TLS/connect) without embedding the private URL.
fn request_error(error: reqwest::Error, url: &str) -> LanpathError {
    let mut cause: &dyn Error = &error;
    while let Some(source) = cause.source() {
        cause = source;
    }
    let detail = cause.to_string().replace(url, "<subscription>");
    LanpathError::Client(format!(
        "subscription request failed: {}: {detail}",
        error.without_url()
    ))
}
