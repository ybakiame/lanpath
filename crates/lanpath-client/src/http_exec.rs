//! Script HTTP uses the saved outbound policy and a cookie jar scoped to one run.
//! A core policy requires an explicitly ready endpoint; it never falls back to direct.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU16, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use lanpath_common::{LanpathError, LanpathResult};
use lanpath_script::{HttpAlpn, HttpExecutor, HttpRequestSpec, HttpResponseData};

use crate::{ClientConfig, RequestProxyMode};

const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
struct SessionClients {
    config: Option<ClientConfig>,
    jar: Option<Arc<reqwest::cookie::Jar>>,
    // Cookie, redirect, insecure-TLS and protocol options are request-local.
    clients: [Option<reqwest::Client>; 24],
}

pub struct ReqwestHttpExecutor {
    root: Arc<PathBuf>,
    core_port: Arc<AtomicU16>,
    owned_system_proxy: Arc<AtomicBool>,
    clients: Mutex<SessionClients>,
}

impl ReqwestHttpExecutor {
    pub fn for_data_dir(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Arc::new(root.into()),
            core_port: Arc::new(AtomicU16::new(0)),
            owned_system_proxy: Arc::new(AtomicBool::new(false)),
            clients: Mutex::new(SessionClients::default()),
        }
    }

    /// Set only after the selected endpoint is ready; with MITM use its core-return
    /// inbound, not the public inbound that would run the same hook recursively.
    pub fn set_core_port(&self, port: Option<u16>) {
        self.core_port.store(port.unwrap_or(0), Ordering::Release);
    }

    /// The lifecycle sets intent before acquisition and clears it only after
    /// restoring its owned proxy. Internal requests must not enter their own hook.
    pub fn set_system_proxy_owned(&self, owned: bool) {
        self.owned_system_proxy.store(owned, Ordering::Release);
    }

    fn client(
        &self,
        cookie: bool,
        redirect: bool,
        insecure: bool,
        alpn: HttpAlpn,
    ) -> LanpathResult<reqwest::Client> {
        let protocol = match alpn {
            HttpAlpn::Auto => 0,
            HttpAlpn::Http1 => 1,
            HttpAlpn::Http2 => 2,
        };
        let key = usize::from(cookie)
            | (usize::from(redirect) << 1)
            | (usize::from(insecure) << 2)
            | (protocol << 3);
        let mut session = self
            .clients
            .lock()
            .map_err(|_| failure("script HTTP session lock poisoned"))?;
        if session.config.is_none() {
            let mut config = ClientConfig::load_for_requests(&self.root)?;
            let port = self.core_port.load(Ordering::Acquire);
            let own_system = config.request_proxy_mode == RequestProxyMode::System
                && self.owned_system_proxy.load(Ordering::Acquire);
            let own_custom = config.request_proxy_mode == RequestProxyMode::Custom
                && port != 0
                && port != config.mixed_port
                && reqwest::Url::parse(&config.request_proxy_url)
                    .ok()
                    .is_some_and(|url| {
                        matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
                            && url.port_or_known_default() == Some(config.mixed_port)
                    });
            if config.request_proxy_mode == RequestProxyMode::Core || own_system || own_custom {
                if port == 0 {
                    return Err(failure(
                        "script core proxy is not ready; direct fallback is disabled",
                    ));
                }
                config.request_proxy_mode = RequestProxyMode::Core;
                config.mixed_port = port;
            }
            session.config = Some(config);
        }
        if let Some(client) = &session.clients[key] {
            return Ok(client.clone());
        }
        let config = session
            .config
            .as_ref()
            .ok_or_else(|| failure("missing script HTTP policy"))?;
        let mut builder = crate::request_client_builder(config)?
            .timeout(Duration::from_secs(30))
            .redirect(if redirect {
                reqwest::redirect::Policy::limited(10)
            } else {
                reqwest::redirect::Policy::none()
            })
            .danger_accept_invalid_certs(insecure);
        builder = match alpn {
            HttpAlpn::Auto => builder,
            HttpAlpn::Http1 => builder.http1_only(),
            HttpAlpn::Http2 => builder.http2_prior_knowledge(),
        };
        if cookie {
            let jar = session
                .jar
                .get_or_insert_with(|| Arc::new(reqwest::cookie::Jar::default()));
            builder = builder.cookie_provider(Arc::clone(jar));
        } else {
            builder = builder.cookie_store(false);
        }
        let client = builder.build().map_err(|error| {
            failure(format!(
                "script HTTP client creation failed: {}",
                error.without_url()
            ))
        })?;
        session.clients[key] = Some(client.clone());
        Ok(client)
    }
}

#[async_trait]
impl HttpExecutor for ReqwestHttpExecutor {
    fn session(self: Arc<Self>) -> Arc<dyn HttpExecutor> {
        Arc::new(Self {
            root: Arc::clone(&self.root),
            core_port: Arc::clone(&self.core_port),
            owned_system_proxy: Arc::clone(&self.owned_system_proxy),
            clients: Mutex::new(SessionClients::default()),
        })
    }

    async fn execute(&self, req: HttpRequestSpec) -> LanpathResult<HttpResponseData> {
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|_| failure("invalid script HTTP method"))?;
        let url = reqwest::Url::parse(&req.url).map_err(|_| failure("invalid script HTTP URL"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(failure("script HTTP URL must use HTTP or HTTPS"));
        }
        if req
            .body
            .as_ref()
            .is_some_and(|body| body.len() > MAX_BODY_BYTES)
        {
            return Err(failure("script HTTP request body exceeds 32 MiB"));
        }
        let client = self.client(req.auto_cookie, req.auto_redirect, req.insecure, req.alpn)?;
        let mut builder = client.request(method, url);
        for (name, value) in req.headers {
            let name = reqwest::header::HeaderName::try_from(name)
                .map_err(|_| failure("invalid script HTTP header name"))?;
            builder = builder.header(name, header_value(value)?);
        }
        if let Some(body) = req.body {
            builder = builder.body(body);
        }
        if let Some(timeout_ms) = req.timeout_ms {
            builder = builder.timeout(Duration::from_millis(timeout_ms));
        }
        let mut response = builder.send().await.map_err(|error| {
            failure(format!(
                "script HTTP request failed: {}",
                error.without_url()
            ))
        })?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                // HTTP field values are opaque octets; use the same reversible
                // Latin-1 bridge as MITM hooks rather than dropping non-UTF-8 fields.
                (
                    name.as_str().to_owned(),
                    value
                        .as_bytes()
                        .iter()
                        .map(|byte| char::from(*byte))
                        .collect(),
                )
            })
            .collect();
        let expected = response.content_length().unwrap_or(0);
        if expected > MAX_BODY_BYTES as u64 {
            return Err(failure("script HTTP response body exceeds 32 MiB"));
        }
        let mut body = Vec::with_capacity(expected as usize);
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            failure(format!(
                "script HTTP response failed: {}",
                error.without_url()
            ))
        })? {
            if chunk.len() > MAX_BODY_BYTES - body.len() {
                return Err(failure("script HTTP response body exceeds 32 MiB"));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpResponseData {
            status,
            headers,
            body,
        })
    }
}

fn header_value(value: String) -> LanpathResult<reqwest::header::HeaderValue> {
    let mut bytes = value.into_bytes();
    let mut read = 0;
    let mut written = 0;
    while read < bytes.len() {
        let byte = bytes[read];
        let decoded = if byte < 0x80 {
            read += 1;
            byte
        } else if matches!(byte, 0xc2 | 0xc3) {
            // String is valid UTF-8, so these two-byte prefixes have a continuation.
            let decoded = ((byte & 0x03) << 6) | (bytes[read + 1] & 0x3f);
            read += 2;
            decoded
        } else {
            return Err(failure(
                "script HTTP header contains a character outside Latin-1",
            ));
        };
        bytes[written] = decoded;
        written += 1;
    }
    bytes.truncate(written);
    reqwest::header::HeaderValue::from_maybe_shared(bytes::Bytes::from(bytes))
        .map_err(|_| failure("invalid script HTTP header value"))
}

fn failure(message: impl Into<String>) -> LanpathError {
    LanpathError::Script(message.into())
}

#[cfg(test)]
#[path = "http_exec_tests.rs"]
mod tests;
