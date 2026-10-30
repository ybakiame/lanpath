use super::*;
use crate::{CaStore, FileCaStore};

fn fixture(dir: &Path) -> (OwnedCaTrustStore, CaInstallReceipt) {
    let material = FileCaStore::new(dir).load_or_generate().unwrap();
    let der = pem_to_der(&material.cert_pem).unwrap();
    let receipt = CaInstallReceipt {
        source_path: dir.join("ca.crt"),
        sha256: sha256_hex(&der),
        scope: CaTrustScope::CurrentUserRoot,
        disposition: CaInstallDisposition::Added,
        der,
        owner_token: Some(b"fixture-original-installation-token".to_vec()),
    };
    (OwnedCaTrustStore::new(dir), receipt)
}

#[test]
fn persisted_receipt_survives_restart_and_cannot_be_overwritten_by_a_later_install() {
    let root = tempfile::tempdir().unwrap();
    let (store, receipt) = fixture(root.path());
    store.save(&receipt).unwrap();
    let original = std::fs::read(store.receipt_path()).unwrap();
    drop(store);
    let restarted = OwnedCaTrustStore::new(root.path());
    let recovered = restarted.load().unwrap().unwrap();
    assert_eq!(recovered.sha256, receipt.sha256);
    assert_eq!(recovered.der, receipt.der);
    assert_eq!(recovered.owner_token, receipt.owner_token);
    let mut other = recovered;
    other.owner_token = Some(b"later installer token".to_vec());
    assert!(restarted.save(&other).is_err());
    assert_eq!(std::fs::read(restarted.receipt_path()).unwrap(), original);
}

#[test]
fn mismatched_install_and_stale_removal_authorization_preserve_the_original_receipt() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let (store, receipt) = fixture(root.path());
    store.save(&receipt).unwrap();
    let original = std::fs::read(store.receipt_path()).unwrap();
    let replacement = FileCaStore::new(other.path()).load_or_generate().unwrap();
    std::fs::write(store.source(), &replacement.cert_pem).unwrap();
    let replacement_hash = sha256_hex(&pem_to_der(&replacement.cert_pem).unwrap());
    // These consumer guards run before any platform trust mutation, on every OS.
    let error = store.install(&replacement_hash).unwrap_err().to_string();
    assert!(error.contains("different CA"), "{error}");
    let error = store.remove(&replacement_hash).unwrap_err().to_string();
    assert!(
        error.contains("changed since removal authorization"),
        "{error}"
    );
    assert_eq!(std::fs::read(store.receipt_path()).unwrap(), original);
    assert_eq!(
        std::fs::read_to_string(store.source()).unwrap(),
        replacement.cert_pem
    );
}

#[test]
fn corrupt_or_nonowned_receipts_fail_before_native_mutation_and_are_preserved() {
    let root = tempfile::tempdir().unwrap();
    let (store, receipt) = fixture(root.path());
    for receipt in [
        CaInstallReceipt {
            sha256: "corrupt fingerprint".into(),
            ..receipt.clone()
        },
        CaInstallReceipt {
            disposition: CaInstallDisposition::AlreadyPresent,
            ..receipt.clone()
        },
        CaInstallReceipt {
            owner_token: None,
            ..receipt.clone()
        },
        CaInstallReceipt {
            scope: CaTrustScope::System,
            ..receipt.clone()
        },
    ] {
        let bytes = serde_json::to_vec(&receipt).unwrap();
        std::fs::write(store.receipt_path(), &bytes).unwrap();
        assert!(store.remove(&receipt.sha256).is_err());
        assert!(store.install(&receipt.sha256).is_err());
        assert_eq!(std::fs::read(store.receipt_path()).unwrap(), bytes);
    }
}

#[test]
fn missing_signing_source_keeps_saved_removal_evidence_without_minting_a_ca() {
    let root = tempfile::tempdir().unwrap();
    let (store, receipt) = fixture(root.path());
    store.save(&receipt).unwrap();
    let bytes = std::fs::read(store.receipt_path()).unwrap();
    std::fs::remove_file(store.source()).unwrap();
    let status = store.query().unwrap();
    assert_eq!(status.status, CaTrustStatus::Unknown);
    assert!(status.sha256.is_none());
    assert_eq!(
        status.receipt_sha256.as_deref(),
        Some(receipt.sha256.as_str())
    );
    assert!(!store.source().exists());
    assert_eq!(std::fs::read(store.receipt_path()).unwrap(), bytes);
}
