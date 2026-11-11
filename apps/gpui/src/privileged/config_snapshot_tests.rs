use serde_json::{Value, json};

use super::{admit, snapshot_resources};

const TLS_INPUT_PATHS: &[&str] = &[
    "certificate_path",
    "client_certificate_path",
    "client_key_path",
];

fn document() -> Value {
    json!({
        "inbounds": [{ "type": "tun" }],
        "outbounds": [{ "type": "trojan", "tls": {} }]
    })
}

#[test]
fn resource_consumers_reject_paths_escaping_the_owned_root() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let outside = parent.path().join("outside.pem");
    std::fs::write(&outside, b"outside certificate").unwrap();
    let session = root.join("session");

    for path in [
        "../outside.pem".to_owned(),
        outside.to_string_lossy().into_owned(),
    ] {
        for value in [json!(path), json!([path])] {
            for key in TLS_INPUT_PATHS {
                let mut config = document();
                config["outbounds"][0]["tls"][key] = value.clone();
                assert!(
                    snapshot_resources(&root, &mut config).is_err(),
                    "{key}: {value}"
                );
                assert!(admit(&mut config, &[], &session, "managed-tun").is_err());
            }
            let mut config = document();
            config["route"] = json!({ "rule_set": [{ "type": "local", "path": value }] });
            assert!(snapshot_resources(&root, &mut config).is_err());
            assert!(admit(&mut config, &[], &session, "managed-tun").is_err());
        }
    }
    for value in [
        json!("lanpath-resource:../outside.pem"),
        json!(["lanpath-resource:unknown"]),
        json!([true]),
        json!([["outside.pem"]]),
    ] {
        let mut config = document();
        config["outbounds"][0]["tls"]["certificate_path"] = value;
        assert!(snapshot_resources(&root, &mut config).is_err());
        assert!(admit(&mut config, &[], &session, "managed-tun").is_err());
    }
}

#[cfg(unix)]
#[test]
fn resource_consumers_reject_linked_paths_even_inside_the_owned_root() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let outside = parent.path().join("outside.pem");
    std::fs::write(&outside, b"outside certificate").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("symlink.pem")).unwrap();
    std::fs::hard_link(&outside, root.join("hardlink.pem")).unwrap();
    std::os::unix::fs::symlink(parent.path(), root.join("linked-parent")).unwrap();

    for path in ["symlink.pem", "hardlink.pem", "linked-parent/outside.pem"] {
        let mut config = document();
        config["outbounds"][0]["tls"]["certificate_path"] = json!([path]);
        assert!(snapshot_resources(&root, &mut config).is_err(), "{path}");
    }
}

#[test]
fn snapshotted_path_arrays_read_the_owned_root_not_the_process_cwd() {
    let root = tempfile::tempdir().unwrap();
    let canonical_root = root.path().canonicalize().unwrap();
    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
    assert_ne!(cwd, canonical_root);
    let relative = format!(
        "{}-ca.pem",
        root.path().file_name().unwrap().to_string_lossy()
    );
    assert!(!cwd.join(&relative).exists());
    let first = canonical_root.join(&relative);
    let second = canonical_root.join("client.pem");
    let session = canonical_root.join("private-session");
    std::fs::write(&first, b"root CA certificate").unwrap();
    std::fs::write(&second, b"root client certificate").unwrap();

    for key in TLS_INPUT_PATHS {
        let mut config = document();
        config["outbounds"][0]["tls"][key] = json!([relative, second.to_string_lossy()]);
        config["outbounds"].as_array_mut().unwrap().push(json!({
            "type": "trojan", "tls": { "certificate_path": [] }
        }));
        let resources = snapshot_resources(&canonical_root, &mut config).unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0].bytes, b"root CA certificate");
        assert_eq!(resources[1].bytes, b"root client certificate");
        assert_eq!(
            config["outbounds"][0]["tls"][key],
            json!(["lanpath-resource:resource-0", "lanpath-resource:resource-1"])
        );

        // Once frozen, admission needs only the authoritative resource set,
        // not the original files or the UI's working directory.
        std::fs::remove_file(&first).unwrap();
        std::fs::remove_file(&second).unwrap();
        admit(&mut config, &resources, &session, "managed-tun").unwrap();
        assert_eq!(
            config["outbounds"][0]["tls"][key],
            json!([
                session.join("resources/resource-0").to_string_lossy(),
                session.join("resources/resource-1").to_string_lossy()
            ])
        );
        assert_eq!(config["outbounds"][1]["tls"]["certificate_path"], json!([]));
        std::fs::write(&first, b"root CA certificate").unwrap();
        std::fs::write(&second, b"root client certificate").unwrap();
    }
}

#[test]
fn hosts_and_ech_files_remain_immutable_after_the_originals_are_removed() {
    let root = tempfile::tempdir().unwrap();
    let session = root.path().join("session");
    std::fs::write(root.path().join("hosts.txt"), b"127.0.0.1 owned.test\n").unwrap();
    std::fs::write(root.path().join("ech.txt"), b"owned ECH configuration").unwrap();
    let mut value = document();
    value["dns"] =
        json!({"servers": [{"type": "hosts", "tag": "owned-hosts", "path": ["hosts.txt"]}]});
    value["outbounds"][0]["tls"]["ech"] = json!({"enabled": true, "config_path": "ech.txt"});
    let resources = snapshot_resources(root.path(), &mut value).unwrap();
    let references = [
        (
            "/dns/servers/0/path/0",
            b"127.0.0.1 owned.test\n".as_slice(),
        ),
        (
            "/outbounds/0/tls/ech/config_path",
            b"owned ECH configuration".as_slice(),
        ),
    ];
    let frozen: Vec<_> = references
        .iter()
        .map(|(pointer, bytes)| {
            let id = value
                .pointer(pointer)
                .unwrap()
                .as_str()
                .unwrap()
                .strip_prefix("lanpath-resource:")
                .expect("input must be snapshotted");
            let resource = resources
                .iter()
                .find(|resource| resource.name == id)
                .unwrap();
            assert_eq!(resource.bytes, *bytes);
            (*pointer, id.to_owned())
        })
        .collect();
    std::fs::remove_file(root.path().join("hosts.txt")).unwrap();
    std::fs::remove_file(root.path().join("ech.txt")).unwrap();
    admit(&mut value, &resources, &session, "owned-tun").unwrap();
    for (pointer, id) in frozen {
        assert_eq!(
            value.pointer(pointer).unwrap(),
            &json!(session.join("resources").join(id))
        );
    }
}

#[test]
fn http_header_names_are_data_not_filesystem_or_execution_capabilities() {
    let root = tempfile::tempdir().unwrap();
    let session = root.path().join("session");
    std::fs::create_dir(&session).unwrap();
    let headers = json!({
        "command": "ordinary-header-value",
        "private_key_path": "../this-is-not-a-file",
        "path": "/ordinary-header-value"
    });
    let mut value = json!({
        "inbounds": [
            {"type": "tun", "tag": "tun-in"},
            {"type": "mixed", "listen": "127.0.0.1", "listen_port": 17899}
        ],
        "outbounds": [{"type": "http", "tag": "owned-http", "server": "127.0.0.1",
            "server_port": 18080, "headers": headers}]
    });
    let resources = snapshot_resources(root.path(), &mut value).unwrap();
    admit(&mut value, &resources, &session, "owned-tun").unwrap();
    crate::configuration::check_privileged_document(&session, &value).unwrap();
    assert_eq!(value["outbounds"][0]["headers"], headers);
    assert_eq!(value["inbounds"][0]["interface_name"], "owned-tun");
}

#[cfg(windows)]
#[test]
#[ignore = "requires LANPATH_TEST_SING_BOX pointing to the real managed Windows core"]
fn real_core_checks_snapshotted_hosts_and_ech_after_original_files_are_removed() {
    use std::{fs, process::Command};

    let core = std::path::PathBuf::from(
        std::env::var_os("LANPATH_TEST_SING_BOX").expect("set the actual Windows core executable"),
    );
    let root = tempfile::tempdir().unwrap();
    let generated = Command::new(&core)
        .args(["generate", "ech-keypair", "owned.test"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "ECH generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let generated = String::from_utf8(generated.stdout).unwrap();
    let begin = generated.find("-----BEGIN ECH CONFIGS-----").unwrap();
    let public = &generated[begin..];
    let end = public.find("-----END ECH CONFIGS-----").unwrap() + "-----END ECH CONFIGS-----".len();
    fs::write(root.path().join("ech.pem"), &public[..end]).unwrap();
    fs::write(root.path().join("hosts.txt"), b"127.0.0.1 owned.test\n").unwrap();
    let mut value = json!({
        "inbounds": [
            {"type": "tun", "tag": "tun-in", "address": ["172.19.0.1/30"], "auto_route": false},
            {"type": "mixed", "listen": "127.0.0.1", "listen_port": 17899}
        ],
        "dns": {"servers": [{"type": "hosts", "tag": "owned-hosts", "path": ["hosts.txt"]}], "final": "owned-hosts"},
        "outbounds": [{"type": "http", "tag": "owned-http", "server": "owned.test", "server_port": 443,
            "tls": {"enabled": true, "server_name": "owned.test", "ech": {"enabled": true, "config_path": "ech.pem"}}}],
        "route": {"final": "owned-http", "default_domain_resolver": "owned-hosts"}
    });
    let resources = snapshot_resources(root.path(), &mut value).unwrap();
    fs::remove_file(root.path().join("hosts.txt")).unwrap();
    fs::remove_file(root.path().join("ech.pem")).unwrap();
    let session = root.path().join("session");
    for directory in ["resources", "logs", "cache"] {
        fs::create_dir_all(session.join(directory)).unwrap();
    }
    let session = session.canonicalize().unwrap();
    admit(&mut value, &resources, &session, "owned-resource-check").unwrap();
    for resource in resources {
        fs::write(
            session.join("resources").join(resource.name),
            resource.bytes,
        )
        .unwrap();
    }
    crate::configuration::check_privileged_document(&session, &value).unwrap();
    let configuration = session.join("config.json");
    fs::write(&configuration, serde_json::to_vec(&value).unwrap()).unwrap();
    let checked = Command::new(core)
        .args(["check", "--disable-color", "--config"])
        .arg(configuration)
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "real core rejected frozen resources: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    println!(
        "real managed core: frozen hosts and generated public ECH config accepted after originals were removed"
    );
}
