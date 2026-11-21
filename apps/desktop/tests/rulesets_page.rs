//! 规则集页面端到端集成测试：手动规则集落盘、远程规则集下载与 304 跳过、市场浏览与删除清理。

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use common::{data_root, start_app};

fn start_mock_ruleset_server() -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0_u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let has_etag = req.lines().any(|line| {
                line.to_ascii_lowercase().contains("if-none-match:") && line.contains("\"rs-v1\"")
            });
            let response = if has_etag {
                "HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_owned()
            } else {
                let body = "SRS-TEST-DATA";
                format!(
                    "HTTP/1.1 200 OK\r\nETag: \"rs-v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://{addr}/corp.srs"), handle)
}

#[test]
fn rulesets_page_manages_manual_remote_download_market_and_delete() {
    let (remote_url, _server) = start_mock_ruleset_server();
    let (dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);

    common::navigate(&mut app, "规则集");
    app.wait_for("id:rulesets-add").unwrap();

    // 1. 新增手动 JSON 规则集
    app.press("id:rulesets-add").unwrap();
    app.wait_for("id:ruleset-source-manual").unwrap();
    app.press("id:ruleset-source-manual").unwrap();
    app.type_text("id:ruleset-draft-tag", "my-manual").unwrap();
    app.type_text("id:ruleset-draft-name", "内部规则").unwrap();
    app.press("id:ruleset-editor-save").unwrap();
    app.wait_for("规则集已保存。").unwrap();
    assert!(dir.path().join("rulesets/manifest.json").is_file());
    let store = lanpath_client::rulesets::RuleSetStore::new(dir.path().to_path_buf());
    let loaded = store.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(store.has_backing_file(&loaded[0]));

    // 2. 新增远程规则集并通过真实 HTTP mock 下载 + 304 跳过
    app.press("id:rulesets-add").unwrap();
    app.wait_for("id:ruleset-draft-url").unwrap();
    app.type_text("id:ruleset-draft-tag", "corp-remote")
        .unwrap();
    app.type_text("id:ruleset-draft-url", &remote_url).unwrap();
    app.press("id:ruleset-editor-save").unwrap();
    app.wait_for("id:ruleset-update-corp-remote").unwrap();

    app.press("id:ruleset-update-corp-remote").unwrap();
    app.wait_for_within("已更新规则集 corp-remote。", Duration::from_secs(15))
        .unwrap();
    let loaded = store.load().unwrap();
    let remote_entry = loaded.iter().find(|e| e.tag == "corp-remote").unwrap();
    assert!(store.has_backing_file(remote_entry));

    // 第二次更新命中 304
    app.press("id:ruleset-update-corp-remote").unwrap();
    app.wait_for_within(
        "规则集 corp-remote 已是最新，未重复写入。",
        Duration::from_secs(15),
    )
    .unwrap();

    // 3. 在「规则集」页呼出右侧分组管理面板并新建分组
    app.press("id:rulesets-open-groups").unwrap();
    app.wait_for("id:ruleset-group-add").unwrap();
    app.press("id:ruleset-group-add").unwrap();
    app.wait_for("id:ruleset-group-draft-tag").unwrap();
    app.type_text("id:ruleset-group-draft-tag", "direct-bundle")
        .unwrap();
    app.type_text("id:ruleset-group-draft-name", "直连合集")
        .unwrap();
    app.press("id:ruleset-group-member-my-manual").unwrap();
    app.press("id:ruleset-group-member-corp-remote").unwrap();
    app.press("id:ruleset-group-editor-save").unwrap();
    app.wait_for("规则集分组已保存。").unwrap();
    app.press("id:workspace-sheet-close").unwrap();
    let manifest = store.load_manifest().unwrap();
    assert_eq!(manifest.groups.len(), 1);
    assert_eq!(manifest.groups[0].tag, "direct-bundle");
    assert_eq!(
        manifest.groups[0].member_tags,
        vec!["my-manual", "corp-remote"]
    );

    // 4. 切到规则集市场校验开放的 geosite-private / geosite-cn 目录渲染
    app.press("id:rulesets-tab-market").unwrap();
    app.wait_for("id:market-add-geosite-private").unwrap();
    app.wait_for("id:market-add-geosite-cn").unwrap();

    // 5. 前往「规则」页：可直接呼出共用分组侧边面板查看/新建分组，并应用 @direct-bundle 规则
    common::navigate(&mut app, "规则");
    app.wait_for("id:rules-open-groups").unwrap();
    app.press("id:rules-open-groups").unwrap();
    app.wait_for("id:ruleset-group-edit-direct-bundle").unwrap();
    app.press("id:workspace-sheet-close").unwrap();

    app.wait_for("批量输入").unwrap();
    app.press("批量输入").unwrap();
    app.type_text("id:rules-batch", "ruleset @direct-bundle -> direct")
        .unwrap();
    app.press("id:rules-save").unwrap();
    app.wait_for("规则已保存，下次启动或重启核心时应用。")
        .unwrap();
    let saved_rules = lanpath_client::rules::load_rules(dir.path()).unwrap();
    assert!(
        saved_rules.iter().any(|r| r.target == "@direct-bundle"
            && r.match_type == lanpath_client::RuleMatchType::RuleSet)
    );

    // 6. 返回「规则集」页删除规则集并确认分组同步清理成员
    common::navigate(&mut app, "规则集");
    app.wait_for("id:ruleset-delete-my-manual").unwrap();
    app.press("id:ruleset-delete-my-manual").unwrap();
    app.wait_for("确认删除").unwrap();
    app.press("确认删除").unwrap();
    app.wait_for("规则集与本地缓存已删除。").unwrap();
    let after = store.load_manifest().unwrap();
    assert_eq!(after.items.len(), 1);
    assert_eq!(after.groups[0].member_tags, vec!["corp-remote"]);
}
