//! Headless 测试（`nsh_testing::Headless`，NSH 0.5.0）：日志页真实内存/磁盘
//! 分区、级别筛选、暂停/刷新、详情、清空确认与导出。采集走真实 tracing
//! 路径（`lanpath_*` target 经全局采集层进入本应用数据根的缓冲与磁盘）；
//! 导出经 NSH 原生保存对话框（headless 注入选定路径）。

mod common;

use std::path::Path;
use std::time::Duration;

use common::{data_root, start_app, texts};
use nsh_testing::Headless;

const TARGET: &str = "lanpath_desktop_nsh::headless_test";

fn emit_debug(message: &str) {
    tracing::debug!(target: TARGET, "{message}");
}

fn emit_info(message: &str) {
    tracing::info!(target: TARGET, "{message}");
}

fn emit_warn(message: &str) {
    tracing::warn!(target: TARGET, "{message}");
}

fn emit_error(message: &str) {
    tracing::error!(target: TARGET, "{message}");
}

/// 打开日志页（服务在页构造时打开并注册采集），等待内存分区就绪。
fn open_logs_page() -> (tempfile::TempDir, Headless) {
    let (dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("解析共享订阅").unwrap();
    app.press("日志").unwrap();
    app.wait_for("id:logs-memory-status").unwrap();
    (dir, app)
}

/// 磁盘文件由后台写入线程落盘：短轮询刷新文件列表直至出现 app.log。
fn wait_disk_file(app: &mut Headless) {
    for _ in 0..40 {
        app.press("刷新").unwrap();
        if texts(app).iter().any(|text| text.starts_with("app.log")) {
            return;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    panic!("no app.log appeared on disk: {:?}", texts(app));
}

/// 内存分区就绪：页头、分区切换、筛选、暂停/刷新与清空入口齐全。
#[test]
fn logs_page_opens_with_a_real_memory_section() {
    let (_dir, app) = open_logs_page();
    let seen = texts(&app);
    for expected in [
        "日志",
        "内存日志",
        "磁盘日志",
        "全部",
        "信息",
        "警告",
        "错误",
        "暂停",
        "刷新",
        "清空内存日志",
    ] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
    let status = seen
        .iter()
        .find(|text| text.contains("/1000"))
        .expect("memory status line");
    assert!(status.contains("缓冲"), "{status:?}");
    assert!(status.contains("显示"), "{status:?}");
}

/// 本应用 tracing 事件经真实采集层进入内存缓冲，手动刷新后可见。
#[test]
fn tracing_events_are_captured_into_the_memory_buffer() {
    let (_dir, mut app) = open_logs_page();
    emit_info("采集验证消息内存分区");
    app.press("刷新").unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter()
            .any(|text| text.contains("采集验证消息内存分区")),
        "{seen:?}"
    );
    assert!(
        seen.iter()
            .any(|text| text.contains("[INFO]") && text.contains(TARGET)),
        "{seen:?}"
    );
}

/// 级别筛选按秩过滤：信息默认；调试显示全部；错误只留错误。
#[test]
fn level_filters_hide_and_show_levels_for_real() {
    let (_dir, mut app) = open_logs_page();
    emit_debug("筛选调试级消息");
    emit_info("筛选信息级消息");
    emit_warn("筛选警告级消息");
    emit_error("筛选错误级消息");
    app.press("刷新").unwrap();

    // 默认“信息”：调试隐藏，其余可见。
    let seen = texts(&app);
    assert!(
        !seen.iter().any(|text| text.contains("筛选调试级消息")),
        "{seen:?}"
    );
    for expected in ["筛选信息级消息", "筛选警告级消息", "筛选错误级消息"] {
        assert!(seen.iter().any(|text| text.contains(expected)), "{seen:?}");
    }

    // “调试”：全部可见。
    app.press("调试").unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("筛选调试级消息")),
        "{seen:?}"
    );

    // “错误”：只剩错误级。
    app.press("错误").unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("筛选错误级消息")),
        "{seen:?}"
    );
    for hidden in ["筛选调试级消息", "筛选信息级消息", "筛选警告级消息"] {
        assert!(!seen.iter().any(|text| text.contains(hidden)), "{seen:?}");
    }
}

/// 未暂停时轮询自动带出新事件（无需手动刷新）。
#[test]
fn auto_refresh_picks_up_new_events() {
    let (_dir, mut app) = open_logs_page();
    emit_info("自动刷新验证消息");
    app.wait_for_within("text:/自动刷新验证消息/", Duration::from_secs(5))
        .unwrap();
}

/// 暂停只停自动刷新，不停止采集；手动刷新仍可取回新事件。
#[test]
fn pause_stops_auto_refresh_but_manual_refresh_still_works() {
    let (_dir, mut app) = open_logs_page();
    emit_info("暂停验证消息第一条");
    app.press("刷新").unwrap();
    assert!(
        texts(&app)
            .iter()
            .any(|text| text.contains("暂停验证消息第一条"))
    );

    app.press("暂停").unwrap();
    assert!(texts(&app).contains(&"继续"));
    emit_info("暂停验证消息第二条");
    // 超过一个轮询周期：自动刷新被暂停，新事件不上屏。
    std::thread::sleep(Duration::from_millis(3200));
    assert!(
        !texts(&app)
            .iter()
            .any(|text| text.contains("暂停验证消息第二条")),
        "{:?}",
        texts(&app)
    );

    // 手动刷新不受暂停影响。
    app.press("刷新").unwrap();
    assert!(
        texts(&app)
            .iter()
            .any(|text| text.contains("暂停验证消息第二条"))
    );
}

/// 选中一行在详情分区展示完整内容。
#[test]
fn selecting_a_memory_row_shows_its_detail() {
    let (_dir, mut app) = open_logs_page();
    emit_warn("详情验证消息行");
    app.press("刷新").unwrap();
    app.press("text:/详情验证消息行/").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"日志详情"), "{seen:?}");
    assert!(
        seen.iter()
            .any(|text| text.contains("[WARN]") && text.contains("详情验证消息行")),
        "{seen:?}"
    );
}

/// 清空内存需经确认对话框；确认后消息从缓冲消失，磁盘分区不受影响。
#[test]
fn clearing_memory_logs_requires_confirmation() {
    let (_dir, mut app) = open_logs_page();
    emit_error("清空验证消息甲");
    app.press("刷新").unwrap();
    assert!(
        texts(&app)
            .iter()
            .any(|text| text.contains("清空验证消息甲"))
    );

    app.press("清空内存日志").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"清空内存日志？"), "{seen:?}");
    // 确认前消息仍在。
    assert!(seen.iter().any(|text| text.contains("清空验证消息甲")));

    app.press("确认清空").unwrap();
    let seen = texts(&app);
    assert!(
        !seen.iter().any(|text| text.contains("清空验证消息甲")),
        "{seen:?}"
    );
    let status = seen
        .iter()
        .find(|text| text.contains("/1000"))
        .expect("memory status line");
    assert!(status.contains("显示 0 条"), "{status:?}");
}

/// 磁盘分区列出本应用真实日志文件并显示尾部原文。
#[test]
fn disk_section_lists_real_files_and_tails_content() {
    let (_dir, mut app) = open_logs_page();
    emit_info("磁盘验证消息尾部读取");
    app.press("磁盘日志").unwrap();
    app.wait_for("logs-disk-status").unwrap();
    wait_disk_file(&mut app);

    let seen = texts(&app);
    let file = seen
        .iter()
        .find(|text| text.starts_with("app.log"))
        .expect("a real app.log file listed");
    assert!(file.chars().all(|c| !c.is_whitespace()), "{file:?}");
    assert!(
        seen.iter()
            .any(|text| text.contains("磁盘验证消息尾部读取")),
        "{seen:?}"
    );
    let status = seen
        .iter()
        .find(|text| text.contains("个磁盘文件"))
        .expect("disk status line");
    assert!(status.contains("当前：app.log"), "{status:?}");
}

/// 磁盘日志写入持续可用：本应用事件真实落到数据根的 logs/ 目录。
#[test]
fn disk_writer_persists_events_into_the_data_root() {
    let (dir, mut app) = open_logs_page();
    emit_info("磁盘持久化验证消息");
    app.press("磁盘日志").unwrap();
    app.wait_for("logs-disk-status").unwrap();
    wait_disk_file(&mut app);

    let logs = dir.path().join("logs");
    for _ in 0..40 {
        let content: String = std::fs::read_dir(&logs)
            .unwrap()
            .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect();
        if content.contains("磁盘持久化验证消息") {
            return;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    panic!("event never reached the disk log");
}

/// 导出经 NSH 原生保存对话框拿到目标路径后写出真实拼接内容。
#[test]
fn export_writes_real_concatenated_logs_to_the_chosen_path() {
    let (_dir, mut app) = open_logs_page();
    emit_info("导出验证消息原文");
    app.press("磁盘日志").unwrap();
    app.wait_for("logs-disk-status").unwrap();
    wait_disk_file(&mut app);

    let target = std::env::temp_dir().join(format!(
        "lanpath-desktop-export-test-{}-{}.log",
        std::process::id(),
        chrono::Local::now().format("%H%M%S")
    ));
    let _ = std::fs::remove_file(&target);
    app.on_pick_save_path(Some(target.clone())).unwrap();
    app.press("导出日志…").unwrap();
    app.wait_for_within("text:/已导出：/", Duration::from_secs(5))
        .unwrap();
    assert!(Path::new(&target).exists(), "exported file missing");
    let content = std::fs::read_to_string(&target).unwrap();
    assert!(
        content.contains("导出验证消息原文"),
        "exported content: {content:?}"
    );
    assert!(
        content.contains("INFO") && content.contains(TARGET),
        "{content:?}"
    );
    let _ = std::fs::remove_file(&target);
}

/// 内存缓冲随分区切换保留：服务按数据根缓存，来回切换不重开缓冲。
#[test]
fn switching_sections_keeps_the_memory_buffer() {
    let (_dir, mut app) = open_logs_page();
    emit_info("跨分区保留验证消息");
    app.press("刷新").unwrap();
    assert!(
        texts(&app)
            .iter()
            .any(|text| text.contains("跨分区保留验证消息"))
    );

    app.press("磁盘日志").unwrap();
    app.wait_for("logs-disk-status").unwrap();
    // 分区真实切换：内存状态行消失，磁盘状态行出现（文件尾部分区
    // 展示的是磁盘原文，同一消息可能 legitimately 出现在尾部文本中）。
    assert!(!app.screen().matches("id:logs-memory-status").unwrap());
    assert!(app.screen().matches("id:logs-disk-status").unwrap());

    app.press("内存日志").unwrap();
    app.wait_for("id:logs-memory-status").unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("跨分区保留验证消息")),
        "{seen:?}"
    );
}

#[test]
fn subscription_actions_are_captured_before_the_first_logs_page_visit() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("尚无订阅。").unwrap();
    app.type_text("id:subscription-name", "启动采集验证")
        .unwrap();
    app.type_text("id:subscription-url", "https://example.com/private.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    app.press("日志").unwrap();
    app.wait_for("text:/新增订阅：启动采集验证/").unwrap();
    assert!(!texts(&app).iter().any(|text| text.contains("private.yaml")));
}
