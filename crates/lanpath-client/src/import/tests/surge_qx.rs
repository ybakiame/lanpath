//! Independent specification-derived inputs, not vendor script copies.
//! Sources: https://manual.nssurge.com/http/map-local.html and sibling rewrite pages;
//! https://github.com/crossutility/Quantumult-X/blob/master/sample.conf.

use super::super::{ImportedConfig, parse_import};
use lanpath_mitm::{HookMode, RewriteAction, RewriteEngine};
use lanpath_script::{ScriptDialect, ScriptKind};

#[test]
fn rejected_local_source_does_not_steal_remote_hook_source() {
    for (dialect, content) in [
        (
            ScriptDialect::QuantumultX,
            "[rewrite_local]\n^https://local\\.invalid/ url script-request-body local.js\n^https://remote\\.invalid/ url script-response-body https://assets.invalid/remote.js#key=kept",
        ),
        (
            ScriptDialect::Surge,
            "[Script]\nlocal=type=http-request,pattern=^https://local\\.invalid/,script-path=local.js\nremote=type=http-response,pattern=^https://remote\\.invalid/,script-path=https://assets.invalid/remote.js#key=kept",
        ),
    ] {
        let cfg = parse_import(content, dialect).unwrap();
        assert_eq!(cfg.scripts.len(), 1);
        assert_eq!(cfg.script_urls.len(), 1);
        let hook = &cfg.scripts[0];
        assert!(
            hook.pattern
                .is_match("https://remote.invalid/data")
                .unwrap()
        );
        assert!(!hook.pattern.is_match("https://local.invalid/data").unwrap());
        assert_eq!(hook.name, cfg.script_urls[0].0);
        assert_eq!(hook.source_id, cfg.script_urls[0].1);
        assert_eq!(hook.source_id, "https://assets.invalid/remote.js#key=kept");
        assert_eq!(hook.dialect, dialect);
        assert_eq!(hook.kind, ScriptKind::HttpResponse);
        assert!(!cfg.warnings.is_empty());
    }
}

#[test]
fn qx_sectionless_echo_modes_keep_body_and_options() {
    let cfg = parse_import(r#"
^https://a\.invalid/ url script-echo-response https://assets.invalid/a.js enabled=false timeout=2.5 argument="one two" binary-body-mode=true full-header-mode=true
^https://b\.invalid/ url script-analyze-echo-response https://assets.invalid/b.js
^https://c\.invalid/ url script-response-header https://assets.invalid/c.js
"#, ScriptDialect::QuantumultX).unwrap();
    assert_eq!(cfg.scripts.len(), 3);
    let first = &cfg.scripts[0];
    assert_eq!(first.mode, HookMode::Echo);
    assert_eq!(first.kind, ScriptKind::HttpRequest);
    assert!(!first.requires_body);
    assert!(!first.enabled);
    assert_eq!(first.timeout_ms, Some(2500));
    assert_eq!(first.argument.as_deref(), Some("one two"));
    assert!(first.binary);
    assert!(first.full_headers);
    assert_eq!(cfg.scripts[1].mode, HookMode::Echo);
    assert!(cfg.scripts[1].requires_body);
    assert_eq!(cfg.scripts[2].mode, HookMode::Modify);
    assert_eq!(cfg.scripts[2].kind, ScriptKind::HttpResponse);
    assert!(!cfg.scripts[2].requires_body);
}

#[test]
fn surge_options_and_cron_fields_are_retained() {
    let cfg = parse_import(r#"
[Script]
request=type=http-request,pattern=^https://a\.invalid/,script-path=https://assets.invalid/r.js,requires-body=true,max-size=-1,binary-body-mode=true,full-header-mode=true,timeout=1.25,enabled=false,argument="x,y"
response=type=http-response,pattern=^https://a\.invalid/,script-path=https://assets.invalid/s.js,max-size=-1
five=type=cron,cronexp="15 9 * * *",script-path=https://assets.invalid/t.js,argument="a,b",timeout=8,enabled=false
six=type=cron,cronexp="30 15 9 * * *",script-path=https://assets.invalid/u.js
"#, ScriptDialect::Surge).unwrap();
    assert_eq!(cfg.scripts.len(), 2);
    let hook = &cfg.scripts[0];
    assert_eq!(hook.mode, HookMode::Modify);
    assert_eq!(hook.dialect, ScriptDialect::Surge);
    assert!(hook.requires_body && hook.binary && hook.full_headers);
    assert_eq!(hook.max_size, 32 * 1024 * 1024);
    assert!(!hook.enabled);
    assert_eq!(hook.timeout_ms, Some(1250));
    assert_eq!(hook.argument.as_deref(), Some("x,y"));
    assert_eq!(cfg.scripts[1].max_size, usize::MAX);
    assert_eq!(cfg.task_scripts.len(), 2);
    let task = &cfg.task_scripts[0].0;
    assert_eq!(task.cron_expr, "0 15 9 * * *");
    assert!(!task.enabled);
    assert_eq!(task.timeout_ms, Some(8000));
    assert_eq!(task.argument.as_deref(), Some("a,b"));
    assert_eq!(task.source_id, cfg.task_scripts[0].1);
    assert_eq!(cfg.task_scripts[1].0.cron_expr, "30 15 9 * * *");
}

#[test]
fn surge_body_envelope_rejects_unsupported_explicit_limits_without_source_activation() {
    let cfg = parse_import(r#"
[Script]
request=type=http-request,pattern=^https://a\.invalid/,script-path=https://assets.invalid/r.js,requires-body=true,max-size=33554433
response=type=http-response,pattern=^https://a\.invalid/,script-path=https://assets.invalid/s.js,requires-body=true,max-size=33554433
bounded=type=http-request,pattern=^https://a\.invalid/,script-path=https://assets.invalid/b.js,requires-body=true,max-size=-1
default=type=http-response,pattern=^https://a\.invalid/,script-path=https://assets.invalid/d.js,requires-body=true
"#, ScriptDialect::Surge).unwrap();
    assert_eq!(
        cfg.scripts
            .iter()
            .map(|rule| rule.name.as_str())
            .collect::<Vec<_>>(),
        ["bounded", "default"]
    );
    assert_eq!(cfg.script_urls.len(), 2);
    assert_eq!(cfg.scripts[0].max_size, lanpath_mitm::SURGE_BODY_ENVELOPE);
    assert_eq!(cfg.scripts[1].max_size, 10 * 1024 * 1024);
    assert_eq!(cfg.warnings.len(), 2);
}

#[test]
fn qx_cron_five_and_six_fields_preserve_task_options() {
    let cfg = parse_import(r#"
[task_local]
15 9 * * * https://assets.invalid/five.js#value=kept, tag=first, argument="x,y", timeout=7, enabled=false
30 15 9 * * * https://assets.invalid/six.js, tag=second
"#, ScriptDialect::QuantumultX).unwrap();
    assert_eq!(cfg.task_scripts.len(), 2);
    let task = &cfg.task_scripts[0].0;
    assert_eq!(task.cron_expr, "0 15 9 * * *");
    assert_eq!(task.dialect, ScriptDialect::QuantumultX);
    assert_eq!(task.source_id, "https://assets.invalid/five.js#value=kept");
    assert!(!task.enabled);
    assert_eq!(task.argument.as_deref(), Some("x,y"));
    assert_eq!(task.timeout_ms, Some(7000));
    assert_eq!(cfg.task_scripts[1].0.cron_expr, "30 15 9 * * *");
}

#[test]
fn surge_header_actions_append_replace_existing_and_remove() {
    let cfg = parse_import(
        r#"
[Header Rewrite]
http-request ^https://a\.invalid/ header-add X-Multi second
http-request ^https://a\.invalid/ header-replace X-Multi replaced
http-request ^https://a\.invalid/ header-replace X-Missing not-added
http-request ^https://a\.invalid/ header-del X-Delete
http-request ^https://a\.invalid/ header-add X-Multi final
"#,
        ScriptDialect::Surge,
    )
    .unwrap();
    let engine = RewriteEngine {
        rules: cfg.rewrites,
    };
    let mut url = "https://a.invalid/".to_string();
    let mut headers = vec![
        ("X-Multi".into(), "first".into()),
        ("X-Delete".into(), "gone".into()),
    ];
    engine
        .apply_request(&mut url, &mut headers, &mut None)
        .unwrap();
    let multi: Vec<_> = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("x-multi"))
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(multi, ["replaced", "replaced", "final"]);
    assert!(
        !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("x-missing")
                || name.eq_ignore_ascii_case("x-delete"))
    );
}

#[test]
fn surge_body_pairs_are_sequential_and_multiline() {
    let cfg = parse_import(
        r#"
[Body Rewrite]
http-response ^https://a\.invalid/ ^old$ middle middle final
"#,
        ScriptDialect::Surge,
    )
    .unwrap();
    let engine = RewriteEngine {
        rules: cfg.rewrites,
    };
    let mut body = Some("old\nold".to_string());
    engine
        .apply_response("https://a.invalid/", &mut 200, &mut Vec::new(), &mut body)
        .unwrap();
    assert_eq!(body.as_deref(), Some("final\nfinal"));
}

#[test]
fn map_local_retains_binary_empty_text_and_duplicate_headers() {
    let cfg = parse_import(r#"
[Map Local]
^https://a\.invalid/ data-type=base64 data="AP+A" header="Set-Cookie:a=1|Set-Cookie:b=2" header="X-Test: yes"
^https://b\.invalid/ data-type=text data=""
^https://c\.invalid/ data-type=tiny-gif
"#, ScriptDialect::Surge).unwrap();
    assert!(cfg.warnings.is_empty());
    assert_eq!(cfg.rewrites.len(), 3);
    let engine = RewriteEngine {
        rules: cfg.rewrites,
    };
    let mut url = "https://a.invalid/".to_string();
    let RewriteAction::Mock {
        status,
        body,
        headers,
    } = engine
        .apply_request(&mut url, &mut Vec::new(), &mut None)
        .unwrap()
    else {
        panic!("expected local response")
    };
    assert_eq!(status, 200);
    assert_eq!(body, [0, 255, 128]);
    let cookies: Vec<_> = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(cookies, ["a=1", "b=2"]);
    url = "https://b.invalid/".into();
    assert!(
        matches!(engine.apply_request(&mut url, &mut Vec::new(), &mut None).unwrap(), RewriteAction::Mock { body, .. } if body.is_empty())
    );
    url = "https://c.invalid/".into();
    assert!(
        matches!(engine.apply_request(&mut url, &mut Vec::new(), &mut None).unwrap(), RewriteAction::Mock { body, headers, .. } if body.starts_with(b"GIF89a") && headers.iter().any(|(name, value)| name == "Content-Type" && value == "image/gif"))
    );
}

#[test]
fn encoded_map_headers_preserve_duplicate_lines() {
    let cfg = parse_import("[Map Local]\n^https://a\\.invalid/ data-type=text data=ok header=\"WC1BOiBvbmUNClgtQTogdHdv\"", ScriptDialect::Surge).unwrap();
    let engine = RewriteEngine {
        rules: cfg.rewrites,
    };
    let RewriteAction::Mock { headers, .. } = engine
        .apply_request(&mut "https://a.invalid/".into(), &mut Vec::new(), &mut None)
        .unwrap()
    else {
        panic!("expected local response")
    };
    assert_eq!(
        &headers[..2],
        &[("X-A".into(), "one".into()), ("X-A".into(), "two".into())]
    );
}

#[test]
fn unsupported_actions_engines_and_files_never_activate_or_expose_input() {
    for (dialect, content) in [
        (
            ScriptDialect::QuantumultX,
            "[rewrite_local]\n^https://private.invalid/?token=secret ^POST url-and-header reject\n^https://private.invalid/ url echo-response text/html echo-response token-secret.html\n^https://private.invalid/ url unknown-secret-action\n^https://private.invalid/ url script-response-body https://assets.invalid/secret.js engine=webview",
        ),
        (
            ScriptDialect::Surge,
            "[Script]\nsecret=type=http-request,pattern=^https://private.invalid/,script-path=https://assets.invalid/secret.js,engine=webview\n[Map Local]\n^https://private.invalid/ data=token-secret.json\n[Header Rewrite]\n^https://private.invalid/ token-secret-action Token secret\n[URL Rewrite]\n^https://private.invalid/ https://target.invalid/ token-secret-mode",
        ),
    ] {
        let cfg = parse_import(content, dialect).unwrap();
        assert!(cfg.scripts.is_empty());
        assert!(cfg.script_urls.is_empty());
        assert!(cfg.rewrites.is_empty());
        assert!(!cfg.warnings.is_empty());
        for warning in cfg.warnings {
            assert!(warning.contains("not activated"));
            assert!(!warning.contains("secret"));
            assert!(!warning.contains("private.invalid"));
            assert!(!warning.contains("assets.invalid"));
        }
    }
}

#[test]
fn qx_header_rewrite_can_change_request_line_without_losing_headers() {
    let cfg = parse_import(
        r#"
[rewrite_local]
^https://a\.invalid/ url request-header ^GET /old HTTP/1\.1 request-header POST /new HTTP/1.1
"#,
        ScriptDialect::QuantumultX,
    )
    .unwrap();
    let engine = RewriteEngine {
        rules: cfg.rewrites,
    };
    let mut method = "GET".to_string();
    let mut url = "https://a.invalid/old".to_string();
    let mut headers = vec![("X-Test".into(), "kept".into())];
    engine
        .apply_request_with_method(&mut method, &mut url, &mut headers, &mut None)
        .unwrap();
    assert_eq!(method, "POST");
    assert_eq!(url, "https://a.invalid/new");
    assert_eq!(headers, [("X-Test".into(), "kept".into())]);
}

#[test]
fn supported_response_unlimited_declaration_retains_complete_nonblocking_warning() {
    let cfg = parse_import("[Script]\nresponse=type=http-response,pattern=^https://example.invalid/,script-path=https://assets.invalid/response.js,requires-body=true,max-size=-1", ScriptDialect::Surge).unwrap();
    assert_eq!(cfg.scripts.len(), 1);
    assert_eq!(cfg.scripts[0].max_size, usize::MAX);
    assert_eq!(cfg.advisory_warnings.len(), 1);
    assert_eq!(cfg.warnings, cfg.advisory_warnings);
    assert_eq!(cfg.blocking_warnings().count(), 0);
    assert!(cfg.warnings[0].contains("32 MiB"));
    assert!(
        cfg.warnings[0].ends_with("larger responses skip the script and pass through unchanged")
    );
    assert!(!cfg.warnings[0].contains("[detail]"));
}

#[test]
fn static_advisories_preserve_apostrophes_while_dynamic_details_remain_redacted() {
    let mut cfg = ImportedConfig::default();
    cfg.advisory(
        "script",
        "Lanpath's supported inspection limit remains bounded",
    );
    cfg.warn(
        "script",
        "",
        "unsupported option 'https://secret.invalid/?token=private'; not activated",
    );
    assert!(cfg.advisory_warnings[0].ends_with("inspection limit remains bounded"));
    assert!(cfg.advisory_warnings[0].contains("Lanpath's"));
    assert_eq!(cfg.blocking_warnings().count(), 1);
    assert!(!cfg.warnings[1].contains("secret"));
    assert!(!cfg.warnings[1].contains("private"));
    assert!(cfg.warnings[1].contains("[detail]"));
}
