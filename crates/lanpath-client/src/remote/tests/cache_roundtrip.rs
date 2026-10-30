use super::*;
use crate::import::parse_import;
use lanpath_mitm::{HookMode, RewriteAction, RewriteEngine};

#[test]
fn static_rewrites_keep_network_effects_after_serialization_and_activation() {
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().to_path_buf());
    let imported = parse_import(
        r#"
[Header Rewrite]
http-request ^https://a\.invalid/headers header-add X-Multi second
http-request ^https://a\.invalid/headers header-replace X-Multi replaced
http-request ^https://a\.invalid/headers header-replace X-Absent not-added
http-request ^https://a\.invalid/headers header-del X-Delete
[Body Rewrite]
http-response ^https://a\.invalid/body ^old$ middle middle final
[Map Local]
^https://a\.invalid/binary data-type=base64 data="AP+A" header="Set-Cookie:a=1|Set-Cookie:b=2"
[URL Rewrite]
^https://a\.invalid/redirect/(.*) https://b.invalid/$1 307
"#,
        ScriptDialect::Surge,
    )
    .unwrap();
    manager.merge_imported(&imported).unwrap();
    let engine = RewriteEngine {
        rules: manager.load_cached().unwrap().rewrites,
    };
    let mut headers = vec![
        ("X-Multi".into(), "first".into()),
        ("X-Delete".into(), "gone".into()),
    ];
    engine
        .apply_request(
            &mut "https://a.invalid/headers".into(),
            &mut headers,
            &mut None,
        )
        .unwrap();
    assert_eq!(
        headers,
        [
            ("X-Multi".into(), "replaced".into()),
            ("X-Multi".into(), "replaced".into())
        ]
    );
    let mut body = Some("old\nold".into());
    engine
        .apply_response(
            "https://a.invalid/body",
            &mut 200,
            &mut Vec::new(),
            &mut body,
        )
        .unwrap();
    assert_eq!(body.as_deref(), Some("final\nfinal"));
    let RewriteAction::Mock { body, headers, .. } = engine
        .apply_request(
            &mut "https://a.invalid/binary".into(),
            &mut Vec::new(),
            &mut None,
        )
        .unwrap()
    else {
        panic!("binary map did not activate")
    };
    assert_eq!(body, [0, 255, 128]);
    assert_eq!(
        &headers[..2],
        &[
            ("Set-Cookie".into(), "a=1".into()),
            ("Set-Cookie".into(), "b=2".into())
        ]
    );
    assert_eq!(
        engine
            .apply_request(
                &mut "https://a.invalid/redirect/path".into(),
                &mut Vec::new(),
                &mut None
            )
            .unwrap(),
        RewriteAction::Redirect {
            status: 307,
            location: "https://b.invalid/path".into()
        }
    );
}

#[tokio::test]
async fn source_downloads_remain_aligned_and_keep_each_hooks_dialect_and_policy() {
    let base = spawn_remote_server(|_| String::new()).await;
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().to_path_buf());
    let qx = format!(
        "^https://local.invalid/ url script-request-body local.js\n^https://failed.invalid/ url script-response-body {base}/missing.js\n^https://echo.invalid/ url script-analyze-echo-response {base}/hook.js timeout=2.5 binary-body-mode=true full-header-mode=true argument=literal"
    );
    let summary = manager
        .import_content(&qx, ScriptDialect::QuantumultX)
        .await
        .unwrap();
    assert_eq!(summary.scripts, 1);
    assert_eq!(summary.warnings.len(), 2);
    assert!(
        summary
            .warnings
            .iter()
            .all(|warning| !warning.contains(&base) && !warning.contains("missing.js"))
    );
    let loon = format!(
        "[Script]\nresponse if ${{url}} ~= /^https:\\/\\/loon\\.invalid\\//i then script(\"{base}/hook.js\", \"current\") with requires_body=true, binary_body_mode=true, timeout=3\nhttp-request ^https://legacy\\.invalid/ script-path={base}/hook.js, argument=legacy, timeout=4"
    );
    assert_eq!(
        manager
            .import_content(&loon, ScriptDialect::Loon)
            .await
            .unwrap()
            .scripts,
        2
    );
    let stash = format!(
        "script-providers:\n  hook:\n    url: {base}/hook.js\nhttp:\n  script:\n    - name: hook\n      match: '^https://stash.invalid/'\n      type: response\n      require-body: true\n      binary-mode: true\n      timeout: 1.5\n      argument: stash"
    );
    assert_eq!(
        manager
            .import_content(&stash, ScriptDialect::Stash)
            .await
            .unwrap()
            .scripts,
        1
    );
    let active = manager.load_cached().unwrap();
    assert_eq!(active.scripts.len(), 4);
    let echo = &active.scripts[0];
    assert!(echo.pattern.is_match("https://echo.invalid/").unwrap());
    assert_eq!(echo.source_id, format!("{base}/hook.js"));
    assert_eq!(echo.source, "const hook = 1;");
    assert_eq!(echo.dialect, ScriptDialect::QuantumultX);
    assert_eq!(echo.mode, HookMode::Echo);
    assert!(echo.requires_body && echo.binary && echo.full_headers);
    assert_eq!(echo.timeout_ms, Some(2500));
    assert_eq!(active.scripts[1].dialect, ScriptDialect::Loon);
    assert!(
        active.scripts[1]
            .pattern
            .is_match("HTTPS://LOON.INVALID/")
            .unwrap()
    );
    assert_eq!(active.scripts[1].argument.as_deref(), Some("current"));
    assert_eq!(active.scripts[1].timeout_ms, Some(3000));
    assert_eq!(active.scripts[2].dialect, ScriptDialect::Loon);
    assert!(
        active.scripts[2]
            .pattern
            .is_match("HTTPS://LEGACY.INVALID/")
            .unwrap()
    );
    assert_eq!(active.scripts[2].argument.as_deref(), Some("legacy"));
    assert_eq!(active.scripts[3].dialect, ScriptDialect::Stash);
    assert!(active.scripts[3].requires_body && active.scripts[3].binary);
    assert_eq!(active.scripts[3].argument.as_deref(), Some("stash"));
    assert_eq!(active.scripts[3].timeout_ms, Some(1500));
}
