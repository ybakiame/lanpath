//! sing-box 配置切片页端到端回归：默认初始化、表单编辑 + JSON 源码同步、
//! 受管端口与控制 API 保存、恢复默认确认、离页未保存保护。

mod common;

use common::{data_root, start_app_desktop as start_app};

#[test]
fn overwrite_page_initializes_slices_edits_form_and_json_and_saves_bindings() {
    let (dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);

    // 初始化后 5 个默认切片文件已落盘。
    for name in [
        "log.json",
        "dns.json",
        "route.json",
        "outbounds.json",
        "experimental.json",
    ] {
        assert!(
            dir.path().join("overwrite/sing-box").join(name).is_file(),
            "missing overwrite/sing-box/{name}"
        );
    }

    common::navigate(&mut app, "配置切片");
    app.wait_for("id:overwrite-mixed-port").unwrap();
    app.wait_for("id:overwrite-slice-group").unwrap();

    // 1. 修改受管端口并保存到 app.toml。
    app.clear("id:overwrite-mixed-port").unwrap();
    app.type_text("id:overwrite-mixed-port", "17911").unwrap();
    app.press("id:overwrite-bindings-save").unwrap();
    app.wait_for("id:overwrite-notice").unwrap();
    let saved_cfg = lanpath_client::ClientConfig::load_for_requests(dir.path()).unwrap();
    assert_eq!(saved_cfg.mixed_port, 17911);

    // 2. 在表单模式下修改 log.json 的 output 并保存。
    app.type_text("id:overwrite-log-output", "logs/custom.log")
        .unwrap();
    app.press("id:overwrite-save").unwrap();
    app.wait_for("已保存 log.json，下次启动或重启核心时应用。")
        .unwrap();
    let log_text = std::fs::read_to_string(dir.path().join("overwrite/sing-box/log.json")).unwrap();
    assert!(log_text.contains("logs/custom.log"), "{log_text}");

    // 3. 切到 JSON 源码模式查看同步结果并切换到 DNS 切片添加上游。
    app.press("id:overwrite-mode-json >> text:JSON 源码")
        .unwrap();
    app.wait_for("id:overwrite-json-editor").unwrap();
    app.wait_for("JSON 结构有效").unwrap();

    app.press("id:overwrite-mode-form >> text:可视化表单")
        .unwrap();
    app.press("id:overwrite-slice-dns >> text:DNS").unwrap();
    app.wait_for("id:overwrite-dns-new-tag").unwrap();
    // 一键添加国内推荐 DNS 预设
    app.wait_for("id:overwrite-dns-preset-add-dnspod-doh")
        .unwrap();
    app.press("id:overwrite-dns-preset-add-dnspod-doh").unwrap();
    // 修改内置 local 上游并使用推荐预设填入替换
    app.press("id:overwrite-dns-edit-local").unwrap();
    app.press("id:overwrite-dns-preset-add-ali-udp").unwrap();
    app.press("id:overwrite-dns-add").unwrap();
    // 手动添加自定义上游
    app.type_text("id:overwrite-dns-new-tag", "cf").unwrap();
    app.type_text("id:overwrite-dns-new-server", "1.1.1.1")
        .unwrap();
    app.press("id:overwrite-dns-add").unwrap();
    app.press("id:overwrite-save").unwrap();
    app.wait_for("已保存 dns.json，下次启动或重启核心时应用。")
        .unwrap();
    let dns_text = std::fs::read_to_string(dir.path().join("overwrite/sing-box/dns.json")).unwrap();
    assert!(dns_text.contains("\"1.1.1.1\""), "{dns_text}");
    assert!(dns_text.contains("\"dnspod-doh\""), "{dns_text}");
    assert!(dns_text.contains("\"udp\""), "{dns_text}");

    // 4. 恢复默认需要确认。
    app.press("id:overwrite-reset").unwrap();
    app.wait_for("id:overwrite-reset-dialog").unwrap();
    app.press("id:overwrite-reset-confirm").unwrap();
    app.wait_for("已将 dns.json 恢复为默认配置。").unwrap();
    let dns_after =
        std::fs::read_to_string(dir.path().join("overwrite/sing-box/dns.json")).unwrap();
    assert!(!dns_after.contains("\"1.1.1.1\""), "{dns_after}");
}
