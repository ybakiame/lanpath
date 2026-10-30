use crate::import::parse_import;
use lanpath_mitm::{RewriteAction, RewriteEngine};
use lanpath_script::{ScriptDialect, ScriptKind};

#[test]
fn stash_yaml_pairs_remote_provider_with_the_exact_hook_and_port_scope() {
    let cfg = parse_import(
        r#"
script-providers:
  local:
    path: private.js
  remote:
    url: https://assets.invalid/script.js
http:
  mitm:
    - api.invalid
    - '*.cdn.invalid:8443'
    - '-excluded.invalid'
  script:
    - name: local
      match: '^https://local.invalid/'
      type: request
    - name: remote
      match: '^https://remote.invalid/'
      type: response
      require-body: true
      binary-mode: true
      timeout: 1.5
      argument: 'a,b'
      max-size: -1
      enabled: false
  url-rewrite:
    - '^https://old.invalid/(.*) https://new.invalid/$1 307'
    - '^https://ads.invalid/ - reject'
"#,
        ScriptDialect::Stash,
    )
    .unwrap();
    assert_eq!(cfg.scripts.len(), 1);
    let rule = &cfg.scripts[0];
    assert_eq!(rule.name, "remote");
    assert_eq!(rule.source_id, "https://assets.invalid/script.js");
    assert_eq!(cfg.script_urls, [("remote".into(), rule.source_id.clone())]);
    assert_eq!(rule.dialect, ScriptDialect::Stash);
    assert_eq!(rule.kind, ScriptKind::HttpResponse);
    assert!(rule.requires_body && rule.binary && !rule.full_headers);
    assert!(!rule.enabled);
    assert_eq!(rule.timeout_ms, Some(1500));
    assert_eq!(rule.max_size, usize::MAX);
    assert_eq!(rule.argument.as_deref(), Some("a,b"));
    assert_eq!(
        cfg.hostnames,
        [
            "api.invalid:443",
            "*.cdn.invalid:8443",
            "-excluded.invalid:443"
        ]
    );
    let engine = RewriteEngine {
        rules: cfg.rewrites,
    };
    assert_eq!(
        engine
            .apply_request(
                &mut "https://old.invalid/path".into(),
                &mut Vec::new(),
                &mut None
            )
            .unwrap(),
        RewriteAction::Redirect {
            status: 307,
            location: "https://new.invalid/path".into()
        }
    );
    assert!(matches!(
        engine
            .apply_request(
                &mut "https://ads.invalid/".into(),
                &mut Vec::new(),
                &mut None
            )
            .unwrap(),
        RewriteAction::Mock { status: 404, .. }
    ));
}

#[test]
fn unsupported_stash_auth_provider_or_engine_never_becomes_plain_download() {
    for provider in [
        "url: https://secret.invalid/a.js\n    headers: {Authorization: secret}",
        "path: secret.js",
        "url: https://secret.invalid/a.js",
    ] {
        let engine = if provider.ends_with("a.js") {
            "\n      engine: webview"
        } else {
            ""
        };
        let cfg = parse_import(&format!("script-providers:\n  hook:\n    {provider}\nhttp:\n  script:\n    - name: hook\n      match: private\n      type: request{engine}"), ScriptDialect::Stash).unwrap();
        assert!(cfg.scripts.is_empty() && cfg.script_urls.is_empty());
        assert!(!cfg.warnings.is_empty());
        assert!(
            cfg.warnings
                .iter()
                .all(|warning| !warning.contains("secret") && !warning.contains("private"))
        );
    }
}
