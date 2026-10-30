use super::*;
use crate::lifecycle::tests::{fixture_binary, fixture_child};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    manager: SingBoxProcessManager,
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "lanpath-stop-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            manager: SingBoxProcessManager::new(fixture_binary(), &dir).unwrap(),
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

#[tokio::test]
async fn stop_process_live_child_returns_forced_after_cleanup() {
    let fixture = Fixture::new("live");
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
async fn stop_process_exited_child_returns_already_stopped() {
    let fixture = Fixture::new("exited");
    fixture.install_child().await;
    {
        let mut process = fixture.manager.process.write().await;
        let child = process.as_mut().unwrap();
        // Closing stdin lets the native fixture exit normally, without timing probes.
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
    }

    assert_eq!(
        fixture.manager.stop_process().await.unwrap(),
        ProcessStopKind::AlreadyStopped
    );
    assert!(fixture.manager.process.read().await.is_none());
    assert!(!fixture.manager.pid_path().exists());
}

#[tokio::test]
async fn stop_process_missing_record_returns_already_stopped() {
    let fixture = Fixture::new("missing");
    assert_eq!(
        fixture.manager.stop_process().await.unwrap(),
        ProcessStopKind::AlreadyStopped
    );
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn stop_process_dead_stale_record_returns_already_stopped() {
    let fixture = Fixture::new("dead-stale");
    fixture.install_child().await;
    let mut child = fixture.manager.process.write().await.take().unwrap();
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());

    assert_eq!(
        fixture.manager.stop_process().await.unwrap(),
        ProcessStopKind::AlreadyStopped
    );
    assert!(!fixture.manager.pid_path().exists());
    assert!(fixture.manager.start_time.read().await.is_none());
}

#[cfg(any(windows, target_os = "linux", target_os = "android"))]
#[tokio::test]
async fn stop_process_live_stale_record_returns_actual_forced_receipt() {
    let fixture = Fixture::new("live-stale");
    fixture.install_child().await;
    let mut child = fixture.manager.process.write().await.take().unwrap();
    let result = fixture.manager.stop_process().await;
    // Keep the native fixture reaped even if recovery unexpectedly failed.
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().unwrap();

    assert_eq!(result.unwrap(), ProcessStopKind::Forced);
    assert!(!status.success());
    assert!(!fixture.manager.pid_path().exists());
    assert!(fixture.manager.start_time.read().await.is_none());
}

#[tokio::test]
async fn stop_process_cleanup_failure_retains_owned_child_and_evidence() {
    let fixture = Fixture::new("cleanup-failure");
    fixture.install_child().await;
    let pid_path = fixture.manager.pid_path();
    let retained_path = fixture.dir.join("retained.pid");
    let record = std::fs::read(&pid_path).unwrap();
    std::fs::rename(&pid_path, &retained_path).unwrap();
    // A directory at the cleanup target fails deterministically, even as root.
    std::fs::create_dir(&pid_path).unwrap();

    let error = fixture.manager.stop_process().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot remove resolved process record")
    );
    {
        let mut process = fixture.manager.process.write().await;
        assert!(process.as_mut().unwrap().try_wait().unwrap().is_some());
    }
    assert!(fixture.manager.start_time.read().await.is_some());
    assert_eq!(std::fs::read(&retained_path).unwrap(), record);
    assert!(pid_path.is_dir());

    std::fs::remove_dir(&pid_path).unwrap();
    std::fs::rename(&retained_path, &pid_path).unwrap();
    assert_eq!(
        fixture.manager.stop_process().await.unwrap(),
        ProcessStopKind::AlreadyStopped
    );
    assert!(fixture.manager.process.read().await.is_none());
    assert!(!pid_path.exists());
}

#[tokio::test]
async fn generic_stop_discards_receipt_after_actual_stop_and_cleanup() {
    let fixture = Fixture::new("generic");
    fixture.install_child().await;
    let manager: &dyn CoreManager = &fixture.manager;
    let (): () = manager.stop().await.unwrap();
    assert!(fixture.manager.process.read().await.is_none());
    assert!(!fixture.manager.pid_path().exists());
}
