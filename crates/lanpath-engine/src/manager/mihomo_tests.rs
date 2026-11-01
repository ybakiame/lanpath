use super::*;
use crate::CoreManagerFactory;
use crate::lifecycle::tests::{fixture_binary, fixture_child};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    manager: MihomoProcessManager,
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "lanpath-mihomo-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            manager: MihomoProcessManager::new(fixture_binary(), &dir).unwrap(),
            dir,
        }
    }

    async fn install_child(&self) {
        let (child, pid) = fixture_child();
        *self.manager.process.write().await = Some(child);
        lifecycle::write_pid_file(&self.manager.pid_path(), pid, &fixture_binary()).unwrap();
        *self.manager.start_time.write().await = Some(Instant::now());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(mut child) = self.manager.process.get_mut().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn factory_selects_mihomo_with_independent_paths_and_exact_spawn_args() {
    let dir = std::env::temp_dir().join("lanpath-mihomo-factory");
    let manager = CoreManagerFactory::create(CoreType::Mihomo, fixture_binary(), &dir).unwrap();
    assert_eq!(manager.core_type(), CoreType::Mihomo);

    let manager = MihomoProcessManager::new(fixture_binary(), &dir).unwrap();
    assert_eq!(manager.config_path(), dir.join("mihomo.yaml"));
    assert_eq!(manager.pid_path(), dir.join("mihomo.pid"));
    assert_eq!(
        manager.spawn_args(),
        [
            OsString::from("-d"),
            dir.as_os_str().to_owned(),
            OsString::from("-f"),
            dir.join("mihomo.yaml").as_os_str().to_owned(),
        ]
    );
}

#[tokio::test]
async fn config_write_uses_independent_mihomo_yaml_file() {
    let fixture = Fixture::new("config");
    let config = serde_json::json!({
        "mixed-port": 7890,
        "proxies": [{"name": "direct", "type": "direct"}]
    });

    let path = fixture.manager.write_config(&config).await.unwrap();

    assert_eq!(path, fixture.dir.join("mihomo.yaml"));
    let written: Value = serde_yaml::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(written, config);
    assert!(!fixture.manager.pid_path().exists());
}

#[tokio::test]
async fn stop_process_reaps_live_child_and_cleans_mihomo_pid() {
    let fixture = Fixture::new("stop");
    fixture.install_child().await;

    assert_eq!(
        fixture.manager.stop_process().await.unwrap(),
        ProcessStopKind::Forced
    );
    assert!(fixture.manager.process.read().await.is_none());
    assert!(fixture.manager.start_time.read().await.is_none());
    assert!(!fixture.manager.pid_path().exists());
    assert_eq!(
        fixture.manager.stop_process().await.unwrap(),
        ProcessStopKind::AlreadyStopped
    );
}

#[tokio::test]
async fn real_mihomo_starts_and_stops_owned_process_when_available() {
    let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-cores/mihomo")
        .with_extension(std::env::consts::EXE_SUFFIX.trim_start_matches('.'));
    if !binary.is_file() {
        return;
    }
    let fixture = Fixture::new("real");
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let manager = MihomoProcessManager::new(&binary, &fixture.dir).unwrap();
    manager
        .start(&serde_json::json!({
            "mixed-port": port,
            "allow-lan": false,
            "bind-address": "127.0.0.1",
            "proxies": [],
            "rules": ["MATCH,DIRECT"]
        }))
        .await
        .unwrap();
    assert!(manager.is_running().await);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "mihomo mixed listener did not start"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        manager.stop_process().await.unwrap(),
        ProcessStopKind::Forced
    );
    assert!(!manager.is_running().await);
    assert!(!manager.pid_path().exists());
    assert!(fixture.dir.join("mihomo.yaml").is_file());
}

#[cfg(any(windows, target_os = "linux", target_os = "android"))]
#[tokio::test]
async fn stale_mihomo_process_is_reaped_by_recorded_identity() {
    let fixture = Fixture::new("stale");
    fixture.install_child().await;
    let mut child = fixture.manager.process.write().await.take().unwrap();

    let result = fixture.manager.stop_process().await;
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().unwrap();

    assert_eq!(result.unwrap(), ProcessStopKind::Forced);
    assert!(!status.success());
    assert!(!fixture.manager.pid_path().exists());
    assert!(fixture.manager.start_time.read().await.is_none());
}
