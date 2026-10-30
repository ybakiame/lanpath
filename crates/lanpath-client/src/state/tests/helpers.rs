//! Shared test helpers for state tests.

#![allow(dead_code)]

pub use std::net::SocketAddr;
pub use std::path::{Path, PathBuf};
pub use std::sync::Arc;
pub use std::time::Duration;

pub use axum::http::StatusCode;
pub use base64::Engine as _;
pub use tempfile::TempDir;

pub use crate::config::ClientConfig;
pub use crate::remote::{RemoteKind, RemoteResource};
pub use crate::state::ClientState;
pub use crate::subscription;
pub use crate::sysproxy::MockSystemProxy;

/// Start a local axum server that returns `(status, body)` for all paths (for testing, no external requests).
pub async fn spawn_server(status: StatusCode, body: &'static str) -> SocketAddr {
    let app = axum::Router::new().fallback(move || async move { (status, body) });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

/// A native image, not an interpreter script: recovery must verify the exact executable.
pub fn native_core_fixture(dir: &TempDir) -> PathBuf {
    let path = dir.path().join("lifecycle-core");
    std::fs::copy(compiled_core_fixture(), &path).unwrap();
    path
}

pub fn native_core_capturing_args(dir: &TempDir, capture: &Path) -> PathBuf {
    let path = native_core_fixture(dir);
    std::fs::write(path.with_extension("capture"), capture.to_str().unwrap()).unwrap();
    path
}

pub fn test_config(dir: &TempDir, hub_url: String) -> ClientConfig {
    let cfg = fixture_config(
        dir.path().to_path_buf(),
        hub_url,
        "tok",
        native_core_fixture(dir),
    );
    // start now always reloads config from disk: test config must first write client.json to disk.
    cfg.save().unwrap();
    cfg
}

fn compiled_core_fixture() -> &'static Path {
    static COMPILED: std::sync::LazyLock<(TempDir, PathBuf)> = std::sync::LazyLock::new(|| {
        let directory = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        let dependencies = executable.parent().unwrap();
        let serde_json = std::fs::read_dir(dependencies)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("libserde_json-") && name.ends_with(".rlib")
                    })
            })
            .expect("Cargo-built serde_json library");
        let output = std::process::Command::new("rustc")
            .args(["--edition=2024", "--crate-name=lanpath_lifecycle_fixture"])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/lifecycle_core.rs"
            ))
            .arg("--extern")
            .arg(format!("serde_json={}", serde_json.display()))
            .arg("-L")
            .arg(format!("dependency={}", dependencies.display()))
            .arg("-o")
            .arg(directory.path().join("core"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "native fixture compile failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let binary = directory.path().join("core");
        (directory, binary)
    });
    &COMPILED.1
}

/// Keep native child listeners outside the OS's automatically allocated ports.
/// Dropping a listener obtained with `:0` lets parallel HTTP clients/servers steal
/// its port between config creation and the child's eventual bind.
fn fixture_port_pair() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(10_000);
    static AUTOMATIC: std::sync::LazyLock<(u16, u16)> = std::sync::LazyLock::new(|| {
        #[cfg(target_os = "linux")]
        {
            let range = std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range").unwrap();
            let mut ports = range.split_whitespace();
            (
                ports.next().unwrap().parse().unwrap(),
                ports.next().unwrap().parse().unwrap(),
            )
        }
        #[cfg(not(target_os = "linux"))]
        {
            (49_152, u16::MAX)
        }
    });
    loop {
        let port = NEXT.fetch_add(2, Ordering::Relaxed);
        assert!(
            (1_024..u16::MAX - 1).contains(&port),
            "fixture port pool exhausted"
        );
        if (AUTOMATIC.0..=AUTOMATIC.1).contains(&port)
            || (AUTOMATIC.0..=AUTOMATIC.1).contains(&(port + 1))
        {
            continue;
        }
        let Ok(_main) = std::net::TcpListener::bind(("127.0.0.1", port)) else {
            continue;
        };
        if std::net::TcpListener::bind(("127.0.0.1", port + 1)).is_ok() {
            return port;
        }
    }
}

pub fn fixture_config(
    data_dir: PathBuf,
    hub_url: impl Into<String>,
    token: impl Into<String>,
    binary: PathBuf,
) -> ClientConfig {
    let mut config = ClientConfig::new(data_dir, hub_url, token, binary);
    config.mixed_port = fixture_port_pair();
    config
}

/// 向 subscriptions.json 添加指向 `url` 的订阅并返回其 id（供测试设为 active_subscription_id；
/// 旧版 Hub 回退路径已移除，启动必须选中订阅）。
pub fn add_local_subscription(dir: &TempDir, url: &str) -> uuid::Uuid {
    let store = crate::subscription::SubscriptionStore::new(dir.path().to_path_buf());
    store.add("local", url, true, None).unwrap().id
}
