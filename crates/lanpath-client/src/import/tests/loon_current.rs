use crate::import::parse_import;
use lanpath_mitm::HookMode;
use lanpath_script::{ScriptDialect, ScriptKind};

#[test]
fn current_loon_preserves_regex_flags_and_execution_options() {
    let cfg = parse_import(r#"
[Script]
request if ${url} ~= /^https:\/\/api\.invalid\/v[12]/i then script("https://assets.invalid/request.js", "a,b") with requires_body=true, binary_body_mode=true, timeout=2.5, enable=false, tag="request"
response if ${url} ~= /^https:\/\/api\.invalid\// then script(`https://assets.invalid/response.js`)
cron "15 9 * * *" then script("https://assets.invalid/task.js", "literal") with timeout=4
cron "30 15 9 * * *" then script("https://assets.invalid/six.js")
"#, ScriptDialect::Loon).unwrap();
    assert_eq!(cfg.scripts.len(), 2);
    let rule = &cfg.scripts[0];
    assert!(rule.pattern.is_match("HTTPS://API.INVALID/v1").unwrap());
    assert_eq!(rule.dialect, ScriptDialect::Loon);
    assert_eq!(rule.mode, HookMode::Modify);
    assert_eq!(rule.kind, ScriptKind::HttpRequest);
    assert!(rule.requires_body && rule.binary);
    assert!(!rule.enabled);
    assert_eq!(rule.max_size, 131_072);
    assert_eq!(rule.timeout_ms, Some(2500));
    assert_eq!(rule.argument.as_deref(), Some("a,b"));
    assert_eq!(rule.source_id, cfg.script_urls[0].1);
    assert_eq!(cfg.scripts[1].kind, ScriptKind::HttpResponse);
    assert!(
        !cfg.scripts[1]
            .pattern
            .is_match("HTTPS://API.INVALID/")
            .unwrap()
    );
    assert_eq!(cfg.task_scripts[0].0.cron_expr, "0 15 9 * * *");
    assert_eq!(cfg.task_scripts[0].0.argument.as_deref(), Some("literal"));
    assert_eq!(cfg.task_scripts[0].0.timeout_ms, Some(4000));
    assert_eq!(cfg.task_scripts[1].0.cron_expr, "30 15 9 * * *");
}

#[test]
fn compound_conditions_dynamic_sources_and_unknown_options_are_never_broadened() {
    for entry in [
        r#"request if ${url} ~= /private/ && ${method} == "POST" then script("https://secret.invalid/a.js")"#,
        r#"request if ${url} ~= /private/ then script("${source}")"#,
        r#"request if ${url} ~= /private/ then script("https://secret.invalid/a.js") with engine="webview""#,
        r#"cron "99 99 * * *" then script("https://secret.invalid/a.js")"#,
        r#"response if ${url} ~= /private/g then script("https://secret.invalid/a.js")"#,
    ] {
        let cfg = parse_import(&format!("[Script]\n{entry}"), ScriptDialect::Loon).unwrap();
        assert!(
            cfg.scripts.is_empty() && cfg.script_urls.is_empty() && cfg.task_scripts.is_empty()
        );
        assert!(!cfg.warnings.is_empty());
        assert!(
            cfg.warnings
                .iter()
                .all(|warning| !warning.contains("secret") && !warning.contains("private"))
        );
    }
}
