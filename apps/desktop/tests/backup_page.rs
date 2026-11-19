//! Headless 测试（`nsh_testing::Headless`，NSH 0.5.1）：备份页导出
//! （掩码口令 ≥12 / 生成强密码 / 原生保存对话框注入 / 真实 age 字节 /
//! 不覆盖已有文件）、导入（原生打开对话框注入 / 错误密码脱敏 / 摘要 /
//! 改密码即弃预览）、恢复确认（停止核心准备 pending / 待重启横幅 /
//! 业务写拒绝 / 重启入口存在但不按下）、启动恢复结果（restored 摘要 +
//! acknowledge、.backup-failed 真实错误）。全部真实文件与真实加密，
//! 不 mock 成功；每个场景独立临时数据根。

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{TestRoute, TestServer, data_root, start_app, texts};

const PASSWORD: &str = "export-password-123";
const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";

/// 轮询直到屏幕上不再出现 `needle`（acknowledge/丢弃后的消失断言）。
fn wait_text_gone(app: &mut nsh_testing::Headless, needle: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if texts(app).iter().all(|text| !text.contains(needle)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("text still on screen: {needle}: {:?}", texts(app));
}

fn open_backup(app: &mut nsh_testing::Headless) {
    app.press("备份").unwrap();
    app.wait_for("导出加密备份").unwrap();
}

fn temp_target(name: &str) -> PathBuf {
    let target = std::env::temp_dir().join(format!(
        "lanpath-desktop-backup-test-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&target);
    target
}

/// 侧栏导航到备份页：导出/导入分区与作用域说明真实展示。
#[test]
fn navigating_to_the_backup_page_shows_export_and_import_scope() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_backup(&mut app);
    let seen = texts(&app);
    for expected in [
        "导出加密备份",
        "从备份恢复",
        "密码只保存在内存",
        "恢复需重启应用完成",
        "密码只用于加密，丢失无法找回",
    ] {
        assert!(
            seen.iter().any(|text| text.contains(expected)),
            "missing {expected}: {seen:?}"
        );
    }
}

/// 导出：弱口令拒绝上屏；强口令经注入的保存对话框写出真实 .zip.age
/// （共享 age 解密可读出摘要），成功后口令输入清空。
#[test]
fn export_writes_real_age_bytes_to_the_picked_path() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_backup(&mut app);

    // 弱口令：明确错误，不打开对话框。
    app.type_text("id:backup-export-password", "short").unwrap();
    app.press("id:backup-export").unwrap();
    app.wait_for("text:/导出失败：备份密码至少 12 个字符/")
        .unwrap();

    let target = temp_target("export.zip.age");
    app.on_pick_save_path(Some(target.clone())).unwrap();
    app.clear("id:backup-export-password").unwrap();
    app.type_text("id:backup-export-password", PASSWORD)
        .unwrap();
    app.press("id:backup-export").unwrap();
    app.wait_for_within("text:/已导出加密备份：/", Duration::from_secs(120))
        .unwrap();
    assert!(target.is_file(), "exported backup missing");
    let bytes = std::fs::read(&target).unwrap();
    assert!(bytes.starts_with(b"age-encryption.org"), "real age header");
    let snapshot = lanpath_client::backup::decode(&bytes, PASSWORD)
        .expect("exported file decrypts with the typed password");
    assert_eq!(snapshot.summary.version, 3);
    // 成功后口令输入清空：口令不留在页面状态。
    assert_eq!(
        app.screen()
            .input_text("id:backup-export-password")
            .unwrap(),
        Some("")
    );
    let _ = std::fs::remove_file(&target);
}

/// 生成强密码：一键填入共享生成器输出（24 位字母数字，满足 ≥12）。
#[test]
fn generate_password_fills_a_strong_shared_password() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_backup(&mut app);
    app.press("id:backup-generate-password").unwrap();
    app.wait_for("text:/已生成随机强密码/").unwrap();
    let generated = app
        .screen()
        .input_text("id:backup-export-password")
        .unwrap()
        .expect("generated password filled")
        .to_owned();
    assert_eq!(generated.len(), 24);
    assert!(
        generated.chars().all(|c| c.is_ascii_alphanumeric()),
        "{generated}"
    );
}

/// 导出绝不覆盖已有文件：真实错误上屏，原文件字节不动。
#[test]
fn export_never_overwrites_an_existing_file() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_backup(&mut app);

    let target = temp_target("existing.zip.age");
    std::fs::write(&target, b"sentinel-original-content").unwrap();
    app.on_pick_save_path(Some(target.clone())).unwrap();
    app.type_text("id:backup-export-password", PASSWORD)
        .unwrap();
    app.press("id:backup-export").unwrap();
    app.wait_for_within("text:/写出备份失败/", Duration::from_secs(120))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "sentinel-original-content"
    );
    let _ = std::fs::remove_file(&target);
}

/// 导入：注入打开对话框选文件 → 错误密码脱敏失败 → 正确密码展示摘要；
/// 修改密码立即放弃预览。
#[test]
fn import_previews_summary_and_discards_on_password_change() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    // 造一份真实备份：经 UI 导出当前根（0 订阅）。
    open_backup(&mut app);
    let backup_file = temp_target("import-source.zip.age");
    app.on_pick_save_path(Some(backup_file.clone())).unwrap();
    app.type_text("id:backup-export-password", PASSWORD)
        .unwrap();
    app.press("id:backup-export").unwrap();
    app.wait_for_within("text:/已导出加密备份：/", Duration::from_secs(120))
        .unwrap();

    // 导入该文件。
    app.on_pick_paths(vec![backup_file.clone()]).unwrap();
    app.press("id:backup-pick").unwrap();
    app.wait_for_within("text:/已选择：/", Duration::from_secs(10))
        .unwrap();
    assert!(
        texts(&app)
            .iter()
            .any(|text| text.contains("import-source.zip.age"))
    );

    // 错误密码：共享层脱敏错误，不含口令本身。
    app.type_text("id:backup-import-password", "wrong-password-000")
        .unwrap();
    app.press("id:backup-inspect").unwrap();
    app.wait_for_within(
        "text:/解密预览失败:.*备份无法解密或校验/",
        Duration::from_secs(120),
    )
    .unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter().all(|text| !text.contains("wrong-password-000")),
        "screen leaked the password: {seen:?}"
    );

    // 正确密码：摘要上屏（版本/时间/订阅/覆写/文件/字节）。
    app.clear("id:backup-import-password").unwrap();
    app.type_text("id:backup-import-password", PASSWORD)
        .unwrap();
    app.press("id:backup-inspect").unwrap();
    app.wait_for_within("text:/备份版本 v3 ·/", Duration::from_secs(120))
        .unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("订阅 0 · 覆写 0")),
        "{seen:?}"
    );

    // 修改密码：预览立即放弃。
    app.type_text("id:backup-import-password", "changed-password-1")
        .unwrap();
    wait_text_gone(&mut app, "备份版本 v3");
    let _ = std::fs::remove_file(&backup_file);
}

/// 端到端恢复：导出（1 订阅）→ 删除订阅 → 导入确认 → 准备 pending；
/// 待重启横幅出现、导出/导入卡片隐藏、业务写被拒、回滚备份存在。
#[test]
fn confirmed_restore_marks_pending_blocks_writes_and_requires_restart() {
    let server = TestServer::new(vec![TestRoute::ok("/a.yaml", YAML)]);
    let (dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::open_subscriptions(&mut app);

    // 订阅页新增并自动刷新成功（2 节点）。
    app.type_text("id:subscription-name", "备份订阅").unwrap();
    app.type_text("id:subscription-url", &server.url("/a.yaml"))
        .unwrap();
    app.press("新增订阅").unwrap();
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();

    // 备份页导出真实加密备份。
    open_backup(&mut app);
    let backup_file = temp_target("restore.zip.age");
    app.on_pick_save_path(Some(backup_file.clone())).unwrap();
    app.type_text("id:backup-export-password", PASSWORD)
        .unwrap();
    app.press("id:backup-export").unwrap();
    app.wait_for_within("text:/已导出加密备份：/", Duration::from_secs(120))
        .unwrap();

    // 删除订阅（确认对话框）。
    app.press("订阅").unwrap();
    app.wait_for("备份订阅").unwrap();
    app.press("id:subscription-row-备份订阅 >> text:更多")
        .unwrap();
    app.press("id:subscription-delete").unwrap();
    app.wait_for("删除订阅“备份订阅”？").unwrap();
    app.press("确认删除").unwrap();
    app.wait_for("尚无订阅。").unwrap();

    // 备份页导入 → 预览（订阅 1）→ 确认恢复。
    open_backup(&mut app);
    app.on_pick_paths(vec![backup_file.clone()]).unwrap();
    app.press("id:backup-pick").unwrap();
    app.wait_for_within("text:/已选择：/", Duration::from_secs(10))
        .unwrap();
    app.type_text("id:backup-import-password", PASSWORD)
        .unwrap();
    app.press("id:backup-inspect").unwrap();
    app.wait_for_within("text:/订阅 1 · 覆写 0/", Duration::from_secs(120))
        .unwrap();
    app.press("id:backup-confirm-open").unwrap();
    app.wait_for("恢复此备份？").unwrap();
    app.press("id:backup-confirm-restore").unwrap();
    // 待重启：横幅真实展示（prepare_status 在导入卡片内，pending 后随
    // 卡片隐藏，成功信号以横幅为准），导出/导入卡片隐藏，磁盘 pending +
    // 回滚存在。
    app.wait_for_within(
        "text:/恢复已准备完成：请重启应用/",
        Duration::from_secs(120),
    )
    .unwrap();
    app.wait_for("text:/回滚备份 .*/").unwrap();
    let seen = texts(&app);
    assert!(
        !seen.iter().any(|text| text.contains("导出加密备份")),
        "pending hides export card: {seen:?}"
    );
    assert!(dir.path().join(".backup-pending").is_dir());
    let history = dir.path().join("backup-history");
    assert!(
        std::fs::read_dir(&history)
            .expect("backup-history")
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("Lanpath-rollback-")
            }),
        "encrypted rollback missing"
    );

    // 业务写被拒（订阅新增）：真实错误上屏。
    common::open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "拒绝新增").unwrap();
    app.type_text("id:subscription-url", &server.url("/a.yaml"))
        .unwrap();
    app.press("新增订阅").unwrap();
    app.wait_for_within("text:/已准备恢复备份/", Duration::from_secs(10))
        .unwrap();

    // 回到备份页：待重启仍在（不按下重启按钮——headless 无法验收跨进程重启）。
    app.press("备份").unwrap();
    app.wait_for("text:/恢复已准备完成：请重启应用/").unwrap();
    app.wait_for("id:backup-restart").unwrap();
    let _ = std::fs::remove_file(&backup_file);
}

/// 启动恢复结果：已应用摘要展示，确认（acknowledge）后才清除标记。
#[test]
fn startup_restored_notice_shows_summary_and_acknowledge_clears_it() {
    let (dir, entries) = data_root();
    {
        let mut app = start_app(entries.clone()).0;
        common::wait_ready(&mut app);
    }
    // 直接在盘上构造“上次启动已应用恢复”的状态（与启动事务同一共享函数）。
    stage_applied_restore(dir.path());

    let mut app = start_app(entries).0;
    app.wait_for("Lanpath NSH Desktop").unwrap();
    open_backup(&mut app);
    app.wait_for("text:/上次启动已从备份恢复/").unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("订阅 1 · 覆写 0")),
        "{seen:?}"
    );
    app.press("id:backup-acknowledge").unwrap();
    wait_text_gone(&mut app, "上次启动已从备份恢复");
    assert!(!dir.path().join(".backup-applied.json").exists());
}

/// 启动恢复失败：.backup-failed 真实错误上屏，原配置保留提示可见。
#[test]
fn startup_failed_restore_shows_the_real_error_banner() {
    let (dir, entries) = data_root();
    {
        let mut app = start_app(entries.clone()).0;
        common::wait_ready(&mut app);
    }
    std::fs::write(
        dir.path().join(".backup-failed"),
        b"restore failed; original config preserved",
    )
    .unwrap();

    let mut app = start_app(entries).0;
    app.wait_for("Lanpath NSH Desktop").unwrap();
    open_backup(&mut app);
    app.wait_for("backup-startup-error").unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter()
            .any(|text| text.contains("恢复未完成，原配置已保留")),
        "{seen:?}"
    );
    assert!(
        seen.iter()
            .any(|text| text.contains("请重新选择备份并导入")),
        "{seen:?}"
    );
}

/// 用另一个原始根造真实备份并直接把“待恢复”应用掉（共享启动事务），
/// 在盘上留下 `.backup-applied.json` 与恢复后的订阅清单。
fn stage_applied_restore(app_root: &Path) {
    let source = tempfile::tempdir().expect("source root");
    lanpath_client::SubscriptionStore::new(source.path().to_path_buf())
        .add("恢复订阅", "https://example.com/restored.yaml", true, None)
        .expect("seed source subscription");
    let service = lanpath_client::backup::BackupService::new(source.path().to_path_buf());
    let encrypted = service
        .export(PASSWORD, None)
        .expect("export source backup");
    let snapshot = lanpath_client::backup::decode(&encrypted, PASSWORD).expect("decode");
    lanpath_client::backup::prepare_restore(app_root, snapshot, PASSWORD, None)
        .expect("stage pending");
    lanpath_client::backup::apply_pending_at_startup(app_root).expect("apply at startup");
}
