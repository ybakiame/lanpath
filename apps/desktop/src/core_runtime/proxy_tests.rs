//! CoreRuntime 系统代理守护编排测试：fake spanner 按共享 session 契约
//! 建模启动/停止/异常路径与一次性接管授权（consent）语义。它是 fixture，
//! 不是 Windows 真实 WinINet（真实验收由主代理执行）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use lanpath_client::sysproxy::SystemProxyOwnership;
use lanpath_common::{CoreType, LanpathResult, ProcessStopKind};

use super::CoreRuntime;
use crate::core_fixture;
use crate::system_proxy::{ArmedGuardian, GuardianSpawner};

/// 守护调用记录 + 失败注入。覆盖启动/停止/异常路径的顺序与保留语义，
/// 以及普通 Acquire 与显式接管（takeover）的严格区分。
struct ProxyProbe {
    calls: Vec<String>,
    arm_fail: bool,
    acquire_fail: bool,
    takeover_acquire_fail: bool,
    shutdown_fail: bool,
    recover_fail: bool,
    ownership_fail: bool,
    /// spawner.ownership() 的返回值（无守护时的共享会话查询）。
    ownership: SystemProxyOwnership,
    /// has_recovery_for_owner() 的返回值。
    journal: bool,
    /// 已装配守护的 status() 返回值。
    armed_status: SystemProxyOwnership,
}

impl Default for ProxyProbe {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            arm_fail: false,
            acquire_fail: false,
            takeover_acquire_fail: false,
            shutdown_fail: false,
            recover_fail: false,
            ownership_fail: false,
            ownership: SystemProxyOwnership::Unowned,
            journal: false,
            armed_status: SystemProxyOwnership::Unowned,
        }
    }
}

struct FakeArmed {
    probe: Arc<std::sync::Mutex<ProxyProbe>>,
}

impl FakeArmed {
    fn record(&self, call: &str) {
        self.probe.lock().unwrap().calls.push(call.to_owned());
    }
}

impl ArmedGuardian for FakeArmed {
    fn acquire(&mut self, port: u16) -> LanpathResult<()> {
        self.record(&format!("acquire:{port}"));
        if self.probe.lock().unwrap().acquire_fail {
            return Err(lanpath_common::LanpathError::Client(
                "fixture foreign manual proxy refused".into(),
            ));
        }
        Ok(())
    }
    fn acquire_takeover(&mut self, port: u16) -> LanpathResult<()> {
        self.record(&format!("acquire_takeover:{port}"));
        if self.probe.lock().unwrap().takeover_acquire_fail {
            return Err(lanpath_common::LanpathError::Client(
                "fixture takeover refused".into(),
            ));
        }
        Ok(())
    }
    fn recover_abandoned(&mut self) -> LanpathResult<()> {
        self.record("recover_abandoned");
        if self.probe.lock().unwrap().recover_fail {
            return Err(lanpath_common::LanpathError::Client(
                "fixture recover refused".into(),
            ));
        }
        Ok(())
    }
    fn recover_watched(&mut self) -> LanpathResult<()> {
        self.record("recover_watched");
        if self.probe.lock().unwrap().recover_fail {
            return Err(lanpath_common::LanpathError::Client(
                "fixture recover refused".into(),
            ));
        }
        Ok(())
    }
    fn status(&mut self) -> LanpathResult<SystemProxyOwnership> {
        self.record("status");
        Ok(self.probe.lock().unwrap().armed_status.clone())
    }
    fn shutdown(&mut self) -> LanpathResult<()> {
        self.record("shutdown");
        if self.probe.lock().unwrap().shutdown_fail {
            return Err(lanpath_common::LanpathError::Client(
                "fixture shutdown failed".into(),
            ));
        }
        Ok(())
    }
}

struct FakeSpawner {
    probe: Arc<std::sync::Mutex<ProxyProbe>>,
}

impl GuardianSpawner for FakeSpawner {
    fn supported(&self) -> bool {
        true
    }
    fn arm(&self, owner: &str) -> LanpathResult<Box<dyn ArmedGuardian>> {
        self.probe
            .lock()
            .unwrap()
            .calls
            .push(format!("arm:{owner}"));
        if self.probe.lock().unwrap().arm_fail {
            return Err(lanpath_common::LanpathError::Client(
                "fixture arm failed".into(),
            ));
        }
        Ok(Box::new(FakeArmed {
            probe: self.probe.clone(),
        }))
    }
    fn ownership(&self, owner: &str) -> LanpathResult<SystemProxyOwnership> {
        self.probe
            .lock()
            .unwrap()
            .calls
            .push(format!("ownership:{owner}"));
        if self.probe.lock().unwrap().ownership_fail {
            return Err(lanpath_common::LanpathError::Client(
                "fixture ownership query failed".into(),
            ));
        }
        Ok(self.probe.lock().unwrap().ownership.clone())
    }
    fn has_recovery_for_owner(&self, owner: &str) -> LanpathResult<bool> {
        self.probe
            .lock()
            .unwrap()
            .calls
            .push(format!("has_journal:{owner}"));
        Ok(self.probe.lock().unwrap().journal)
    }
}

/// 读改写本根 `app.toml`（经共享 `load_for_requests` 语义）。
fn edit_config(root: &Path, edit: impl FnOnce(&mut lanpath_client::ClientConfig)) {
    let mut config = crate::request_settings::load_request_config(root).expect("load config");
    edit(&mut config);
    config.save().expect("save config");
}

/// 新增已启用订阅并设为当前生效（含原文与节点缓存，合成不联网）。
fn seed_subscription(root: &Path) {
    use lanpath_client::{
        CachedSubscriptionContent, SubscriptionSource, SubscriptionStore, parse_subscription_body,
    };
    const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";
    let store = SubscriptionStore::new(root.to_path_buf());
    let subscription = store
        .add("运行订阅", "https://example.com/c.yaml", true, None)
        .expect("add subscription");
    let fetched = parse_subscription_body(YAML, None).expect("parse fixture yaml");
    store
        .write_source(
            subscription.id,
            &SubscriptionSource {
                format: fetched.format,
                content: fetched.source.clone(),
            },
        )
        .expect("write source");
    store
        .write_cached_content(
            subscription.id,
            &CachedSubscriptionContent {
                format: fetched.format,
                singbox_nodes: fetched.singbox_nodes,
            },
        )
        .expect("write cache");
    edit_config(root, |config| {
        config.active_subscription_id = Some(subscription.id);
    });
}

/// 注入 fake spanner 的运行时场景。
struct ProxyCore {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runtime: Arc<CoreRuntime>,
    probe: Arc<std::sync::Mutex<ProxyProbe>>,
}

fn proxy_core() -> ProxyCore {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    let runtime = CoreRuntime::for_root_with_timeouts(
        &root,
        Duration::from_secs(3),
        Duration::from_secs(2),
        Duration::from_secs(5),
    )
    .expect("open runtime");
    let probe = Arc::new(std::sync::Mutex::new(ProxyProbe::default()));
    runtime.replace_proxy_spawner(Arc::new(FakeSpawner {
        probe: probe.clone(),
    }));
    ProxyCore {
        _dir: dir,
        root,
        runtime,
        probe,
    }
}

fn proxy_core_with_subscription(mixed_port: u16, api_port: u16) -> (ProxyCore, PathBuf) {
    let core = proxy_core();
    seed_subscription(&core.root);
    let binary = core_fixture::seed_inventory_core(&core.root, CoreType::SingBox, "1.2.3");
    edit_config(&core.root, |config| {
        config.mixed_port = mixed_port;
        config.clash_api_port = api_port;
        config.core_binary = binary.clone();
        config.system_proxy_enabled = true;
    });
    (core, binary)
}

fn calls(probe: &Arc<std::sync::Mutex<ProxyProbe>>) -> Vec<String> {
    probe.lock().unwrap().calls.clone()
}

fn acquire_kinds(probe: &Arc<std::sync::Mutex<ProxyProbe>>) -> Vec<String> {
    calls(probe)
        .into_iter()
        .filter(|call| call.starts_with("acquire"))
        .collect()
}

#[tokio::test]
async fn start_with_proxy_pref_arms_guardian_and_acquires_after_ready() {
    let (core, _binary) = proxy_core_with_subscription(18241, 18242);
    let outcome = core.runtime.start().await.expect("start");
    assert_eq!(outcome.mixed_port, 18241);
    // arm 的 owner 是本数据根 UUID；acquire 指向已保存 mixed 端口；
    // 启动前的 recover 检查先查共享会话 ownership（无遗留 → 不动作）。
    let owner = core.runtime.proxy_owner_for_test();
    assert_eq!(
        calls(&core.probe),
        vec![
            format!("ownership:{owner}"),
            format!("arm:{owner}"),
            "acquire:18241".to_owned()
        ]
    );
    // 状态查询走守护真实 ownership。
    core.probe.lock().unwrap().armed_status = SystemProxyOwnership::Owned;
    let status = core.runtime.status().await;
    assert!(status.proxy_supported);
    assert_eq!(
        status.proxy_ownership,
        Some(Ok(SystemProxyOwnership::Owned))
    );
    let kind = core.runtime.stop().await.expect("stop");
    assert_eq!(kind, ProcessStopKind::Forced);
    // 停止先守护恢复（shutdown）再停核心。
    let expected = vec![
        format!("ownership:{owner}"),
        format!("arm:{owner}"),
        "acquire:18241".to_owned(),
        "status".to_owned(),
        "shutdown".to_owned(),
        format!("has_journal:{owner}"),
    ];
    assert_eq!(calls(&core.probe), expected);
}

#[tokio::test]
async fn acquire_failure_keeps_core_running_and_preserves_evidence() {
    let (core, _binary) = proxy_core_with_subscription(18251, 18252);
    core.probe.lock().unwrap().acquire_fail = true;
    let error = core.runtime.start().await.expect_err("acquire fails");
    assert!(
        error.to_string().contains("系统代理守护接管失败"),
        "{error}"
    );
    assert!(error.to_string().contains("保留核心与恢复资源"), "{error}");
    // 核心保留运行（不 rollback、不清证据）；守护仍装配。
    assert!(core.runtime.status().await.running);
    tokio::net::TcpStream::connect(("127.0.0.1", 18251))
        .await
        .expect("mixed listener still alive");
    // 停止完成精确恢复：shutdown + owner 精确检查，核心随后回收。
    // （测试前面对 status() 的轮询也经守护真实 ownership；停止后最后的
    // status() 回落到共享会话 ownership 查询。）
    let kind = core.runtime.stop().await.expect("stop restores");
    assert_eq!(kind, ProcessStopKind::Forced);
    assert!(!core.runtime.status().await.running);
    let owner = core.runtime.proxy_owner_for_test();
    assert_eq!(
        calls(&core.probe),
        vec![
            format!("ownership:{owner}"),
            format!("arm:{owner}"),
            "acquire:18251".to_owned(),
            "status".to_owned(),
            "shutdown".to_owned(),
            format!("has_journal:{owner}"),
            format!("ownership:{owner}"),
        ]
    );
}

#[tokio::test]
async fn start_recovers_abandoned_journal_before_starting_core() {
    let (core, _binary) = proxy_core_with_subscription(18261, 18262);
    core.probe.lock().unwrap().ownership = SystemProxyOwnership::RecoveryRequired;
    core.probe.lock().unwrap().journal = true;
    core.runtime.start().await.expect("start after recovery");
    let owner = core.runtime.proxy_owner_for_test();
    // abandoned 恢复在核心启动前完成：独立守护 arm → 恢复 → 退出确认。
    let expected = vec![
        format!("ownership:{owner}"),
        format!("arm:{owner}"),
        "recover_abandoned".to_owned(),
        "shutdown".to_owned(),
        format!("arm:{owner}"),
        "acquire:18261".to_owned(),
    ];
    assert_eq!(calls(&core.probe), expected);
    core.runtime.stop().await.expect("stop");
}

#[tokio::test]
async fn recover_refusal_surfaces_error_and_keeps_evidence() {
    let (core, _binary) = proxy_core_with_subscription(18271, 18272);
    core.probe.lock().unwrap().ownership = SystemProxyOwnership::RecoveryRequired;
    core.probe.lock().unwrap().recover_fail = true;
    let error = core.runtime.start().await.expect_err("recover refused");
    assert!(
        error.to_string().contains("遗留系统代理恢复失败"),
        "{error}"
    );
    assert!(
        error.to_string().contains("fixture recover refused"),
        "{error}"
    );
    // 失败可重试：清除拒绝后再次启动成功（日志保留、未被清空）。
    core.probe.lock().unwrap().recover_fail = false;
    core.runtime.start().await.expect("retry start");
    core.runtime.stop().await.expect("stop");
}

#[tokio::test]
async fn stop_restore_failure_keeps_core_and_allows_retry() {
    let (core, _binary) = proxy_core_with_subscription(18281, 18282);
    core.runtime.start().await.expect("start");
    core.probe.lock().unwrap().shutdown_fail = true;
    let error = core.runtime.stop().await.expect_err("restore fails");
    assert!(
        error.to_string().contains("系统代理守护恢复/退出未确认"),
        "{error}"
    );
    // 核心保留运行（绝不跳过恢复直接停核心），可再次停止重试。
    assert!(core.runtime.status().await.running);
    tokio::net::TcpStream::connect(("127.0.0.1", 18281))
        .await
        .expect("core retained");
    core.probe.lock().unwrap().shutdown_fail = false;
    let kind = core.runtime.stop().await.expect("retry stop");
    assert_eq!(kind, ProcessStopKind::Forced);
    assert!(!core.runtime.status().await.running);
}

#[tokio::test]
async fn drop_attempts_guardian_restore_before_terminating_core() {
    let (core, _binary) = proxy_core_with_subscription(18291, 18292);
    core.runtime.start().await.expect("start");
    let probe = core.probe.clone();
    drop(core.runtime);
    // Drop 先尽力守护恢复（shutdown），恢复失败/成功都不阻断核心回收。
    assert!(calls(&probe).contains(&"shutdown".to_owned()));
    // 核心子进程已随运行时释放回收（端口释放）。
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if std::net::TcpListener::bind(("127.0.0.1", 18291)).is_ok() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "drop 必须回收核心进程"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[tokio::test]
async fn status_reports_probe_error_as_unknown_instead_of_unowned() {
    let core = proxy_core();
    core.probe.lock().unwrap().ownership_fail = true;
    // 查询错误如实上屏（未知错误），绝不伪造成“未接管”。
    let status = core.runtime.status().await;
    assert!(status.proxy_supported);
    let ownership = status.proxy_ownership.expect("supported");
    let error = ownership.expect_err("probe error surfaces");
    assert!(error.contains("fixture ownership query failed"), "{error}");
}

#[tokio::test]
async fn explicit_restore_uses_only_owner_exact_recovery() {
    let (core, _binary) = proxy_core_with_subscription(18301, 18302);
    core.runtime.start().await.expect("start");
    core.runtime.stop().await.expect("stop");
    // 显式恢复：无装配守护时先查 owner 精确记录，有才 arm 继任守护恢复。
    core.probe.lock().unwrap().journal = true;
    let probe = core.probe.clone();
    let before = calls(&probe).len();
    core.runtime
        .restore_system_proxy()
        .await
        .expect("explicit restore");
    let owner = core.runtime.proxy_owner_for_test();
    assert_eq!(
        calls(&probe)[before..],
        vec![
            format!("has_journal:{owner}"),
            format!("arm:{owner}"),
            "recover_watched".to_owned(),
            "shutdown".to_owned(),
        ]
    );
}

// ---------- 一次性显式接管授权（consent） ----------

/// 未确认：外来活动代理仍被共享 RequireDirect 拒绝（fixture 以普通
/// acquire 失败建模），绝不升级为 takeover。
#[tokio::test]
async fn unconfirmed_foreign_proxy_is_refused_via_plain_acquire() {
    let (core, _binary) = proxy_core_with_subscription(18311, 18312);
    core.probe.lock().unwrap().acquire_fail = true;
    let error = core.runtime.start().await.expect_err("foreign refused");
    assert!(
        error.to_string().contains("系统代理守护接管失败"),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("fixture foreign manual proxy refused")
    );
    assert_eq!(acquire_kinds(&core.probe), vec!["acquire:18311"]);
    // 核心与证据保留，可停止恢复。
    assert!(core.runtime.status().await.running);
    core.runtime.stop().await.expect("stop");
}

/// 确认一次只影响一次：首次 acquire 为 takeover，之后回到普通 Acquire。
#[tokio::test]
async fn one_consent_enables_exactly_one_takeover() {
    let (core, _binary) = proxy_core_with_subscription(18321, 18322);
    core.runtime.grant_takeover_consent();
    core.runtime.start().await.expect("takeover start");
    core.runtime.stop().await.expect("stop");
    core.runtime.start().await.expect("plain start");
    core.runtime.stop().await.expect("stop");
    assert_eq!(
        acquire_kinds(&core.probe),
        vec!["acquire_takeover:18321", "acquire:18321"]
    );
}

/// 授权在成功或失败时都消费：takeover 失败后再次启动不再携带授权。
#[tokio::test]
async fn consent_is_consumed_even_when_takeover_fails() {
    let (core, _binary) = proxy_core_with_subscription(18331, 18332);
    core.runtime.grant_takeover_consent();
    core.probe.lock().unwrap().takeover_acquire_fail = true;
    core.runtime.start().await.expect_err("takeover refused");
    core.runtime.stop().await.expect("stop retains evidence");
    // 授权已消费：再次启动走普通 Acquire 并成功。
    core.runtime.start().await.expect("plain start");
    core.runtime.stop().await.expect("stop");
    assert_eq!(
        acquire_kinds(&core.probe),
        vec!["acquire_takeover:18331", "acquire:18331"]
    );
}

/// 停用清授权：clear 后即使曾确认也回到普通 Acquire。
#[tokio::test]
async fn disabling_clears_takeover_consent() {
    let (core, _binary) = proxy_core_with_subscription(18341, 18342);
    core.runtime.grant_takeover_consent();
    core.runtime.clear_takeover_consent();
    core.runtime.start().await.expect("plain start");
    core.runtime.stop().await.expect("stop");
    assert_eq!(acquire_kinds(&core.probe), vec!["acquire:18341"]);
}

/// 授权不持久化：授予 consent 不触碰任何配置文件（新进程天然无授权，
/// 由 build() 的 AtomicBool::new(false) 保证）。
#[test]
fn consent_is_process_local_and_never_persisted() {
    let (core, _binary) = proxy_core_with_subscription(18351, 18352);
    let path = core.root.join("app.toml");
    let before = std::fs::read(&path).expect("read app.toml");
    core.runtime.grant_takeover_consent();
    assert_eq!(std::fs::read(&path).expect("reread"), before);
}
