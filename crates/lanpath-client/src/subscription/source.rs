//! Original subscription responses are independent of normalized runtime JSON.

use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{SubFormat, SubscriptionStore};

/// Last successfully parsed response, including comments, whitespace and encoding text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriptionSource {
    pub format: SubFormat,
    pub content: String,
}

impl SubscriptionSource {
    /// Extension reflects the received representation; base64 is exported as text.
    pub fn extension(&self) -> &'static str {
        match self.format {
            SubFormat::ClashYaml => "yaml",
            SubFormat::SingBoxJson => "json",
            SubFormat::ShareLinks => "txt",
        }
    }
}

impl SubscriptionStore {
    fn source_file(&self, id: Uuid) -> std::path::PathBuf {
        self.file()
            .with_file_name("subscription_cache")
            .join(format!("{id}.source.json"))
    }

    /// Persist the response separately; existing converted JSON remains unchanged.
    pub fn write_source(&self, id: Uuid, source: &SubscriptionSource) -> LanpathResult<()> {
        let path = self.source_file(id);
        std::fs::create_dir_all(self.file().with_file_name("subscription_cache"))?;
        std::fs::write(path, serde_json::to_vec(source)?)?;
        Ok(())
    }

    /// Missing source on an older installation is expected; refresh to populate it.
    pub fn load_source(&self, id: Uuid) -> LanpathResult<Option<SubscriptionSource>> {
        match std::fs::read(self.source_file(id)) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub(super) fn clear_source(&self, id: Uuid) {
        if let Err(e) = std::fs::remove_file(self.source_file(id))
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%id, error = %e, "failed to clear subscription source");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CachedSubscriptionContent, parse_subscription_body};

    #[test]
    fn preserves_source_without_changing_runtime_cache_and_clears_on_url_change() {
        let dir = tempfile::tempdir().unwrap();
        let store = SubscriptionStore::new(dir.path().into());
        let sub = store
            .add("test", "https://example.com/a", true, None)
            .unwrap();
        assert_eq!(store.load_source(sub.id).unwrap(), None);
        let yaml = "\u{feff}# original comment\r\nproxies: []\r\n";
        let fetched = parse_subscription_body(yaml, None).unwrap();
        assert_eq!(fetched.source.as_bytes(), yaml.as_bytes());
        let source = SubscriptionSource {
            format: fetched.format,
            content: fetched.source,
        };
        store.write_source(sub.id, &source).unwrap();
        store
            .write_cached_content(
                sub.id,
                &CachedSubscriptionContent {
                    format: fetched.format,
                    singbox_nodes: fetched.singbox_nodes,
                },
            )
            .unwrap();
        assert_eq!(source.extension(), "yaml");
        assert_eq!(store.load_source(sub.id).unwrap(), Some(source.clone()));
        assert!(
            !std::fs::read_to_string(store.cache_file(sub.id))
                .unwrap()
                .contains("original comment")
        );
        assert!(parse_subscription_body("invalid", None).is_err());
        assert_eq!(store.load_source(sub.id).unwrap(), Some(source));
        store
            .update(sub.id, "renamed", "https://example.com/a", None)
            .unwrap();
        assert!(store.load_source(sub.id).unwrap().is_some());
        store
            .update(sub.id, "renamed", "https://example.com/b", None)
            .unwrap();
        assert_eq!(store.load_source(sub.id).unwrap(), None);
        assert!(!store.cache_file(sub.id).exists());
    }

    #[test]
    fn json_and_base64_keep_received_representation_and_remove_cleans_source() {
        let dir = tempfile::tempdir().unwrap();
        let store = SubscriptionStore::new(dir.path().into());
        let sub = store
            .add("test", "https://example.com", true, None)
            .unwrap();
        for (text, extension) in [
            ("{\n  \"outbounds\": []\n}\n", "json"),
            ("dHJvamFuOi8vcGFzc0Bob3N0OjQ0Mw==\n", "txt"),
        ] {
            let fetched = parse_subscription_body(text, None).unwrap();
            let source = SubscriptionSource {
                format: fetched.format,
                content: fetched.source,
            };
            assert_eq!(source.content, text);
            assert_eq!(source.extension(), extension);
            store.write_source(sub.id, &source).unwrap();
        }
        store.remove(sub.id).unwrap();
        assert_eq!(store.load_source(sub.id).unwrap(), None);
    }
}

#[cfg(test)]
mod http_tests {
    use super::super::SubscriptionFetcher;
    use axum::{Router, routing::get};

    #[tokio::test]
    async fn fetch_preserves_http_body_and_rejects_lossy_utf8() {
        const YAML: &str = "\u{feff}# original\r\nproxies: []\r\n";
        let app = Router::new()
            .route("/yaml", get(|| async { YAML }))
            .route("/invalid", get(|| async { vec![0xff_u8, 0xfe, 0x00] }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let fetcher = SubscriptionFetcher::new();
        let fetched = fetcher.fetch(&format!("http://{addr}/yaml")).await.unwrap();
        assert_eq!(fetched.source.as_bytes(), YAML.as_bytes());
        assert!(
            fetcher
                .fetch(&format!("http://{addr}/invalid"))
                .await
                .unwrap_err()
                .to_string()
                .contains("not UTF-8")
        );
        server.abort();
    }
}
