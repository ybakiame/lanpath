//! 备份业务测试：真实文件 + 真实 age 加密/解密，不 mock 成功。
//! 恢复事务经共享 `apply_pending_at_startup` 模拟重启（headless/单元
//! 测试无法跨进程重启；真实进程重启语义由 DataRoot::open 内的同一
//! 函数承担）。网络用例只连本线程 127.0.0.1 测试服务器。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use lanpath_client::RequestProxyMode;
use lanpath_client::backup;
use lanpath_common::CoreType;

use super::Backup;
use crate::core_fixture;
use crate::core_runtime::CoreRuntime;
use crate::profiles::Profiles;
use crate::request_settings::{RequestSettings, RequestSettingsDocument};
use crate::subscriptions::Subscriptions;

const PASSWORD: &str = "correct horse battery";
const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a free port probe");
    listener.local_addr().expect("probe address").port()
}

fn subscription_names(root: &Path) -> Vec<String> {
    lanpath_client::SubscriptionStore::new(root.to_path_buf())
        .load()
        .expect("load subscriptions")
        .into_iter()
        .map(|subscription| subscription.name)
        .collect()
}

/// 延迟 HTTP 服务器：任何请求都延迟后回固定 YAML（在途刷新屏障用例）。
struct SlowServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl SlowServer {
    fn new(delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind slow server");
        listener.set_nonblocking(true).expect("set non-blocking");
        let addr = listener.local_addr().expect("slow server address");
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        std::thread::spawn(move || handle_slow(stream, delay));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Self { addr, stop }
    }

    fn url(&self) -> String {
        format!("http://{addr}/sub.yaml", addr = self.addr)
    }
}

impl Drop for SlowServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn handle_slow(mut stream: TcpStream, delay: Duration) {
    let mut head = [0_u8; 4096];
    let _ = stream.read(&mut head);
    std::thread::sleep(delay);
    let body = YAML.as_bytes();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/yaml\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
        len = body.len()
    );
    let mut bytes = response.into_bytes();
    bytes.extend_from_slice(body);
    let _ = stream.write_all(&bytes);
}

#[test]
fn export_roundtrips_through_real_age_and_rejects_weak_password() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let service = Backup::for_root(root).expect("open backup service");
    Subscriptions::for_root(root)
        .expect("open subscriptions")
        .add("订阅甲", "https://example.com/a.yaml", None)
        .expect("add subscription");
    Profiles::for_root(root)
        .expect("open profiles")
        .save(None, "模板甲", String::new(), String::new(), None, None)
        .expect("save profile");

    let encrypted = service
        .export_encrypted(PASSWORD)
        .expect("export encrypted backup");
    assert!(encrypted.starts_with(b"age-encryption.org"));

    // 真实 age 解密（共享 API）：摘要与清单内容一致。
    let snapshot = backup::decode(&encrypted, PASSWORD).expect("decrypt exported backup");
    assert_eq!(snapshot.summary.subscriptions, 1);
    assert_eq!(snapshot.summary.profiles, 1);
    assert!(snapshot.summary.files > 0);

    // 错误密码：共享层只回脱敏错误。
    assert!(backup::decode(&encrypted, "wrong-password-000").is_err());
    // 弱口令：导出前明确拒绝。
    assert!(service.export_encrypted("short").is_err());
}

#[test]
fn write_export_never_overwrites_and_cleans_up_partial() {
    let dir = tempfile::tempdir().expect("tempdir");
    let service = Backup::for_root(dir.path()).expect("open backup service");
    let bytes = service
        .export_encrypted(PASSWORD)
        .expect("export encrypted backup");

    let target = dir.path().join("chosen-backup.zip.age");
    service.write_export(&target, &bytes).expect("first write");
    let content = std::fs::read(&target).expect("read back");
    assert_eq!(content, bytes);

    // 已有文件：真实错误，绝不被覆盖。
    assert!(service.write_export(&target, &bytes).is_err());
    assert_eq!(std::fs::read(&target).expect("intact"), content);

    // 目标目录不存在：失败且不留下半成品。
    let missing = dir.path().join("missing").join("backup.zip.age");
    assert!(service.write_export(&missing, &bytes).is_err());
    assert!(!missing.exists());
    assert!(!missing.parent().expect("parent").exists());
}

#[test]
fn read_import_enforces_the_archive_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let service = Backup::for_root(dir.path()).expect("open backup service");

    // 稀疏文件超过 MAX_ARCHIVE：按元数据拒绝，不读入内存。
    let oversized = dir.path().join("oversized.zip.age");
    let file = std::fs::File::create(&oversized).expect("create sparse file");
    file.set_len(backup::MAX_ARCHIVE as u64 + 1)
        .expect("set sparse length");
    drop(file);
    let error = service
        .read_import(&oversized)
        .expect_err("oversized import rejected");
    assert!(format!("{error}").contains("上限"), "{error}");

    // 正常小文件可读。
    let small = dir.path().join("small.zip.age");
    let fixture = b"age-encryption.org fixture";
    std::fs::write(&small, fixture).expect("write small file");
    assert_eq!(
        service.read_import(&small).expect("read small").len(),
        fixture.len()
    );
}

#[test]
fn inspect_sanitizes_wrong_password_and_preview_is_single_use() {
    let dir = tempfile::tempdir().expect("tempdir");
    let service = Backup::for_root(dir.path()).expect("open backup service");
    Subscriptions::for_root(dir.path())
        .expect("open subscriptions")
        .add("订阅甲", "https://example.com/a.yaml", None)
        .expect("add subscription");
    let encrypted = service
        .export_encrypted(PASSWORD)
        .expect("export encrypted backup");

    // 错误密码：统一脱敏文案，不含明文/口令细节（Ok 侧无 Debug，先 match）。
    let error = match service.inspect(&encrypted, "wrong-password-000".to_owned()) {
        Ok(_) => panic!("wrong password must not decrypt"),
        Err(error) => error,
    };
    let message = format!("{error}");
    assert!(message.contains("无法解密或校验"), "{message}");
    assert!(!message.contains("wrong-password"), "{message}");

    // 正确密码：返回一次性 preview_id；丢弃后立即失效。
    let preview = service
        .inspect(&encrypted, PASSWORD.to_owned())
        .expect("inspect with right password");
    assert_eq!(preview.summary.subscriptions, 1);
    let first_id = preview.preview_id.clone();
    service.discard(&first_id);
    assert!(
        service.take_preview(&first_id).is_err(),
        "discard 后不可消费"
    );

    // 二次 inspect 生成新 id；take 消费后不可再用（一次性）。
    let second = service
        .inspect(&encrypted, PASSWORD.to_owned())
        .expect("second inspect");
    assert_ne!(second.preview_id, first_id);
    let input = service
        .take_preview(&second.preview_id)
        .expect("consume preview");
    assert!(
        service.take_preview(&second.preview_id).is_err(),
        "preview 一次性"
    );
    drop(input);
}

#[tokio::test]
async fn prepare_waits_for_in_flight_refresh_before_capturing_rollback() {
    let delay = Duration::from_millis(500);
    let server = SlowServer::new(delay);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let subscriptions = Subscriptions::for_root(root).expect("open subscriptions");
    let record = subscriptions
        .add("慢订阅", &server.url(), None)
        .expect("add slow subscription");
    let id = record.id;

    // 导入所用备份：当前清单的快照（刷新前 node_count=0）。
    let service = Backup::for_root(root).expect("open backup service");
    let encrypted = service
        .export_encrypted(PASSWORD)
        .expect("export encrypted backup");
    let preview = service
        .inspect(&encrypted, PASSWORD.to_owned())
        .expect("inspect");
    let runtime = CoreRuntime::for_root(root).expect("open runtime");

    // 在途刷新持活动读保护跨网络 await；恢复准备必须等它真正结束。
    let started = Instant::now();
    let refresh = subscriptions.refresh(id);
    std::thread::sleep(Duration::from_millis(50));
    let prepare = service.prepare_confirmed_restore(&preview.preview_id, &runtime);
    let (refresh_result, prepare_result) = tokio::join!(refresh, prepare);
    assert!(
        started.elapsed() >= delay,
        "准备恢复不得早于在途刷新结束（屏障失效）：{:?}",
        started.elapsed()
    );
    assert!(
        matches!(
            refresh_result.expect("refresh result"),
            crate::subscriptions::RefreshStatus::Updated { node_count: 2, .. }
        ),
        "在途刷新必须真实写回"
    );
    prepare_result.expect("prepare after refresh settles");

    // 回滚快照捕获发生在刷新写回之后：当前清单带 2 个节点。
    let toml = String::from_utf8(
        std::fs::read(root.join("subscriptions.toml")).expect("read subscriptions.toml"),
    )
    .expect("utf8");
    assert!(toml.contains("node_count = 2"), "{toml}");
    assert!(root.join(".backup-pending").is_dir());
}

#[tokio::test]
async fn confirmed_prepare_blocks_writes_until_startup_applies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let service = Backup::for_root(root).expect("open backup service");
    let subscriptions = Subscriptions::for_root(root).expect("open subscriptions");
    subscriptions
        .add("订阅甲", "https://example.com/a.yaml", None)
        .expect("add subscription");
    let encrypted = service
        .export_encrypted(PASSWORD)
        .expect("export encrypted backup");
    subscriptions
        .add("订阅乙", "https://example.com/b.yaml", None)
        .expect("add second subscription");

    let preview = service
        .inspect(&encrypted, PASSWORD.to_owned())
        .expect("inspect");
    let runtime = CoreRuntime::for_root(root).expect("open runtime");
    let rollback = service
        .prepare_confirmed_restore(&preview.preview_id, &runtime)
        .await
        .expect("prepare restore");
    assert!(rollback.is_file(), "回滚备份真实存在：{rollback:?}");
    assert!(root.join(".backup-pending").is_dir());
    assert!(service.is_pending());

    // 待重启期间一切业务写被拒（读仍允许）。
    let blocked = subscriptions
        .add("订阅丙", "https://example.com/c.yaml", None)
        .expect_err("pending 拒绝新增订阅");
    assert!(format!("{blocked}").contains("已准备恢复备份"), "{blocked}");
    assert!(
        Profiles::for_root(root)
            .expect("open profiles")
            .save(None, "模板丙", String::new(), String::new(), None, None)
            .is_err()
    );
    assert!(
        RequestSettings::for_root(root)
            .expect("open request settings")
            .save(&RequestSettingsDocument::new(
                RequestProxyMode::None,
                String::new(),
                String::new()
            ))
            .is_err()
    );
    assert!(subscriptions.refresh(uuid::Uuid::new_v4()).await.is_err());
    // 读操作允许。
    assert_eq!(subscription_names(root), vec!["订阅甲", "订阅乙"]);

    // 禁止重复 prepare。
    let again = service
        .prepare_confirmed_restore(&preview.preview_id, &runtime)
        .await
        .expect_err("已有待恢复时重复准备必须拒绝");
    assert!(format!("{again}").contains("已有待恢复"), "{again}");

    // 模拟重启：启动事务原子应用 pending（与 DataRoot::open 同一函数）。
    backup::apply_pending_at_startup(root).expect("startup apply");
    assert!(!root.join(".backup-pending").exists());
    assert_eq!(subscription_names(root), vec!["订阅甲"]);
    let summary = service
        .restored()
        .expect("restored result")
        .expect("applied summary");
    assert_eq!(summary.subscriptions, 1);

    // 确认提示后 acknowledge 清除标记；写恢复。
    service.acknowledge().expect("acknowledge");
    assert!(service.restored().expect("restored cleared").is_none());
    subscriptions
        .add("订阅丙", "https://example.com/c.yaml", None)
        .expect("writes allowed again after startup apply");
}

#[tokio::test]
async fn failed_prepare_leaves_no_pending_and_allows_retry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let service = Backup::for_root(root).expect("open backup service");
    Subscriptions::for_root(root)
        .expect("open subscriptions")
        .add("订阅甲", "https://example.com/a.yaml", None)
        .expect("add subscription");
    let encrypted = service
        .export_encrypted(PASSWORD)
        .expect("export encrypted backup");
    let runtime = CoreRuntime::for_root(root).expect("open runtime");

    // 无效 preview_id：消费授权失败即返回，不遗留 pending。
    let error = service
        .prepare_confirmed_restore("not-a-real-preview", &runtime)
        .await
        .expect_err("invalid preview rejected");
    assert!(format!("{error}").contains("过期或已使用"), "{error}");
    assert!(!service.is_pending());
    assert!(!root.join(".backup-pending").exists());

    // 失败后允许重试：重新解密拿新授权，准备成功。
    let preview = service
        .inspect(&encrypted, PASSWORD.to_owned())
        .expect("inspect again");
    service
        .prepare_confirmed_restore(&preview.preview_id, &runtime)
        .await
        .expect("retry prepare succeeds");
    assert!(service.is_pending());
}

#[tokio::test]
async fn prepare_stops_a_running_core_before_pending() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let runtime = CoreRuntime::for_root_with_timeouts(
        root,
        Duration::from_secs(3),
        Duration::from_secs(2),
        Duration::from_secs(5),
    )
    .expect("open runtime");
    seed_cached_subscription(root);
    let mixed_port = free_port();
    let api_port = free_port();
    let binary = core_fixture::seed_inventory_core(root, CoreType::SingBox, "1.2.3");
    let mut config = crate::request_settings::load_request_config(root).expect("load config");
    config.mixed_port = mixed_port;
    config.clash_api_port = api_port;
    config.core_binary = binary;
    config.save().expect("save config");

    runtime.start().await.expect("start fixture core");
    assert!(runtime.is_running_sync());

    let service = Backup::for_root(root).expect("open backup service");
    let encrypted = service
        .export_encrypted(PASSWORD)
        .expect("export encrypted backup");
    let preview = service
        .inspect(&encrypted, PASSWORD.to_owned())
        .expect("inspect");
    service
        .prepare_confirmed_restore(&preview.preview_id, &runtime)
        .await
        .expect("prepare while core running");

    // 核心已被停止（含非 Windows 的空恢复路径），端口释放，pending 落下。
    assert!(!runtime.is_running_sync());
    tokio::net::TcpStream::connect(("127.0.0.1", mixed_port))
        .await
        .expect_err("mixed port released after prepare");
    assert!(root.join(".backup-pending").is_dir());
}

#[test]
fn corrupted_pending_fails_for_real_and_keeps_original_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let service = Backup::for_root(root).expect("open backup service");
    Subscriptions::for_root(root)
        .expect("open subscriptions")
        .add("原订阅", "https://example.com/original.yaml", None)
        .expect("add subscription");

    // 用另一个根做一个真实备份，直接经共享 prepare_restore 落 pending
    // （不经本服务句柄，模拟崩溃后残留的待恢复状态）。
    let other = tempfile::tempdir().expect("other tempdir");
    let other_service = Backup::for_root(other.path()).expect("open other backup");
    let encrypted = other_service
        .export_encrypted(PASSWORD)
        .expect("export other backup");
    let snapshot = backup::decode(&encrypted, PASSWORD).expect("decode snapshot");
    backup::prepare_restore(root, snapshot, PASSWORD, None).expect("stage pending");

    // 损坏 pending（版本不受支持）：启动事务必须真实失败并保留原配置。
    let manifest = r#"{"version":99,"app_version":"0.0.0","platform":"linux","created_at":"2026-11-13T00:00:00Z","subscriptions":0,"profiles":0,"files":0,"bytes":0,"appearance":null}"#;
    std::fs::write(root.join(".backup-pending").join("manifest.json"), manifest)
        .expect("corrupt manifest");
    backup::apply_pending_at_startup(root).expect("startup reports failure as state");
    assert!(root.join(".backup-failed").is_file());
    assert!(!root.join(".backup-pending").exists());

    // 原配置完好；restored() 返回真实错误，确认后可清除。
    assert_eq!(subscription_names(root), vec!["原订阅"]);
    let error = service
        .restored()
        .expect_err(".backup-failed surfaces as real error");
    assert!(format!("{error}").contains("恢复未完成"), "{error}");
    service.acknowledge().expect("acknowledge failure state");
    assert!(service.restored().expect("cleared").is_none());
}

/// 新增已启用订阅并写入原文/节点缓存（合成不联网；核心启动前置）。
fn seed_cached_subscription(root: &Path) {
    let store = lanpath_client::SubscriptionStore::new(root.to_path_buf());
    let subscription = store
        .add("运行订阅", "https://example.com/run.yaml", true, None)
        .expect("add subscription");
    let fetched = lanpath_client::parse_subscription_body(YAML, None).expect("parse fixture yaml");
    store
        .write_source(
            subscription.id,
            &lanpath_client::SubscriptionSource {
                format: fetched.format,
                content: fetched.source.clone(),
            },
        )
        .expect("write source");
    store
        .write_cached_content(
            subscription.id,
            &lanpath_client::CachedSubscriptionContent {
                format: fetched.format,
                singbox_nodes: fetched.singbox_nodes,
            },
        )
        .expect("write cache");
    let mut config = crate::request_settings::load_request_config(root).expect("load config");
    config.active_subscription_id = Some(subscription.id);
    config.save().expect("save active subscription");
}
