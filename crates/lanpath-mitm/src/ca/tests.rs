use super::*;
use std::sync::{Arc, Barrier};

fn fixture(subject: &str, change: impl FnOnce(&mut rcgen::CertificateParams)) -> CaMaterial {
    let mut params = ca_params().unwrap();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, subject);
    change(&mut params);
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    CaMaterial {
        cert_pem: cert.pem(),
        key_pem: key.serialize_pem(),
    }
}
fn write_pair(dir: &Path, material: &CaMaterial) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("ca.crt"), &material.cert_pem).unwrap();
    fs::write(dir.join("ca.key"), &material.key_pem).unwrap();
}
fn assert_pair(dir: &Path, material: &CaMaterial) {
    assert_eq!(
        fs::read(dir.join("ca.crt")).unwrap(),
        material.cert_pem.as_bytes()
    );
    assert!(fs::read(dir.join("ca.key")).unwrap() == material.key_pem.as_bytes());
}
fn assert_rejected_unchanged(material: &CaMaterial) {
    let dir = tempfile::tempdir().unwrap();
    write_pair(dir.path(), material);
    let store = FileCaStore::new(dir.path());
    assert!(store.load_or_generate().is_err());
    assert_pair(dir.path(), material);
    assert!(!dir.path().join(".ca-staging").exists());
    assert!(!dir.path().join(".ca-transaction").exists());
}

#[test]
fn generation_is_lanpath_named_and_reloading_retains_exact_identity() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileCaStore::new(dir.path());
    let first = store.load_or_generate().unwrap();
    validate_ca_material(&first).unwrap();
    let der = crate::pem_to_der(&first.cert_pem).unwrap();
    let (_, cert) = x509_parser::parse_x509_certificate(&der).unwrap();
    assert_eq!(
        cert.subject()
            .iter_common_name()
            .next()
            .unwrap()
            .as_str()
            .unwrap(),
        "Lanpath MITM CA"
    );
    assert_eq!(store.load_or_generate().unwrap(), first);
    assert_pair(dir.path(), &first);
    assert!(!format!("{first:?}").contains("BEGIN PRIVATE KEY"));
}

#[test]
fn valid_legacy_subject_and_pem_whitespace_are_not_reissued() {
    let dir = tempfile::tempdir().unwrap();
    let mut old = fixture("ProxyPanel MITM CA", |_| {});
    old.cert_pem = format!("\n{}\n", old.cert_pem.replace('\n', "\r\n"));
    old.key_pem = old.key_pem.replace('\n', "\r\n");
    write_pair(dir.path(), &old);
    assert_eq!(
        FileCaStore::new(dir.path()).load_or_generate().unwrap(),
        old
    );
    assert_pair(dir.path(), &old);
}

#[test]
fn either_missing_half_preserves_surviving_bytes_without_generation() {
    let original = fixture("Legacy CA", |_| {});
    for (name, bytes) in [
        ("ca.crt", original.cert_pem.as_bytes()),
        ("ca.key", original.key_pem.as_bytes()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(name), bytes).unwrap();
        let error = FileCaStore::new(dir.path())
            .load_or_generate()
            .unwrap_err()
            .to_string();
        assert!(error.contains("incomplete"), "{error}");
        assert!(fs::read(dir.path().join(name)).unwrap() == bytes);
        assert!(
            !dir.path()
                .join(if name == "ca.crt" { "ca.key" } else { "ca.crt" })
                .exists()
        );
        assert!(!dir.path().join(".ca-staging").exists());
    }
}

#[test]
fn malformed_mismatched_and_multiple_objects_are_never_replaced() {
    let good = fixture("Original CA", |_| {});
    let other = fixture("Other CA", |_| {});
    let mut malformed = good.clone();
    malformed.cert_pem = "not a certificate\n".into();
    assert_rejected_unchanged(&malformed);
    let mut invalid_key = good.clone();
    invalid_key.key_pem = "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n".into();
    assert_rejected_unchanged(&invalid_key);
    let mut mismatch = good.clone();
    mismatch.key_pem = other.key_pem;
    assert_rejected_unchanged(&mismatch);
    let mut chain = good.clone();
    chain.cert_pem.push_str(&good.cert_pem);
    assert_rejected_unchanged(&chain);
    let mut keys = good.clone();
    keys.key_pem.push_str(&good.key_pem);
    assert_rejected_unchanged(&keys);
    let mut mixed = good.clone();
    mixed.cert_pem.push_str(&good.key_pem);
    assert_rejected_unchanged(&mixed);
    let mut prefixed = good;
    prefixed.cert_pem.insert_str(0, "unexpected data\n");
    assert_rejected_unchanged(&prefixed);
}

#[test]
fn expired_future_non_ca_and_non_signing_certificates_preserve_both_files() {
    let now = time::OffsetDateTime::now_utc();
    assert_rejected_unchanged(&fixture("Expired", |params| {
        params.not_before = now - time::Duration::days(3);
        params.not_after = now - time::Duration::days(1);
    }));
    assert_rejected_unchanged(&fixture("Future", |params| {
        params.not_before = now + time::Duration::days(1);
        params.not_after = now + time::Duration::days(3);
    }));
    assert_rejected_unchanged(&fixture("Not CA", |params| {
        params.is_ca = rcgen::IsCa::NoCa
    }));
    assert_rejected_unchanged(&fixture("Cannot sign", |params| {
        params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature]
    }));
}

#[test]
fn self_issued_certificate_signed_by_another_key_is_rejected() {
    let params = ca_params().unwrap();
    let identity_key = rcgen::KeyPair::generate().unwrap();
    let wrong_signer = rcgen::KeyPair::generate().unwrap();
    let issuer = rcgen::Issuer::from_params(&params, &wrong_signer);
    let cert = params.signed_by(&identity_key, &issuer).unwrap();
    assert_rejected_unchanged(&CaMaterial {
        cert_pem: cert.pem(),
        key_pem: identity_key.serialize_pem(),
    });
}

#[test]
fn concurrent_creators_observe_one_complete_matching_identity() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FileCaStore::new(dir.path()));
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.load_or_generate().unwrap()
            })
        })
        .collect();
    let results: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    for result in &results {
        assert_eq!(result, &results[0]);
    }
    assert_pair(dir.path(), &results[0]);
    assert!(!dir.path().join(".ca-staging").exists());
    assert!(!dir.path().join(".ca-transaction").exists());
}

#[test]
fn complete_staging_or_journal_recovers_exact_pair_including_partial_publication() {
    let original = fixture("Recover original", |_| {});
    for name in [".ca-staging", ".ca-transaction"] {
        for published in [None, Some("ca.crt"), Some("ca.key")] {
            let dir = tempfile::tempdir().unwrap();
            write_pair(&dir.path().join(name), &original);
            if let Some(file) = published {
                let content = if file == "ca.crt" {
                    &original.cert_pem
                } else {
                    &original.key_pem
                };
                fs::write(dir.path().join(file), content).unwrap();
            }
            let store = FileCaStore::new(dir.path());
            assert_eq!(store.load_or_generate().unwrap(), original);
            assert_eq!(store.load_or_generate().unwrap(), original);
            assert_pair(dir.path(), &original);
            assert!(!dir.path().join(name).exists());
        }
    }
}

#[test]
fn conflicting_journal_checks_both_destinations_before_publishing_anything() {
    let original = fixture("Original", |_| {});
    let staged = fixture("Staged", |_| {});
    for existing in ["ca.crt", "ca.key"] {
        let dir = tempfile::tempdir().unwrap();
        let transaction = dir.path().join(".ca-transaction");
        write_pair(&transaction, &staged);
        let content = if existing == "ca.crt" {
            &original.cert_pem
        } else {
            &original.key_pem
        };
        fs::write(dir.path().join(existing), content).unwrap();
        assert!(FileCaStore::new(dir.path()).load_or_generate().is_err());
        assert!(fs::read(dir.path().join(existing)).unwrap() == content.as_bytes());
        assert!(
            !dir.path()
                .join(if existing == "ca.crt" {
                    "ca.key"
                } else {
                    "ca.crt"
                })
                .exists()
        );
        assert_pair(&transaction, &staged);
    }
}

#[test]
fn incomplete_or_multiple_transactions_require_recovery_and_preserve_every_byte() {
    let original = fixture("Original", |_| {});
    for name in [".ca-staging", ".ca-transaction"] {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join(name);
        fs::create_dir(&journal).unwrap();
        fs::write(journal.join("ca.crt"), &original.cert_pem).unwrap();
        assert!(FileCaStore::new(dir.path()).load_or_generate().is_err());
        assert_eq!(
            fs::read(journal.join("ca.crt")).unwrap(),
            original.cert_pem.as_bytes()
        );
        assert!(!journal.join("ca.key").exists());
        assert!(!dir.path().join("ca.crt").exists());
        assert!(!dir.path().join("ca.key").exists());
    }
    let dir = tempfile::tempdir().unwrap();
    write_pair(&dir.path().join(".ca-staging"), &original);
    write_pair(&dir.path().join(".ca-transaction"), &original);
    assert!(FileCaStore::new(dir.path()).load_or_generate().is_err());
    assert_pair(&dir.path().join(".ca-staging"), &original);
    assert_pair(&dir.path().join(".ca-transaction"), &original);
    assert!(!dir.path().join("ca.crt").exists());
}

#[test]
fn storage_obstructions_are_not_deleted_or_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("certs");
    fs::write(&blocked, b"caller-owned obstruction").unwrap();
    assert!(FileCaStore::new(&blocked).load_or_generate().is_err());
    assert_eq!(fs::read(blocked).unwrap(), b"caller-owned obstruction");
}

#[cfg(unix)]
#[test]
fn key_and_directory_permissions_are_private_even_for_rejected_identity() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let material = fixture("Expired", |params| {
        params.not_before = time::OffsetDateTime::now_utc() - time::Duration::days(3);
        params.not_after = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    });
    write_pair(dir.path(), &material);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
    fs::set_permissions(dir.path().join("ca.key"), fs::Permissions::from_mode(0o666)).unwrap();
    assert!(FileCaStore::new(dir.path()).load_or_generate().is_err());
    assert_eq!(
        fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(dir.path().join("ca.key"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_pair(dir.path(), &material);
}

#[cfg(unix)]
#[test]
fn symbolic_link_paths_are_rejected_without_touching_targets() {
    use std::os::unix::fs::symlink;
    let target = tempfile::tempdir().unwrap();
    let original = fixture("Unrelated", |_| {});
    write_pair(target.path(), &original);
    for name in [
        "ca.crt",
        "ca.key",
        ".ca.lock",
        ".ca-staging",
        ".ca-transaction",
    ] {
        let dir = tempfile::tempdir().unwrap();
        symlink(
            target
                .path()
                .join(if name == "ca.key" { "ca.key" } else { "ca.crt" }),
            dir.path().join(name),
        )
        .unwrap();
        assert!(FileCaStore::new(dir.path()).load_or_generate().is_err());
        assert_pair(target.path(), &original);
    }
    let parent = tempfile::tempdir().unwrap();
    let link = parent.path().join("certs");
    symlink(target.path(), &link).unwrap();
    assert!(FileCaStore::new(link).load_or_generate().is_err());
    assert_pair(target.path(), &original);
}
