use std::path::{Path, PathBuf};

use super::*;

fn binary(root: &Path, value: &str) -> PathBuf {
    root.join("cores/sing-box")
        .join(value)
        .join(version::binary_name_on_disk())
}

fn seed(root: &Path, value: &str, reported: &str) -> PathBuf {
    let path = binary(root, value);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!("#!/bin/sh\necho 'sing-box version {reported}'\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

fn save(root: &Path, selected: PathBuf) {
    ClientConfig::new(root.to_path_buf(), "https://hub.example", "token", selected)
        .save()
        .unwrap();
}

#[test]
fn retention_keeps_current_and_one_previous_version_per_channel() {
    let root = tempfile::tempdir().unwrap();
    let core = |version: &str| ManagedCore {
        core_type: CoreType::SingBox,
        version: version.into(),
        environment: None,
        tags: Vec::new(),
        path: binary(root.path(), version),
        active: false,
    };
    let pruned = prune_candidates(
        vec![
            core("1.14.0"),
            core("1.13.15"),
            core("1.13.14"),
            core("1.14.0-beta.4"),
        ],
        "1.14.0",
    );
    let versions: Vec<_> = pruned.iter().map(|core| core.version()).collect();
    assert_eq!(versions, ["1.13.14"]);
}

#[tokio::test]
async fn invalid_versions_never_create_download_paths() {
    let root = tempfile::tempdir().unwrap();
    let service = CoreManagementService::experimental(root.path().to_path_buf());
    for value in [
        "",
        "..",
        "../1.2.3",
        "1.2.3/escape",
        "1.2.3\\escape",
        "1.2.3?x",
        "/1.2.3",
        "1.2",
        "1.2.3-",
        "v../1.2.3",
        "1.2.3\0",
    ] {
        assert!(service.download(value).await.is_err(), "{value:?}");
    }
    assert!(!root.path().join("cores").exists());
    assert!(!root.path().join("client.json").exists());
}

#[test]
fn confinement_requires_exact_inventory_binary_and_rejects_traversal() {
    let root = tempfile::tempdir().unwrap();
    let path = seed(root.path(), "1.2.3", "1.2.3");
    assert!(
        CoreManagementService::validate_managed_binary(root.path(), CoreType::SingBox, &path)
            .is_ok()
    );
    for invalid in [
        root.path().join("sing-box"),
        root.path()
            .join("cores/other/1.2.3")
            .join(version::binary_name_on_disk()),
        root.path()
            .join("cores/sing-box/1.2.3/../1.2.3")
            .join(version::binary_name_on_disk()),
        root.path()
            .join("cores/sing-box/v1.2.3")
            .join(version::binary_name_on_disk()),
        path.parent().unwrap().join("other-binary"),
    ] {
        assert!(
            CoreManagementService::validate_managed_binary(
                root.path(),
                CoreType::SingBox,
                &invalid
            )
            .is_err()
        );
    }
}

#[cfg(unix)]
#[test]
fn selection_merges_latest_only_and_survives_service_reopen() {
    let root = tempfile::tempdir().unwrap();
    let old = seed(root.path(), "1.2.2", "1.2.2");
    let selected = seed(root.path(), "1.2.3", "1.2.3");
    save(root.path(), old);
    let stale = CoreManagementService::experimental(root.path().to_path_buf());
    let file = root.path().join("client.json");
    let mut latest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    latest["hub_url"] = "https://new.example".into();
    latest["future_setting"] = serde_json::json!({"retained": true});
    std::fs::write(&file, serde_json::to_vec(&latest).unwrap()).unwrap();
    stale.select(&selected).unwrap();
    let reopened = CoreManagementService::experimental(root.path().to_path_buf());
    let cores = reopened.list_local().unwrap();
    assert_eq!(cores.iter().filter(|core| core.is_active()).count(), 1);
    assert_eq!(
        cores.iter().find(|core| core.is_active()).unwrap().path(),
        selected
    );
    let after: serde_json::Value =
        crate::settings::load_client(&crate::settings::app_settings_path(root.path())).unwrap();
    latest["core_binary"] = serde_json::to_value(&selected).unwrap();
    if let Some(object) = latest.as_object_mut() {
        object.remove("active_subscription_id");
    }
    assert_eq!(after, latest, "selection changes no unrelated latest field");
    assert_eq!(
        ClientConfig::load(root.path()).unwrap().core_binary,
        selected
    );
    let nested = crate::settings::load_core_settings(root.path())
        .unwrap()
        .unwrap();
    assert_eq!(nested.current.unwrap().version, "1.2.3");
}

#[cfg(unix)]
#[test]
fn probe_mismatch_and_nonexecutable_selection_preserve_previous_bytes() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let previous = seed(root.path(), "1.2.2", "1.2.2");
    save(root.path(), previous);
    let service = CoreManagementService::experimental(root.path().to_path_buf());
    let original = std::fs::read(root.path().join("client.json")).unwrap();
    // A substring match used to accept 1.2.30 as 1.2.3.
    let wrong = seed(root.path(), "1.2.3", "1.2.30");
    assert!(service.select(&wrong).is_err());
    let unexecutable = seed(root.path(), "1.2.4", "1.2.4");
    std::fs::set_permissions(&unexecutable, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(service.select(&unexecutable).is_err());
    assert_eq!(
        std::fs::read(root.path().join("client.json")).unwrap(),
        original
    );
}

#[cfg(unix)]
#[test]
fn stable_external_selection_remains_allowed_experiment_rejects_it() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let outside = seed(external.path(), "1.2.3", "1.2.3");
    assert!(
        CoreManagementService::experimental(root.path().to_path_buf())
            .select(&outside)
            .is_err()
    );
    CoreManagementService::new(root.path().to_path_buf())
        .select(&outside)
        .unwrap();
    assert_eq!(
        ClientConfig::load(root.path()).unwrap().core_binary,
        outside
    );
}

#[test]
fn active_delete_rejected_and_inactive_directory_only_removed() {
    let root = tempfile::tempdir().unwrap();
    let active = seed(root.path(), "1.2.3", "1.2.3");
    let inactive = seed(root.path(), "1.2.2", "1.2.2");
    save(root.path(), active.clone());
    let service = CoreManagementService::experimental(root.path().to_path_buf());
    assert!(service.delete(&active).is_err());
    assert!(active.exists());
    service.delete(&inactive).unwrap();
    assert!(!inactive.parent().unwrap().exists());
    assert!(active.exists());
}

#[cfg(unix)]
#[test]
fn links_and_hardlinks_are_not_experimental_inventory() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let source = seed(outside.path(), "1.2.3", "1.2.3");
    let target = binary(root.path(), "1.2.3");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    symlink(&source, &target).unwrap();
    let service = CoreManagementService::experimental(root.path().to_path_buf());
    assert!(service.select(&target).is_err());
    assert!(service.delete(&target).is_err());
    std::fs::remove_file(&target).unwrap();
    std::fs::hard_link(&source, &target).unwrap();
    assert!(service.select(&target).is_err());
    assert!(source.exists());
    std::fs::remove_file(&target).unwrap();
    std::fs::remove_dir(target.parent().unwrap()).unwrap();
    symlink(source.parent().unwrap(), target.parent().unwrap()).unwrap();
    assert!(service.list_local().is_err());
    assert!(service.delete(&target).is_err());
}

#[cfg(windows)]
#[test]
fn windows_hardlinked_binary_is_not_managed_inventory() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let source = seed(outside.path(), "1.2.3", "1.2.3");
    let target = binary(root.path(), "1.2.3");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::hard_link(&source, &target).unwrap();
    assert!(
        CoreManagementService::validate_managed_binary(root.path(), CoreType::SingBox, &target)
            .is_err()
    );
    assert!(
        CoreManagementService::experimental(root.path().to_path_buf())
            .delete(&target)
            .is_err()
    );
    assert!(source.exists());
}

#[test]
fn interrupted_installation_is_not_published_as_inventory() {
    let root = tempfile::tempdir().unwrap();
    let active = seed(root.path(), "1.2.3", "1.2.3");
    save(root.path(), active.clone());
    let staged = seed(root.path(), "1.2.4", "1.2.4");
    std::fs::rename(
        staged.parent().unwrap(),
        root.path()
            .join("cores/sing-box/.core-download-interrupted"),
    )
    .unwrap();
    let inventory = CoreManagementService::experimental(root.path().to_path_buf())
        .list_local()
        .unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].path(), active);
    assert!(inventory[0].is_active());
}

#[cfg(unix)]
#[tokio::test]
async fn linked_download_directory_never_writes_outside_root() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("cores")).unwrap();
    symlink(outside.path(), root.path().join("cores/sing-box")).unwrap();
    let service = CoreManagementService::experimental(root.path().to_path_buf());
    assert!(service.download("1.2.3").await.is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn cached_download_propagates_selection_save_failure() {
    let root = tempfile::tempdir().unwrap();
    let cached = seed(root.path(), "1.2.3", "1.2.3");
    std::fs::write(root.path().join("client.json"), b"{broken").unwrap();
    let service = CoreManagementService::experimental(root.path().to_path_buf());
    assert!(service.download("1.2.3").await.is_err());
    assert_eq!(
        std::fs::read(root.path().join("client.json")).unwrap(),
        b"{broken"
    );
    assert!(cached.exists());
}

#[cfg(unix)]
#[test]
fn corrupt_configuration_rejects_selection_without_replacing_file() {
    let root = tempfile::tempdir().unwrap();
    let path = seed(root.path(), "1.2.3", "1.2.3");
    let file = root.path().join("client.json");
    std::fs::write(&file, b"{broken").unwrap();
    assert!(
        CoreManagementService::experimental(root.path().to_path_buf())
            .select(&path)
            .is_err()
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"{broken");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
}

#[cfg(unix)]
mod release {
    use super::*;

    fn archive(reported: &str) -> Vec<u8> {
        let script = format!("#!/bin/sh\necho 'sing-box version {reported}'\n");
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(script.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "release/sing-box", script.as_bytes())
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap()
    }

    async fn fixture(name: String, body: Vec<u8>, status: axum::http::StatusCode) -> String {
        let app = axum::Router::new()
            .route("/repos/SagerNet/sing-box/releases/tags/v1.2.3", axum::routing::get(move |headers: axum::http::HeaderMap| {
                let name = name.clone();
                async move {
                    let host = headers.get("host").unwrap().to_str().unwrap();
                    serde_json::json!({"assets":[{"name":name,"browser_download_url":format!("http://{host}/asset")}]}).to_string()
                }
            }))
            .route("/asset", axum::routing::get(move || {
                let body = body.clone();
                async move { (status, body) }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{address}")
    }

    fn asset() -> String {
        let (platform, _) = crate::cores::version::target_spec().unwrap();
        format!("sing-box-1.2.3-{platform}.tar.gz")
    }

    #[tokio::test]
    async fn local_release_pipeline_auto_selects_verified_binary() {
        let root = tempfile::tempdir().unwrap();
        let base = fixture(asset(), archive("1.2.3"), axum::http::StatusCode::OK).await;
        let service =
            CoreManagementService::experimental(root.path().to_path_buf()).with_api_base(base);
        let result = service.download("v1.2.3").await.unwrap();
        assert!(result.is_active());
        assert_eq!(result.version(), "1.2.3");
        assert_eq!(
            ClientConfig::load(root.path()).unwrap().core_binary,
            result.path()
        );
        assert_eq!(service.list_downloaded_versions().unwrap(), ["1.2.3"]);
    }

    #[tokio::test]
    async fn legacy_incomplete_slot_is_completed_without_removing_other_files() {
        let root = tempfile::tempdir().unwrap();
        let slot = root.path().join("cores/sing-box/1.2.3");
        std::fs::create_dir_all(&slot).unwrap();
        let retained = slot.join("prior-download.partial");
        std::fs::write(&retained, b"previous download fragment").unwrap();
        let base = fixture(asset(), archive("1.2.3"), axum::http::StatusCode::OK).await;
        let service =
            CoreManagementService::experimental(root.path().to_path_buf()).with_api_base(base);
        let installed = service.download("1.2.3").await.unwrap();
        assert_eq!(installed.path(), binary(root.path(), "1.2.3"));
        assert_eq!(
            ClientConfig::load(root.path()).unwrap().core_binary,
            installed.path()
        );
        assert_eq!(
            std::fs::read(retained).unwrap(),
            b"previous download fragment"
        );
    }

    #[tokio::test]
    async fn mismatch_and_http_failure_preserve_previous_selection_and_inventory() {
        for (reported, status) in [
            ("1.2.30", axum::http::StatusCode::OK),
            ("1.2.3", axum::http::StatusCode::BAD_GATEWAY),
        ] {
            let root = tempfile::tempdir().unwrap();
            let previous = seed(root.path(), "1.2.2", "1.2.2");
            save(root.path(), previous.clone());
            let bytes = std::fs::read(root.path().join("client.json")).unwrap();
            let base = fixture(asset(), archive(reported), status).await;
            let service =
                CoreManagementService::experimental(root.path().to_path_buf()).with_api_base(base);
            assert!(service.download("1.2.3").await.is_err());
            assert_eq!(
                std::fs::read(root.path().join("client.json")).unwrap(),
                bytes
            );
            assert!(previous.exists());
            assert!(!binary(root.path(), "1.2.3").exists());
            assert_eq!(
                std::fs::read_dir(root.path().join("cores/sing-box"))
                    .unwrap()
                    .count(),
                1
            );
        }
    }

    #[tokio::test]
    async fn untrusted_asset_name_cannot_escape_staging() {
        let root = tempfile::tempdir().unwrap();
        let base = fixture(
            format!("../../{}", asset()),
            archive("1.2.3"),
            axum::http::StatusCode::OK,
        )
        .await;
        let service =
            CoreManagementService::experimental(root.path().to_path_buf()).with_api_base(base);
        assert!(service.download("1.2.3").await.is_err());
        assert!(!root.path().join("client.json").exists());
        assert_eq!(
            std::fs::read_dir(root.path().join("cores/sing-box"))
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn archive_link_cannot_be_installed_as_binary() {
        let root = tempfile::tempdir().unwrap();
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("/outside/sing-box").unwrap();
        header.set_cksum();
        tar.append_data(&mut header, "release/sing-box", std::io::empty())
            .unwrap();
        let body = tar.into_inner().unwrap().finish().unwrap();
        let base = fixture(asset(), body, axum::http::StatusCode::OK).await;
        let service =
            CoreManagementService::experimental(root.path().to_path_buf()).with_api_base(base);
        assert!(service.download("1.2.3").await.is_err());
        assert!(!binary(root.path(), "1.2.3").exists());
        assert!(!root.path().join("client.json").exists());
    }
}

#[cfg(unix)]
#[test]
fn active_core_survives_type_switch_and_exposes_build_tags() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cores/mihomo/1.19.29/mihomo");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         echo 'Mihomo Meta v1.19.29 linux amd64 with go1.25'\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    save(root.path(), path.clone());

    let service = CoreManagementService::experimental(root.path().to_path_buf());
    let active = service.active_core().unwrap();
    assert_eq!(active.core_type(), CoreType::Mihomo);
    assert_eq!(active.version(), "1.19.29");
    assert_eq!(active.path(), path);

    assert!(service.for_core(CoreType::Mihomo).list_local().unwrap()[0].is_active());
    assert!(
        service
            .for_core(CoreType::SingBox)
            .list_local()
            .unwrap()
            .is_empty()
    );

    service.for_core(CoreType::Mihomo).select(&path).unwrap();
    let nested =
        crate::settings::load_app_section::<crate::settings::CoreSettings>(root.path(), "core")
            .unwrap()
            .unwrap();
    assert_eq!(nested.kind, CoreType::Mihomo);
    assert_eq!(
        nested.current.as_ref().unwrap().path,
        Path::new("cores/mihomo/1.19.29/mihomo")
    );
    assert_eq!(
        ClientConfig::load(root.path()).unwrap().core_type,
        CoreType::Mihomo
    );
}

#[cfg(unix)]
#[test]
fn active_core_details_include_singbox_environment_and_tags() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cores/sing-box/1.15.0/sing-box");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         echo 'sing-box version 1.15.0'\n\
         echo 'Environment: go1.25.1 linux/amd64'\n\
         echo 'Tags: with_quic,with_dhcp,with_gvisor'\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    save(root.path(), path);

    let active = CoreManagementService::experimental(root.path().to_path_buf())
        .active_core()
        .unwrap();
    assert_eq!(active.environment(), Some("go1.25.1 linux/amd64"));
    assert_eq!(active.tags(), ["with_quic", "with_dhcp", "with_gvisor"]);
}
#[tokio::test]
#[ignore = "downloads the real Mihomo release from GitHub"]
async fn downloads_and_verifies_real_mihomo_alpha() {
    let root = tempfile::tempdir().unwrap();
    let service =
        CoreManagementService::experimental(root.path().to_path_buf()).for_core(CoreType::Mihomo);
    let remote = service.list_remote_channels().await.unwrap();
    let alpha = remote
        .iter()
        .find(|channel| channel.channel() == "prerelease")
        .expect("Mihomo Alpha channel must be available");
    let core = service.download(alpha.version()).await.unwrap();
    assert_eq!(core.core_type(), CoreType::Mihomo);
    assert_eq!(core.version(), alpha.version());
    assert!(core.path().is_file());
    assert_eq!(
        service
            .for_core(CoreType::Mihomo)
            .active_core()
            .unwrap()
            .path(),
        core.path()
    );
}

#[cfg(windows)]
#[test]
fn canonical_windows_data_root_exposes_its_selected_inventory() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let path = seed(&root, "1.2.3", "1.2.3");
    save(&root, path.clone());
    let service = CoreManagementService::experimental(root);
    let inventory = service.list_local().unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].path(), path);
    assert!(inventory[0].is_active());
}
