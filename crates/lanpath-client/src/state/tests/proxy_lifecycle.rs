use super::helpers::*;
use crate::sysproxy::{SystemProxy, SystemProxyOwnership};
use lanpath_common::{LanpathError, LanpathResult};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Default)]
struct FailingProxy {
    owned: AtomicBool,
    fail_apply: AtomicBool,
    fail_restore: AtomicBool,
    fail_recovery: AtomicBool,
    fail_read: AtomicBool,
    recoveries: AtomicUsize,
}

#[async_trait::async_trait]
impl SystemProxy for FailingProxy {
    async fn enable(&self, _endpoint: SocketAddr) -> LanpathResult<()> {
        self.owned.store(true, Ordering::SeqCst);
        if self.fail_apply.load(Ordering::SeqCst) {
            Err(LanpathError::Client("partial apply".into()))
        } else { Ok(()) }
    }
    async fn disable(&self) -> LanpathResult<()> {
        if self.fail_restore.load(Ordering::SeqCst) {
            return Err(LanpathError::Client("restore conflict".into()));
        }
        self.owned.store(false, Ordering::SeqCst);
        Ok(())
    }
    async fn is_enabled(&self) -> LanpathResult<bool> {
        if self.fail_read.load(Ordering::SeqCst) {
            Err(LanpathError::Client("read denied".into()))
        } else { Ok(self.owned.load(Ordering::SeqCst)) }
    }
    async fn recover_abandoned(&self) -> LanpathResult<()> {
        self.recoveries.fetch_add(1, Ordering::SeqCst);
        if self.fail_recovery.load(Ordering::SeqCst) {
            Err(LanpathError::Client("abandoned journal conflict".into()))
        } else { Ok(()) }
    }
    fn ownership(&self) -> LanpathResult<SystemProxyOwnership> {
        if self.fail_read.load(Ordering::SeqCst) {
            return Err(LanpathError::Client("ownership read denied".into()));
        }
        Ok(if self.owned.load(Ordering::SeqCst) {
            if self.fail_restore.load(Ordering::SeqCst) { SystemProxyOwnership::RecoveryRequired }
            else { SystemProxyOwnership::Owned }
        } else { SystemProxyOwnership::Unowned })
    }
}

fn core_config(port: u16) -> serde_json::Value {
    serde_json::json!({
        "inbounds": [{"type": "mixed", "tag": "main-in", "listen": "127.0.0.1", "listen_port": port}],
        "outbounds": [{"type": "direct", "tag": "direct"}]
    })
}

#[tokio::test]
async fn restore_failure_retains_live_core_and_successful_retry_stops_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = test_config(&dir, String::new());
    config.clash_api_enabled = false;
    config.system_proxy_enabled = true;
    let port = config.mixed_port;
    let proxy = Arc::new(FailingProxy::default());
    let mut client = ClientState::with_system_proxy(config, proxy.clone());
    client.start_services(&core_config(port)).await.unwrap();
    proxy.fail_restore.store(true, Ordering::SeqCst);
    assert!(client.stop().await.is_err());
    let status = client.status().await;
    assert!(status.core_running);
    assert_eq!(status.system_proxy_ownership, Some(SystemProxyOwnership::RecoveryRequired));
    assert!(tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok());
    proxy.fail_restore.store(false, Ordering::SeqCst);
    client.stop().await.unwrap();
    assert!(!client.status().await.core_running);
    assert!(!proxy.owned.load(Ordering::SeqCst));
}

#[tokio::test]
async fn partial_apply_rollback_failure_does_not_destroy_the_referenced_core() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = test_config(&dir, String::new());
    config.clash_api_enabled = false;
    config.system_proxy_enabled = true;
    let port = config.mixed_port;
    let proxy = Arc::new(FailingProxy::default());
    proxy.fail_apply.store(true, Ordering::SeqCst);
    proxy.fail_restore.store(true, Ordering::SeqCst);
    let mut client = ClientState::with_system_proxy(config, proxy.clone());
    assert!(client.start_services(&core_config(port)).await.is_err());
    assert!(client.status().await.core_running);
    assert!(proxy.owned.load(Ordering::SeqCst));
    proxy.fail_restore.store(false, Ordering::SeqCst);
    client.stop().await.unwrap();
    assert!(!client.status().await.core_running);
}

#[tokio::test]
async fn abandoned_recovery_error_prevents_starting_or_reaping_a_core() {
    let dir = tempfile::tempdir().unwrap();
    let capture = dir.path().join("started-config.json");
    let mut config = test_config(&dir, String::new());
    config.core_binary = native_core_capturing_args(&dir, &capture);
    config.save().unwrap();
    let proxy = Arc::new(FailingProxy::default());
    proxy.fail_recovery.store(true, Ordering::SeqCst);
    let mut client = ClientState::with_system_proxy(config, proxy.clone());
    assert!(client.start().await.is_err());
    assert_eq!(proxy.recoveries.load(Ordering::SeqCst), 1);
    assert!(!capture.exists());
    assert!(!client.status().await.core_running);
}

#[tokio::test]
async fn failed_status_reads_remain_unknown_and_expose_the_error() {
    let dir = tempfile::tempdir().unwrap();
    let proxy = Arc::new(FailingProxy::default());
    proxy.fail_read.store(true, Ordering::SeqCst);
    let client = ClientState::with_system_proxy(test_config(&dir, String::new()), proxy);
    let status = client.status().await;
    assert_eq!(status.system_proxy, None);
    assert_eq!(status.system_proxy_ownership, None);
    assert!(status.system_proxy_error.is_some());
}

#[tokio::test]
async fn core_without_a_listening_entry_is_not_reported_ready() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = test_config(&dir, String::new());
    config.clash_api_enabled = false;
    let proxy = Arc::new(FailingProxy::default());
    let mut client = ClientState::with_system_proxy(config, proxy.clone());
    assert!(client.start_services(&serde_json::json!({"outbounds": []})).await.is_err());
    assert!(!client.status().await.core_running);
    assert!(!proxy.owned.load(Ordering::SeqCst));
}
