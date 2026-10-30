use super::*;
use crate::{CaStore, FileCaStore};

fn receipt() -> CaInstallReceipt {
    CaInstallReceipt {
        source_path: PathBuf::from("fixture/ca.crt"),
        sha256: sha256_hex(b"exact fixture DER"),
        scope: CaTrustScope::CurrentUserRoot,
        disposition: CaInstallDisposition::Added,
        der: b"exact fixture DER".to_vec(),
        owner_token: Some(b"unique installation token".to_vec()),
    }
}

#[test]
fn owned_removal_requires_exact_der_scope_and_nonempty_matching_token() {
    let own = receipt();
    assert_eq!(
        removal_decision(&own, Some(&own.der), own.owner_token.as_deref()),
        CaRemovalDisposition::Removed
    );
    assert_eq!(
        removal_decision(&own, None, None),
        CaRemovalDisposition::AlreadyAbsent
    );
    assert_eq!(
        removal_decision(
            &own,
            Some(b"different root with same subject"),
            own.owner_token.as_deref()
        ),
        CaRemovalDisposition::OwnershipChanged
    );
    assert_eq!(
        removal_decision(&own, Some(&own.der), Some(b"another installation")),
        CaRemovalDisposition::OwnershipChanged
    );
    assert_eq!(
        removal_decision(&own, Some(&own.der), None),
        CaRemovalDisposition::OwnershipChanged
    );
    let mut missing_token = own.clone();
    missing_token.owner_token = None;
    assert_eq!(
        removal_decision(&missing_token, Some(&own.der), None),
        CaRemovalDisposition::OwnershipChanged
    );
    missing_token.owner_token = Some(Vec::new());
    assert_eq!(
        removal_decision(&missing_token, Some(&own.der), Some(&[])),
        CaRemovalDisposition::OwnershipChanged
    );
    let mut wrong_scope = own.clone();
    wrong_scope.scope = CaTrustScope::System;
    assert_eq!(
        removal_decision(&wrong_scope, Some(&own.der), own.owner_token.as_deref()),
        CaRemovalDisposition::OwnershipChanged
    );
}

#[test]
fn preexisting_and_untracked_receipts_never_authorize_removal_even_with_matching_token() {
    for disposition in [
        CaInstallDisposition::AlreadyPresent,
        CaInstallDisposition::InstalledUntracked,
    ] {
        let mut prior = receipt();
        prior.disposition = disposition;
        assert_eq!(
            removal_decision(&prior, Some(&prior.der), prior.owner_token.as_deref()),
            CaRemovalDisposition::PreservedPreexisting
        );
        assert_eq!(
            removal_decision(&prior, None, None),
            CaRemovalDisposition::PreservedPreexisting
        );
    }
}

#[test]
fn persisted_receipt_preserves_exact_ownership_after_application_restart() {
    let original = receipt();
    let encoded = serde_json::to_vec(&original).unwrap();
    let recovered: CaInstallReceipt = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(recovered.source_path, original.source_path);
    assert_eq!(recovered.sha256, original.sha256);
    assert_eq!(
        removal_decision(
            &recovered,
            Some(&original.der),
            original.owner_token.as_deref()
        ),
        CaRemovalDisposition::Removed
    );
    let mut corrupt = recovered;
    corrupt.sha256 = "not its fingerprint".into();
    // Receipt validation precedes any native store open or mutation on every platform.
    assert!(
        remove_ca(&corrupt)
            .unwrap_err()
            .to_string()
            .contains("fingerprint mismatch")
    );
}

#[test]
fn changed_authorization_source_is_rejected_before_any_trust_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileCaStore::new(dir.path());
    let original = store.load_or_generate().unwrap();
    let original_fingerprint = sha256_hex(&pem_to_der(&original.cert_pem).unwrap());
    let other_dir = tempfile::tempdir().unwrap();
    let other = FileCaStore::new(other_dir.path())
        .load_or_generate()
        .unwrap();
    std::fs::write(store.cert_path(), &other.cert_pem).unwrap();
    let error = install_ca_with_expected(&store.cert_path(), &original_fingerprint)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("source changed since authorization"),
        "{error}"
    );
    assert_eq!(
        std::fs::read_to_string(store.cert_path()).unwrap(),
        other.cert_pem
    );
    assert!(std::fs::read_to_string(store.key_path()).unwrap() == original.key_pem);
}

#[test]
fn malformed_missing_or_multiple_source_certificates_are_unknown_not_absent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ca.crt");
    assert!(query_ca(&path).is_err());
    assert_eq!(ca_trust_status(&path).status, CaTrustStatus::Unknown);
    for bytes in [
        "broken certificate\n",
        "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n",
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(query_ca(&path).is_err());
        assert_eq!(ca_trust_status(&path).status, CaTrustStatus::Unknown);
        assert!(install_ca_with_expected(&path, "irrelevant").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
    }
    let material = FileCaStore::new(dir.path().join("valid"))
        .load_or_generate()
        .unwrap();
    let chain = format!("{}{}", material.cert_pem, material.cert_pem);
    std::fs::write(&path, &chain).unwrap();
    assert!(query_ca(&path).is_err());
    assert_eq!(ca_trust_status(&path).status, CaTrustStatus::Unknown);
    assert_eq!(std::fs::read_to_string(path).unwrap(), chain);
}
