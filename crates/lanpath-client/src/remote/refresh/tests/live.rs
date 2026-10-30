use super::*;
use crate::{ClientConfig, RequestProxyMode};

fn port(server: &Server) -> u16 {
    reqwest::Url::parse(&server.base).unwrap().port().unwrap()
}

fn config(mode: RequestProxyMode) -> ClientConfig {
    ClientConfig {
        request_proxy_mode: mode,
        ..Default::default()
    }
}

#[tokio::test]
async fn owned_core_and_owned_proxy_policies_bypass_main_listener_without_direct_requests() {
    let main = Server::start().await;
    let returning = Server::start().await;
    let origin = Server::start().await;
    main.set("script.js", StatusCode::OK, "const intercepted = true;");
    returning.set("script.js", StatusCode::OK, "const bypassed = true;");
    origin.set("script.js", StatusCode::OK, "const leaked = true;");
    for mode in [
        RequestProxyMode::Core,
        RequestProxyMode::Custom,
        RequestProxyMode::System,
    ] {
        let root = tempfile::tempdir().unwrap();
        let manager = RemoteManager::new(root.path().into());
        let mut saved = config(mode);
        // Deliberately stale saved port: only the admitted active ports are authority.
        saved.mixed_port = port(&origin);
        saved.request_proxy_url = main.base.clone();
        let context =
            RemoteFetchContext::running(saved, port(&main), port(&returning), true, Vec::new())
                .unwrap();
        let remote = origin.resource("script", "script.js");
        let report = fetch_at(&manager, &[remote], Some(&context), || 100)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(report.fetched, 1);
        assert_eq!(
            std::fs::read_to_string(root.path().join("scripts/script.js")).unwrap(),
            "const bypassed = true;"
        );
    }
    assert_eq!(main.requests(), 0);
    assert_eq!(returning.requests(), 3);
    assert_eq!(origin.requests(), 0);
}

#[tokio::test]
async fn unrelated_custom_policy_remains_explicit_instead_of_using_owned_core() {
    let main = Server::start().await;
    let returning = Server::start().await;
    let custom = Server::start().await;
    let origin = Server::start().await;
    custom.set("script.js", StatusCode::OK, "const custom = true;");
    origin.set("script.js", StatusCode::OK, "const leaked = true;");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let mut saved = config(RequestProxyMode::Custom);
    saved.request_proxy_url = custom.base.clone();
    let context =
        RemoteFetchContext::running(saved, port(&main), port(&returning), false, Vec::new())
            .unwrap();
    assert_eq!(
        fetch_at(
            &manager,
            &[origin.resource("script", "script.js")],
            Some(&context),
            || 100
        )
        .await
        .unwrap()
        .unwrap()
        .fetched,
        1
    );
    assert_eq!(custom.requests(), 1);
    assert_eq!(main.requests(), 0);
    assert_eq!(returning.requests(), 0);
    assert_eq!(origin.requests(), 0);
}

#[tokio::test]
async fn owned_custom_proxy_cannot_downgrade_tls_or_discard_credentials() {
    let owned = Server::start().await;
    for authority in [
        format!("https://127.0.0.1:{}", port(&owned)),
        format!("http://owned-user:owned-secret@127.0.0.1:{}", port(&owned)),
        format!(
            "socks5://owned-user:owned-secret@127.0.0.1:{}",
            port(&owned)
        ),
    ] {
        let mut saved = config(RequestProxyMode::Custom);
        saved.request_proxy_url = authority;
        let error =
            match RemoteFetchContext::running(saved, port(&owned), port(&owned), false, Vec::new())
            {
                Ok(_) => panic!("explicit proxy security settings were silently replaced"),
                Err(error) => error.to_string(),
            };
        assert!(!error.contains("owned-secret"));
        assert!(!error.contains("owned-user"));
    }
    assert_eq!(owned.requests(), 0);
}

#[tokio::test]
async fn stopped_core_rejects_foreign_saved_listener_and_unavailable_route_never_leaks_direct() {
    let foreign = Server::start().await;
    let origin = Server::start().await;
    origin.set("script.js", StatusCode::OK, "const leaked = true;");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let mut saved = config(RequestProxyMode::Core);
    saved.mixed_port = port(&foreign);
    assert!(RemoteFetchContext::stopped(saved).is_err());
    assert_eq!(foreign.requests(), 0);
    assert!(!root.path().join("remote_refresh.json").exists());

    // An accepted endpoint can disappear: its connection failure must never retry direct.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let unavailable = listener.local_addr().unwrap().port();
    drop(listener);
    let context = RemoteFetchContext::running(
        config(RequestProxyMode::Core),
        port(&foreign),
        unavailable,
        false,
        Vec::new(),
    )
    .unwrap();
    let report = fetch_at(
        &manager,
        &[origin.resource("script", "script.js")],
        Some(&context),
        || 100,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(report.fetched, 0);
    assert!(!report.warnings.is_empty());
    assert_eq!(origin.requests(), 0);
    assert_eq!(foreign.requests(), 0);
    assert!(!root.path().join("scripts").exists());
}

#[tokio::test]
async fn dependent_hook_and_task_keep_accepted_policy_after_client_json_changes() {
    let proxy = Server::start().await;
    let origin = Server::start().await;
    proxy.set("module", StatusCode::OK, format!(
        "[Script]\nhook=type=http-request,pattern=^https://example\\.invalid/,script-path={}/gated.js\ntask=type=cron,cronexp=\"0 0 * * *\",script-path={}/task.js",
        origin.base, origin.base,
    ));
    proxy.set("gated.js", StatusCode::OK, "const acceptedHook = true;");
    proxy.set("task.js", StatusCode::OK, "const acceptedTask = true;");
    origin.set("task.js", StatusCode::OK, "const leaked = true;");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let mut remote = origin.resource("module", "module");
    remote.kind = RemoteKind::Snippet;
    manager.save(std::slice::from_ref(&remote)).unwrap();
    let mut saved = config(RequestProxyMode::Core);
    saved.data_dir = root.path().into();
    saved.mixed_port = port(&proxy);
    saved.save().unwrap();
    let context = RemoteFetchContext::running(
        ClientConfig::load_for_requests(root.path()).unwrap(),
        port(&proxy),
        port(&proxy),
        false,
        Vec::new(),
    )
    .unwrap();
    let resources = [remote];
    let mut refresh = Box::pin(fetch_at(&manager, &resources, Some(&context), || 100));
    tokio::select! {
        result = &mut refresh => panic!("refresh unexpectedly completed: {result:?}"),
        () = proxy.state.entered.notified() => {},
    }
    saved.request_proxy_mode = RequestProxyMode::None;
    saved.github_proxy_prefix = "http://127.0.0.1:1/private-prefix".into();
    saved.save().unwrap();
    proxy.state.release.notify_one();
    let report = refresh.await.unwrap().unwrap();
    assert_eq!((report.fetched, report.scripts, report.tasks), (1, 1, 1));
    assert_eq!(proxy.requests(), 3);
    assert_eq!(origin.requests(), 0);
    let active = manager.load_cached().unwrap();
    assert!(active.scripts[0].source.contains("acceptedHook"));
    assert!(active.task_scripts[0].source.contains("acceptedTask"));
}

#[tokio::test]
async fn resources_becoming_due_during_download_wait_for_the_next_admitted_batch() {
    let server = Server::start().await;
    server.set("gated.js", StatusCode::OK, "const admitted = true;");
    server.set("script.js", StatusCode::OK, "const active = true;");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let resources = [
        server.resource("first", "gated.js"),
        server.resource("second", "script.js"),
    ];
    run(&manager, &resources[1..], true, 95).await.unwrap();
    let active = root.path().join("scripts/second.js");
    let previous = std::fs::read(&active).unwrap();
    let before = server.requests();
    let context = RemoteFetchContext::running(
        config(RequestProxyMode::None),
        port(&server),
        port(&server),
        false,
        vec![active.clone()],
    )
    .unwrap();
    let now = AtomicUsize::new(100);
    let mut refresh = Box::pin(fetch_at(&manager, &resources, Some(&context), || {
        now.load(Ordering::SeqCst) as u64
    }));
    tokio::select! {
        result = &mut refresh => panic!("refresh unexpectedly completed: {result:?}"),
        () = server.state.entered.notified() => {},
    }
    now.store(110, Ordering::SeqCst);
    server.state.release.notify_one();
    let report = refresh.await.unwrap().unwrap();
    assert_eq!(report.fetched, 1);
    assert_eq!(server.requests(), before + 1);
    assert_eq!(std::fs::read(&active).unwrap(), previous);
    let schedule = Schedule::load(&root.path().join("remote_refresh.json")).unwrap();
    assert_eq!(schedule.attempts[&identity(&resources[0])], 110);
    assert_eq!(schedule.attempts[&identity(&resources[1])], 95);
}

#[tokio::test]
async fn any_protected_batch_destination_defers_before_network_and_preserves_all_bytes_and_ages() {
    let server = Server::start().await;
    server.set("script.js", StatusCode::OK, "const replacement = true;");
    server.set(
        "module",
        StatusCode::OK,
        "#!arguments=token:default\n[MITM]\nhostname=new.invalid",
    );
    for protected in [
        "scripts/script.js",
        "remote_cache",
        "remotes.json",
        "remote_refresh.json",
        "REMOTES.JSON",
    ] {
        let root = tempfile::tempdir().unwrap();
        let manager = RemoteManager::new(root.path().into());
        let script = server.resource("script", "script.js");
        let mut module = server.resource("module", "module");
        module.kind = RemoteKind::Snippet;
        let resources = [script, module];
        manager.save(&resources).unwrap();
        run(&manager, &resources, true, 100).await.unwrap();
        let id = crate::remote::activation::remote_module_id(&resources[1]).replace(':', "-");
        let paths = [
            root.path().join("scripts/script.js"),
            root.path().join(format!("remote_cache/{id}.json")),
            manager.remotes_file(),
            root.path().join("remote_refresh.json"),
        ];
        let original: Vec<_> = paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();
        let before = server.requests();
        let context = RemoteFetchContext::running(
            config(RequestProxyMode::None),
            port(&server),
            port(&server),
            false,
            vec![root.path().join(protected)],
        )
        .unwrap();
        let error = fetch_at(&manager, &resources, Some(&context), || 110)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("deferred"));
        assert!(!error.contains(root.path().to_str().unwrap()));
        assert_eq!(server.requests(), before);
        for (path, bytes) in paths.iter().zip(original) {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn redirected_cache_ancestor_is_rechecked_before_publication_without_aging() {
    use std::os::unix::fs::symlink;
    let server = Server::start().await;
    server.set("gated.js", StatusCode::OK, "const replacement = true;");
    let root = tempfile::tempdir().unwrap();
    let safe = root.path().join("safe");
    let protected = root.path().join("protected");
    std::fs::create_dir(&safe).unwrap();
    std::fs::create_dir(&protected).unwrap();
    std::fs::write(safe.join("gated.js"), "const retained = true;").unwrap();
    std::fs::write(protected.join("gated.js"), "const live = true;").unwrap();
    let scripts = root.path().join("scripts");
    symlink(&safe, &scripts).unwrap();
    let manager = RemoteManager::new(root.path().into());
    let resources = [server.resource("gated", "gated.js")];
    manager.save(&resources).unwrap();
    let manifest = std::fs::read(manager.remotes_file()).unwrap();
    let context = RemoteFetchContext::running(
        config(RequestProxyMode::None),
        port(&server),
        port(&server),
        false,
        vec![protected.clone()],
    )
    .unwrap();
    let mut refresh = Box::pin(fetch_at(&manager, &resources, Some(&context), || 100));
    tokio::select! {
        result = &mut refresh => panic!("refresh unexpectedly completed: {result:?}"),
        () = server.state.entered.notified() => {},
    }
    std::fs::remove_file(&scripts).unwrap();
    symlink(&protected, &scripts).unwrap();
    server.state.release.notify_one();
    assert!(refresh.await.unwrap_err().to_string().contains("deferred"));
    assert_eq!(
        std::fs::read_to_string(protected.join("gated.js")).unwrap(),
        "const live = true;"
    );
    assert_eq!(
        std::fs::read_to_string(safe.join("gated.js")).unwrap(),
        "const retained = true;"
    );
    assert_eq!(std::fs::read(manager.remotes_file()).unwrap(), manifest);
    assert!(!root.path().join("remote_refresh.json").exists());
}

#[tokio::test]
async fn lost_ownership_before_redirect_prevents_foreign_request_and_attempt_age() {
    let destination = Server::start().await;
    destination.set("script.js", StatusCode::OK, "const leaked = true;");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let request_port = listener.local_addr().unwrap().port();
    let owned = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let route = format!("{}/script.js", destination.base);
    let app = axum::Router::new().fallback(get({
        let owned = owned.clone();
        move || {
            let owned = owned.clone();
            let route = route.clone();
            async move {
                owned.store(false, Ordering::SeqCst);
                (
                    StatusCode::FOUND,
                    [(axum::http::header::LOCATION, route)],
                    "",
                )
            }
        }
    }));
    let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let context = RemoteFetchContext::running(
        config(RequestProxyMode::Core),
        request_port,
        request_port,
        false,
        Vec::new(),
    )
    .unwrap()
    .with_live_guard(Arc::new(move || {
        if owned.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(lanpath_common::LanpathError::Client(
                "ownership lost".into(),
            ))
        }
    }));
    let error = fetch_at(
        &manager,
        &[destination.resource("script", "script.js")],
        Some(&context),
        || 100,
    )
    .await
    .unwrap_err()
    .to_string();
    worker.abort();
    assert!(error.contains("unavailable"));
    assert_eq!(destination.requests(), 0);
    assert!(!root.path().join("scripts").exists());
    assert!(!root.path().join("remote_refresh.json").exists());
}
