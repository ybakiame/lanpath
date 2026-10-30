//! Accepted resource request policy and live-input publication boundary.
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use lanpath_common::{LanpathError, LanpathResult};

use crate::{ClientConfig, RequestProxyMode};

/// One accepted policy snapshot for a resource batch, including dependent assets.
/// A live runtime must retain lifecycle serialization until the batch is dropped/completed.
pub struct RemoteFetchContext {
    config: Option<ClientConfig>,
    protected_inputs: Vec<(PathBuf, String)>,
    live_guard: Option<crate::ResourceRequestGuard>,
    rejected: Arc<AtomicBool>,
}

impl RemoteFetchContext {
    /// Preparation without a ready owned core may read existing caches, but may
    /// neither issue requests nor publish an automatic refresh attempt.
    pub fn cache_only() -> Self {
        Self {
            config: None,
            protected_inputs: Vec::new(),
            live_guard: None,
            rejected: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn allows_network(&self) -> bool {
        self.config.is_some()
    }

    /// A stopped runtime cannot lend a saved port to Core-policy requests.
    pub fn stopped(config: ClientConfig) -> LanpathResult<Self> {
        if config.request_proxy_mode == RequestProxyMode::Core {
            return Err(unavailable());
        }
        config.validate_request_proxy()?;
        Ok(Self {
            config: Some(config),
            protected_inputs: Vec::new(),
            live_guard: None,
            rejected: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Accept only a ready process/listener proved owned by the caller. `main_port`
    /// and `request_port` are active-session ports, never reconstructed saved ports.
    /// With MITM, `request_port` must be its core-return inbound, not interception.
    pub fn running(
        mut config: ClientConfig,
        main_port: u16,
        request_port: u16,
        owns_system_proxy: bool,
        protected_inputs: Vec<PathBuf>,
    ) -> LanpathResult<Self> {
        if main_port == 0 || request_port == 0 {
            return Err(unavailable());
        }
        config.validate_request_proxy()?;
        let own_custom = config.request_proxy_mode == RequestProxyMode::Custom
            && reqwest::Url::parse(config.request_proxy_url.trim())
                .ok()
                .is_some_and(|url| {
                    matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
                        && matches!(url.port_or_known_default(), Some(port) if port == main_port || port == request_port)
                });
        if own_custom {
            let url =
                reqwest::Url::parse(config.request_proxy_url.trim()).map_err(|_| unavailable())?;
            // An owned HTTP return inbound cannot satisfy an explicitly requested
            // HTTPS proxy or different authentication contract. Never downgrade it.
            if url.scheme() == "https" || !url.username().is_empty() || url.password().is_some() {
                return Err(LanpathError::Client(
                    "owned core proxy bypass cannot weaken explicit TLS or authentication settings"
                        .into(),
                ));
            }
        }
        if config.request_proxy_mode == RequestProxyMode::Core
            || (config.request_proxy_mode == RequestProxyMode::System && owns_system_proxy)
            || own_custom
        {
            config.request_proxy_mode = RequestProxyMode::Core;
            config.mixed_port = request_port;
        }
        let protected_inputs = protected_inputs
            .into_iter()
            .map(|path| normalized_path(&path).map(|normalized| (path, normalized)))
            .collect::<LanpathResult<_>>()?;
        Ok(Self {
            config: Some(config),
            protected_inputs,
            live_guard: None,
            rejected: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Revalidate immutable runtime ownership before every send/redirect/retry
    /// and publication. A rejection invalidates the entire accepted context.
    pub fn with_live_guard(
        mut self,
        guard: Arc<dyn Fn() -> LanpathResult<()> + Send + Sync>,
    ) -> Self {
        let rejected = Arc::clone(&self.rejected);
        self.live_guard = Some(Arc::new(move || {
            if rejected.load(Ordering::Acquire) || guard().is_err() {
                rejected.store(true, Ordering::Release);
                return Err(unavailable());
            }
            Ok(())
        }));
        self
    }

    pub(super) fn check_live(&self) -> LanpathResult<()> {
        if self.rejected.load(Ordering::Acquire) {
            return Err(unavailable());
        }
        if let Some(guard) = &self.live_guard {
            guard()?;
        }
        Ok(())
    }

    pub(crate) fn require_network(&self) -> LanpathResult<()> {
        if !self.allows_network() {
            return Err(LanpathError::Client(
                "resource preparation is cache-only: owned core proxy is not ready".into(),
            ));
        }
        self.check_live()
    }

    pub(crate) async fn fetch_text(&self, url: &str) -> LanpathResult<String> {
        let bytes = self
            .fetch_bytes(url, std::time::Duration::from_secs(30))
            .await?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| LanpathError::Client("resource body is not UTF-8".into()))
    }

    pub(crate) async fn fetch_bytes(
        &self,
        url: &str,
        timeout: std::time::Duration,
    ) -> LanpathResult<bytes::Bytes> {
        self.require_network()?;
        let config = self.config.as_ref().ok_or_else(unavailable)?;
        crate::fetch_resource_bytes_with_guard(config, url, timeout, self.live_guard.clone())
            .await
            .map_err(|_| LanpathError::Client("resource download failed".into()))
    }

    /// Check every prospective destination before network or filesystem mutation,
    /// and again immediately before publication. Prefix roots also protect directories.
    pub(crate) fn check_write(&self, path: &Path) -> LanpathResult<()> {
        self.check_live()?;
        let result = self.check_path(path);
        if result.is_err() {
            self.rejected.store(true, Ordering::Release);
        }
        result
    }

    fn check_path(&self, path: &Path) -> LanpathResult<()> {
        let destination = normalized_path(path)?;
        for (original, accepted) in &self.protected_inputs {
            let current = normalized_path(original)?;
            if overlaps(&destination, accepted) || overlaps(&destination, &current) {
                return Err(conflict());
            }
        }
        Ok(())
    }
}

fn unavailable() -> LanpathError {
    LanpathError::Client(
        "automatic refresh deferred: owned core request endpoint is unavailable; direct fallback is disabled"
            .into(),
    )
}

fn conflict() -> LanpathError {
    LanpathError::Client(
        "automatic refresh deferred: publication overlaps a live core input or its path cannot be proven safe; cache and attempt age retained"
            .into(),
    )
}

fn overlaps(left: &str, right: &str) -> bool {
    left == right || descendant(left, right) || descendant(right, left)
}

fn descendant(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|suffix| root.ends_with('/') || suffix.starts_with('/'))
}

fn normalized_path(path: &Path) -> LanpathResult<String> {
    // Resolve the deepest existing ancestor, including symlinks/junctions. Missing
    // suffixes are allowed, but traversal/ambiguous platform aliases are not.
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(conflict());
    }
    let mut ancestor = path;
    let mut suffix = Vec::new();
    let canonical = loop {
        match std::fs::canonicalize(ancestor) {
            Ok(canonical) => break canonical,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A dangling symlink is not a safe missing directory/file.
                if std::fs::symlink_metadata(ancestor).is_ok() {
                    return Err(conflict());
                }
                suffix.push(ancestor.file_name().ok_or_else(conflict)?.to_owned());
                ancestor = ancestor.parent().ok_or_else(conflict)?;
            }
            Err(_) => return Err(conflict()),
        }
    };
    let mut resolved = canonical;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    let text = resolved.to_str().ok_or_else(conflict)?.replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    if text.starts_with("//./") || text.starts_with("/??/") {
        return Err(conflict());
    }
    // Canonical Windows namespace and ordinary spelling share a comparison key;
    // case folding on Unix deliberately errs on the conservative side as well.
    let text = text
        .strip_prefix("UNC/")
        .map_or_else(|| text.to_owned(), |rest| format!("//{rest}"));
    for (index, part) in text.split('/').enumerate() {
        if part.ends_with('.')
            || part.ends_with(' ')
            || (part.contains(':') && !(index == 0 && part.len() == 2 && part.ends_with(':')))
        {
            return Err(conflict());
        }
    }
    Ok(text.to_lowercase())
}
