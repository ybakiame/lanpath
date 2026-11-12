//! 核心运行时测试：真实假核心二进制（`tests/fixtures/core_fixture.rs`，
//! rustc 现场编译——真实进程身份、真实监听、真实退出码）走完整的
//! 校验 → spawn → pid 身份 → 监听就绪 → 回收链路。每个用例独立临时数据根
//! 与独立端口（并行不冲突），不依赖互联网。这是 fixture 核心，不等于
//! Windows 真实 sing-box/Mihomo 验收（由主代理执行）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lanpath_client::{
    CachedSubscriptionContent, ClientConfig, SubscriptionSource, SubscriptionStore,
    parse_subscription_body,
};
use lanpath_common::{CoreType, ProcessStopKind};

use super::*;
use crate::core_fixture;

const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";

/// 场景夹具：独立数据根 + 注入短超时的运行时（失败用例不真的等 15/20 秒）。
struct TestCore {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runtime: Arc<CoreRuntime>,
}

fn test_core() -> TestCore {
    test_core_with_api_timeout(Duration::from_secs(2))
}

fn test_core_with_api_timeout(api_timeout: Duration) -> TestCore {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    let runtime = CoreRuntime::for_root_with_timeouts(
        &root,
        Duration::from_secs(3),
        api_timeout,
        Duration::from_secs(5),
    )
    .expect("open runtime");
    TestCore {
        _dir: dir,
        root,
        runtime,
    }
}

/// 读改写本根 `app.toml`（经共享 `load_for_requests` 语义）。
fn edit_config(root: &Path, edit: impl FnOnce(&mut ClientConfig)) {
    let mut config = crate::request_settings::load_request_config(root).expect("load config");
    edit(&mut config);
    config.save().expect("save config");
}

/// 新增已启用订阅并设为当前生效（含原文与节点缓存，合成不联网）。
fn seed_subscription(root: &Path) {
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

/// 种子 sing-box 假核心并保存选择（mixed/api 端口按场景独立分配）。
fn seed_singbox(core: &TestCore, mixed_port: u16, api_port: u16) -> PathBuf {
    let binary = core_fixture::seed_inventory_core(&core.root, CoreType::SingBox, "1.2.3");
    edit_config(&core.root, |config| {
        config.mixed_port = mixed_port;
        config.clash_api_port = api_port;
        config.core_binary = binary.clone();
    });
    binary
}

fn behavior_sidecar(binary: &Path, behavior: serde_json::Value) {
    std::fs::write(
        binary.with_extension("behavior"),
        serde_json::to_vec(&behavior).unwrap(),
    )
    .expect("rewrite behavior sidecar");
}

/// happy path：真实二进制校验 → 真实进程 → mixed/API 真实监听 → 停止回收。
#[tokio::test]
async fn start_validates_launches_and_stops_singbox() {
    let core = test_core();
    seed_subscription(&core.root);
    seed_singbox(&core, 18101, 18102);

    let outcome = core.runtime.start().await.expect("start");
    assert_eq!(outcome.core_type, CoreType::SingBox);
    assert_eq!(outcome.version, "1.2.3");
    assert_eq!(outcome.mixed_port, 18101);
    assert_eq!(outcome.clash_api_port, Some(18102));
    assert_eq!(outcome.node_count, 2);

    // 真实监听：mixed 端口可连接（fixture 核心已绑定）。
    tokio::net::TcpStream::connect(("127.0.0.1", 18101))
        .await
        .expect("mixed listener is real");
    let status = core.runtime.status().await;
    assert!(status.running, "status 必须真实反映运行中:{status:?}");
    let session = status.session.expect("running session");
    assert_eq!(session.core_type, CoreType::SingBox);
    assert_eq!(session.mixed_port, 18101);

    let kind = core.runtime.stop().await.expect("stop");
    assert_eq!(kind, ProcessStopKind::Forced);
    assert!(!core.runtime.status().await.running);
    tokio::net::TcpStream::connect(("127.0.0.1", 18101))
        .await
        .expect_err("停止后端口必须释放");
}

/// 未选择核心：明确错误，不 panic、不 spawn。
#[tokio::test]
async fn start_requires_a_selected_core() {
    let core = test_core();
    seed_subscription(&core.root);
    let error = core.runtime.start().await.expect_err("no core");
    assert!(error.to_string().contains("尚未选择核心"), "{error}");
    assert!(!core.root.join("sing-box.pid").exists());
}

/// 本切片未授权 TUN / 系统代理：已保存设置直接拒绝，设置保留。
#[tokio::test]
async fn start_rejects_tun_and_system_proxy() {
    let core = test_core();
    seed_subscription(&core.root);
    seed_singbox(&core, 18121, 18122);
    edit_config(&core.root, |config| config.tun_enabled = true);
    let error = core.runtime.start().await.expect_err("tun rejected");
    assert!(error.to_string().contains("未授权 TUN"), "{error}");
    edit_config(&core.root, |config| {
        config.tun_enabled = false;
        config.system_proxy_enabled = true;
    });
    let error = core.runtime.start().await.expect_err("sysproxy rejected");
    assert!(error.to_string().contains("未授权系统代理"), "{error}");
}

/// 端口被占用：预检真实报错（含端口），不产生任何核心进程。
#[tokio::test]
async fn start_preflight_rejects_occupied_port_without_spawning() {
    let core = test_core();
    seed_subscription(&core.root);
    seed_singbox(&core, 18131, 18132);
    let _listener = std::net::TcpListener::bind(("127.0.0.1", 18131)).expect("occupy port");
    let error = core.runtime.start().await.expect_err("occupied");
    assert!(error.to_string().contains("18131"), "{error}");
    assert!(!core.runtime.status().await.running);
    assert!(!core.root.join("sing-box.pid").exists());
}

/// 真实二进制校验失败（fixture check 退出 1）：错误带核心诊断，不 spawn。
#[tokio::test]
async fn start_fails_when_real_binary_check_fails() {
    let core = test_core();
    seed_subscription(&core.root);
    let binary = core_fixture::seed_inventory_core_with_behavior(
        &core.root,
        CoreType::SingBox,
        "1.2.3",
        &serde_json::json!({ "fixture_fail": "boom-check-failed" }),
    );
    edit_config(&core.root, |config| {
        config.mixed_port = 18141;
        config.clash_api_port = 18142;
        config.core_binary = binary;
    });
    let error = core.runtime.start().await.expect_err("check fails");
    let message = error.to_string();
    assert!(message.contains("核心配置校验失败"), "{message}");
    assert!(message.contains("boom-check-failed"), "{message}");
    assert!(!core.runtime.status().await.running);
    assert!(!core.root.join("sing-box.pid").exists());
}

/// 启动后始终未监听：等待超时 → 回滚回收（pid 记录清理、状态可重试），
/// 再次启动成功。
#[tokio::test]
async fn start_rolls_back_when_listener_never_ready_and_retries() {
    let core = test_core();
    seed_subscription(&core.root);
    let binary = core_fixture::seed_inventory_core_with_behavior(
        &core.root,
        CoreType::SingBox,
        "1.2.3",
        &serde_json::json!({ "fixture_no_listen": true }),
    );
    edit_config(&core.root, |config| {
        config.mixed_port = 18151;
        config.clash_api_port = 18152;
        config.core_binary = binary.clone();
    });
    let error = core.runtime.start().await.expect_err("listener timeout");
    assert!(error.to_string().contains("等待代理监听"), "{error}");
    assert!(
        !core.root.join("sing-box.pid").exists(),
        "回滚必须清理 pid 身份记录"
    );
    assert!(!core.runtime.status().await.running);

    // 可重试：同一快照再次启动（行为恢复正常）真实成功。
    behavior_sidecar(&binary, serde_json::json!({}));
    let outcome = core.runtime.start().await.expect("retry start");
    assert_eq!(outcome.mixed_port, 18151);
    core.runtime.stop().await.expect("stop");
}

/// 状态 poll 发现核心异常退出（外部强杀）：记录异常、清理会话、可再启动。
#[tokio::test]
async fn status_poll_detects_unexpected_exit_and_recovers() {
    let core = test_core();
    seed_subscription(&core.root);
    let binary = core_fixture::seed_inventory_core_with_behavior(
        &core.root,
        CoreType::SingBox,
        "1.2.3",
        &serde_json::json!({ "fixture_exit_after_ms": 60_000 }),
    );
    edit_config(&core.root, |config| {
        config.mixed_port = 18161;
        config.clash_api_port = 18162;
        config.core_binary = binary;
    });
    core.runtime.start().await.expect("start");
    let identity = read_pid_identity(&core.root, CoreType::SingBox).expect("pid identity");
    identity.terminate_verified().expect("external kill");

    let deadline = Instant::now() + Duration::from_secs(5);
    let exit_error = loop {
        let status = core.runtime.status().await;
        if !status.running {
            break status.unexpected_exit;
        }
        assert!(Instant::now() < deadline, "poll 必须及时发现异常退出");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let exit_error = exit_error.expect("异常退出必须被记录");
    assert!(exit_error.contains("异常退出"), "{exit_error}");

    // 异常退出后可再次启动（残留已清）。
    let outcome = core.runtime.start().await.expect("restart after exit");
    assert_eq!(outcome.core_type, CoreType::SingBox);
    core.runtime.stop().await.expect("stop");
}

/// 启停与库存互斥：运行中 select/download/delete 被业务层真实拒绝（不是
/// UI 状态）；停止后恢复可操作。
#[tokio::test]
async fn inventory_mutations_are_rejected_while_running() {
    let core = test_core();
    seed_subscription(&core.root);
    seed_singbox(&core, 18171, 18172);
    core.runtime.start().await.expect("start");
    let inventory = CoreInventory::for_root(&core.root).expect("inventory");
    let other = core_fixture::seed_inventory_core(&core.root, CoreType::SingBox, "1.2.2");

    let error = inventory
        .select(CoreType::SingBox, &other)
        .expect_err("select rejected");
    assert!(
        error
            .to_string()
            .contains("核心运行期间不可修改核心文件或选择"),
        "{error}"
    );
    let error = inventory
        .delete(CoreType::SingBox, &other)
        .expect_err("delete rejected");
    assert!(error.to_string().contains("核心运行期间"), "{error}");
    let error = inventory
        .download(CoreType::SingBox, "9.9.9", &|_| {})
        .await
        .expect_err("download rejected");
    assert!(error.to_string().contains("核心运行期间"), "{error}");

    core.runtime.stop().await.expect("stop");
    inventory
        .select(CoreType::SingBox, &other)
        .expect("select works after stop");
    let config = crate::request_settings::load_request_config(&core.root).expect("load config");
    assert_eq!(config.core_binary, other);
}

/// 启动/停止与库存 mutation 同锁：在途 mutation 期间启动/停止立即 busy。
#[tokio::test]
async fn start_and_stop_are_busy_while_inventory_mutation_is_in_flight() {
    let core = test_core();
    seed_subscription(&core.root);
    seed_singbox(&core, 18181, 18182);
    let inventory = CoreInventory::for_root(&core.root).expect("inventory");
    let _guard = inventory.try_mutation().expect("hold mutation lock");
    let error = core.runtime.start().await.expect_err("start busy");
    assert!(
        error
            .to_string()
            .contains(crate::core_inventory::MUTATION_BUSY),
        "{error}"
    );
    let error = core.runtime.stop().await.expect_err("stop busy");
    assert!(
        error
            .to_string()
            .contains(crate::core_inventory::MUTATION_BUSY),
        "{error}"
    );
}

/// 空闲时停止：AlreadyStopped，不报错。
#[tokio::test]
async fn stop_when_idle_reports_already_stopped() {
    let core = test_core();
    let kind = core.runtime.stop().await.expect("idle stop");
    assert_eq!(kind, ProcessStopKind::AlreadyStopped);
}

/// GUI 正常退出路径：运行时 drop 按记录的 OS 身份回收本次子进程（端口
/// 真实释放；OS 父子绑定是兜底）。
#[tokio::test]
async fn dropping_the_runtime_reaps_the_child_process() {
    let core = test_core();
    seed_subscription(&core.root);
    seed_singbox(&core, 18201, 18202);
    core.runtime.start().await.expect("start");
    drop(core.runtime);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if std::net::TcpListener::bind(("127.0.0.1", 18201)).is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "drop 必须回收子进程释放端口");
        std::thread::sleep(Duration::from_millis(100));
    }
    // 端口释放即进程死亡的充分证据（zombie 不持有监听socket）。
}

/// Mihomo：订阅配置合成 YAML → 校验 → 真实启动 mixed/API → 停止。
#[tokio::test]
async fn start_and_stop_mihomo_with_subscription_config() {
    let core = test_core();
    seed_subscription(&core.root);
    let binary = core_fixture::seed_inventory_core(&core.root, CoreType::Mihomo, "1.19.29");
    edit_config(&core.root, |config| {
        config.mixed_port = 18211;
        config.clash_api_port = 18212;
        config.core_binary = binary;
    });
    let outcome = core.runtime.start().await.expect("mihomo start");
    assert_eq!(outcome.core_type, CoreType::Mihomo);
    assert_eq!(outcome.version, "1.19.29");
    assert_eq!(outcome.clash_api_port, Some(18212));
    tokio::net::TcpStream::connect(("127.0.0.1", 18211))
        .await
        .expect("mihomo mixed listener is real");
    let kind = core.runtime.stop().await.expect("stop");
    assert_eq!(kind, ProcessStopKind::Forced);
}

/// 启动阶段失败也走本层回滚：引擎写 pid 身份记录失败（fixture 预置损坏
/// 记录使 create_new 冲突）并已自行回收子进程；本层 rollback 的
/// stop_process 经 reap_stale 读到损坏记录而失败——错误必须同时报告
/// 原错误与回收错误，manager 保留为可重试状态；清理故障后停止（重试
/// 回收）与再次启动都真实成功。
#[tokio::test]
async fn start_stage_failure_rolls_back_with_combined_error_and_retries() {
    let core = test_core();
    seed_subscription(&core.root);
    let binary = core_fixture::seed_inventory_core_with_behavior(
        &core.root,
        CoreType::SingBox,
        "1.2.3",
        &serde_json::json!({ "fixture_write_bogus_pid": true }),
    );
    edit_config(&core.root, |config| {
        config.mixed_port = 18221;
        config.clash_api_port = 18222;
        config.core_binary = binary.clone();
    });

    let error = core.runtime.start().await.expect_err("start fails");
    let message = error.to_string();
    assert!(message.contains("identity record failed"), "{message}");
    assert!(message.contains("启动回收未完成"), "{message}");
    assert!(message.contains("invalid recovery identity"), "{message}");
    // 子进程已被引擎回收：端口真实释放、无 pid 身份残留。
    std::net::TcpListener::bind(("127.0.0.1", 18221)).expect("端口已释放");
    assert!(!core.runtime.status().await.running);

    // 可重试状态：清理故障记录后，停止（重试回收）成功。
    std::fs::remove_file(core.root.join("sing-box.pid")).unwrap();
    let kind = core.runtime.stop().await.expect("stop retries cleanup");
    assert_eq!(kind, ProcessStopKind::AlreadyStopped);

    // 再次启动真实成功。
    behavior_sidecar(&binary, serde_json::json!({}));
    core.runtime.start().await.expect("restart");
    core.runtime.stop().await.expect("stop");
}

/// 就绪后 pid 身份读取失败同样走回滚：子进程当时仍存活，rollback 的
/// stop_process 必须真实杀掉它（端口立即释放，而非 manager drop 在
/// Unix 上 detach 到进程退出）；无 active 残留，可重试。
#[tokio::test]
async fn identity_read_failure_after_ready_rolls_back_the_live_child() {
    // API 延迟 2.5s 绑定 + pid 记录 1.2s 后删除（引擎 ~0.5s 写记录），
    // 保证删除发生在写入之后、运行时读回之前（api 超时放宽到 5s）。
    let core = test_core_with_api_timeout(Duration::from_secs(5));
    seed_subscription(&core.root);
    let binary = core_fixture::seed_inventory_core_with_behavior(
        &core.root,
        CoreType::SingBox,
        "1.2.3",
        &serde_json::json!({
            "fixture_delete_pid_file_after_ms": 1200,
            "fixture_api_delay_ms": 2500
        }),
    );
    edit_config(&core.root, |config| {
        config.mixed_port = 18231;
        config.clash_api_port = 18232;
        config.core_binary = binary.clone();
    });

    let error = core.runtime.start().await.expect_err("identity read fails");
    let message = error.to_string();
    assert!(message.contains("无法读取进程身份记录"), "{message}");
    // 回滚真实杀掉存活子进程：端口立即重新可绑定（ early-return 的
    // manager drop 在 Unix 只 detach，子进程会继续持有端口到进程退出）。
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if std::net::TcpListener::bind(("127.0.0.1", 18231)).is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "回滚必须杀掉存活子进程");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!core.runtime.status().await.running);
    assert!(!core.root.join("sing-box.pid").exists());

    // 可重试：行为恢复后再次启动真实成功。
    behavior_sidecar(&binary, serde_json::json!({}));
    core.runtime.start().await.expect("restart");
    core.runtime.stop().await.expect("stop");
}
