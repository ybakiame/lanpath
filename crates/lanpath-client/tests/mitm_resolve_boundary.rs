//! Opt-in real-core regression: selected HTTPS mock responses need no origin DNS.
#![cfg(all(feature = "mitm", not(target_os = "android")))]

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures::FutureExt;
use lanpath_client::{
    config::ClientConfig,
    core_config::{MitmChain, PanelFeatures, apply_panel_features, compose_singbox_config},
    mitm::{MitmBuildOptions, build_mitm_proxy},
    rules::{LocalRule, RuleAction, RuleMatchType, apply_singbox_rules},
    runner::CoreRunner,
};
use lanpath_mitm::{MemoryRecorder, RewriteEngine, RewriteKind, RewriteRule};
use serde_json::json;
use tokio::net::{TcpListener, UdpSocket};

fn singbox_binary() -> PathBuf {
    let path = std::env::var_os("LANPATH_TEST_SINGBOX")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-cores/sing-box")
        });
    assert!(
        path.is_file(),
        "set LANPATH_TEST_SINGBOX to a real sing-box binary"
    );
    path
}

fn proxy_client(port: u16, ca: &[u8]) -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).unwrap())
        .tls_built_in_root_certs(false)
        .add_root_certificate(reqwest::Certificate::from_pem(ca).unwrap())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

async fn body(client: &reqwest::Client, url: &str) -> Option<String> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.text().await.ok()
}

async fn ready(port: u16) {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut poll = tokio::time::interval(Duration::from_millis(20));
        loop {
            poll.tick().await;
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                return;
            }
        }
    })
    .await
    .expect("core mixed inbound did not become ready");
}

#[tokio::test]
#[ignore = "requires a real sing-box binary; loopback only, no OS trust or proxy changes"]
async fn real_singbox_mitm_mock_skips_only_selected_main_in_resolution() {
    let binary = singbox_binary();
    let dir = tempfile::tempdir().unwrap();
    let main_reservation = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let return_reservation = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_reservation = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let main_port = main_reservation.local_addr().unwrap().port();
    let return_port = return_reservation.local_addr().unwrap().port();
    let api_port = api_reservation.local_addr().unwrap().port();
    let hostnames = vec![
        "one.probe.invalid:443".into(),
        "*.two.probe.invalid:8443".into(),
        "-excluded.two.probe.invalid:8443".into(),
        "[::1]:443".into(),
    ];

    // A real loopback DNS server returns NXDOMAIN immediately and records every lookup.
    // No unavailable upstream, external DNS, cache hit or fixed sleep can fake success.
    let dns = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dns_port = dns.local_addr().unwrap().port();
    let lookups = Arc::new(AtomicUsize::new(0));
    let observed = lookups.clone();
    let (dns_stop, mut stopped) = tokio::sync::oneshot::channel();
    let dns_task = tokio::spawn(async move {
        let mut packet = [0; 4096];
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                result = dns.recv_from(&mut packet) => {
                    let (length, peer) = result.unwrap();
                    assert!(length >= 12, "truncated DNS query");
                    assert_eq!(&packet[4..6], &[0, 1], "expected one DNS question");
                    let mut question_end = 12;
                    loop {
                        assert!(question_end < length, "truncated DNS name");
                        let label = usize::from(packet[question_end]);
                        assert!(label < 64, "unexpected compressed query name");
                        question_end += 1 + label;
                        if label == 0 {
                            break;
                        }
                    }
                    question_end += 4; // QTYPE + QCLASS, excluding any request EDNS record
                    assert!(question_end <= length, "truncated DNS question");
                    observed.fetch_add(1, Ordering::SeqCst);
                    packet[2] = 0x81;
                    packet[3] = 0x83; // response, recursion available, NXDOMAIN
                    packet[6..12].fill(0); // no answer/authority/additional records
                    dns.send_to(&packet[..question_end], peer).await.unwrap();
                }
            }
        }
    });

    let mut client_config = ClientConfig::new(dir.path().to_path_buf(), "", "", binary.clone());
    client_config.mixed_port = main_port;
    client_config.mitm.hostnames = hostnames.clone();
    let mut mitm = build_mitm_proxy(
        &client_config,
        MitmBuildOptions {
            upstream_port: Some(return_port),
            rewrite: RewriteEngine {
                rules: vec![RewriteRule {
                    pattern: lanpath_mitm::Pattern::new(r"^https://[^/]+/mock$").unwrap(),
                    kind: RewriteKind::Mock {
                        status: 200,
                        body: b"selected-mitm-response".to_vec(),
                        headers: vec![],
                    },
                }],
            },
            ..Default::default()
        },
        Arc::new(MemoryRecorder::new(64)),
    )
    .unwrap()
    .start()
    .await
    .unwrap();
    // Trust only this fixture's public CA in this client. Never install it into OS trust.
    let public_ca = std::fs::read(client_config.mitm.ca_dir.join("ca.crt")).unwrap();
    let subscription = json!({
        "dns": {
            "servers": [{"type": "udp", "tag": "fixture", "server": "127.0.0.1", "server_port": dns_port}],
            "final": "fixture", "disable_cache": true
        },
        "outbounds": [
            {"type": "direct", "tag": "direct"},
            {"type": "selector", "tag": "global", "outbounds": ["direct"]}
        ],
        "route": {"final": "direct"}
    });
    let local_rules = vec![LocalRule {
        id: "fixture-local-reject".into(),
        name: String::new(),
        enabled: true,
        match_type: RuleMatchType::Domain,
        target: "local.two.probe.invalid".into(),
        action: RuleAction::Reject,
        advanced: Default::default(),
        note: String::new(),
        created_at: 0,
        sort_order: 0,
        builtin: false,
    }];
    let runner =
        CoreRunner::create(lanpath_common::CoreType::SingBox, &binary, dir.path()).unwrap();
    drop((main_reservation, return_reservation, api_reservation));
    // Always stop the owned fixture core before propagating an assertion panic.
    let outcome = std::panic::AssertUnwindSafe(async {
        for mode in ["rule", "direct", "global", "owned"] {
            let mut config = compose_singbox_config(
                &subscription,
                main_port,
                Some(MitmChain {
                    proxy_addr: mitm.addr,
                    return_port,
                    hostnames: hostnames.clone(),
                }),
            )
            .unwrap();
            if mode == "owned" {
                config["route"]["rules"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({
                        "clash_mode": "rule", "domain": ["owned.invalid"], "outbound": "direct"
                    }));
            }
            apply_singbox_rules(&mut config, &local_rules);
            let features = PanelFeatures {
                tun_enabled: false,
                tun_stack: "mixed".into(),
                tun_auto_route: false,
                clash_api_enabled: true,
                clash_api_port: api_port,
                clash_api_secret: String::new(),
                clash_api_ui: "zashboard".into(),
                rule_mode: if mode == "owned" { "rule" } else { mode }.into(),
                ipv6_enabled: false,
                dns_fakeip_enabled: false,
                dns_mode: lanpath_client::config_slices::DnsMode::FollowSystem,
                github_proxy_prefix: String::new(),
                data_dir: dir.path().to_string_lossy().into_owned(),
            };
            apply_panel_features(&mut config, &features);
            // This routing fixture has no dashboard assets or network downloads.
            let api = config["experimental"]["clash_api"].as_object_mut().unwrap();
            api.remove("external_ui");
            api.remove("external_ui_download_url");
            runner.start(&config).await.unwrap();
            ready(main_port).await;
            ready(return_port).await;
            let client = proxy_client(main_port, &public_ca);
            let return_client = proxy_client(return_port, &public_ca);
            let before = lookups.load(Ordering::SeqCst);
            if mode == "rule" || mode == "owned" {
                for url in [
                    "https://one.probe.invalid/mock",
                    "https://sub.two.probe.invalid:8443/mock",
                ] {
                    assert_eq!(
                        body(&client, url).await.as_deref(),
                        Some("selected-mitm-response")
                    );
                }
                assert_eq!(
                    lookups.load(Ordering::SeqCst),
                    before,
                    "MITM mock resolved origin"
                );
                for url in [
                    "https://local.two.probe.invalid:8443/mock", // local reject outranks MITM
                    "https://[::1]/mock",                        // IPv6 rejection outranks MITM
                ] {
                    assert!(body(&client, url).await.is_none(), "policy bypassed: {url}");
                }
                assert_eq!(lookups.load(Ordering::SeqCst), before);
                for (proxy, url) in [
                    (&client, "https://one.probe.invalid:8443/mock"), // no host/port cross product
                    (&client, "https://sub.two.probe.invalid/mock"),
                    (&client, "https://excluded.two.probe.invalid:8443/mock"),
                    (&client, "https://outside.invalid/mock"),
                    (&return_client, "https://one.probe.invalid/mock"),
                    (&client, "https://one.probe.invalid/forward"), // real MITM return path
                ] {
                    let before = lookups.load(Ordering::SeqCst);
                    assert!(
                        body(proxy, url).await.is_none(),
                        "unexpected success: {url}"
                    );
                    assert!(
                        lookups.load(Ordering::SeqCst) > before,
                        "missing normal DNS: {url}"
                    );
                }
            } else {
                assert!(
                    body(&client, "https://one.probe.invalid/mock")
                        .await
                        .is_none()
                );
                assert!(
                    lookups.load(Ordering::SeqCst) > before,
                    "mode {mode} skipped normal DNS"
                );
            }
            if mode == "owned" {
                let api = reqwest::Client::builder().no_proxy().build().unwrap();
                let endpoint = format!("http://127.0.0.1:{api_port}/configs");
                api.patch(&endpoint)
                    .json(&json!({"mode": "global"}))
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap();
                let current: serde_json::Value = api
                    .get(&endpoint)
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                assert_eq!(
                    current["mode"], "rule",
                    "unowned global mode became available"
                );
                let before = lookups.load(Ordering::SeqCst);
                assert_eq!(
                    body(&client, "https://one.probe.invalid/mock")
                        .await
                        .as_deref(),
                    Some("selected-mitm-response")
                );
                assert_eq!(lookups.load(Ordering::SeqCst), before);
            }
            drop((client, return_client));
            runner.stop().await.unwrap();
        }
    })
    .catch_unwind()
    .await;
    runner.stop().await.unwrap();
    mitm.shutdown().await.unwrap();
    let _ = dns_stop.send(());
    dns_task.await.unwrap();
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
