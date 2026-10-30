use super::*;
use crate::{CaStore, FileCaStore};

/// Explicit parent authorization only; creates two temporary exact CurrentUser roots and cleans
/// only receipts proving those additions. No subject-based deletion and no ambient test mutation.
#[test]
#[ignore = "explicit Windows CurrentUser Root temporary-certificate authorization required"]
fn protected_receipt_current_user_root_restart_roundtrip_preserves_preexisting() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    FileCaStore::new(root.path()).load_or_generate().unwrap();
    FileCaStore::new(other.path()).load_or_generate().unwrap();
    let store = OwnedCaTrustStore::new(root.path());
    let preexisting_store = OwnedCaTrustStore::new(other.path());
    let initial = store.query().unwrap();
    assert_eq!(initial.status, CaTrustStatus::NotTrusted);
    let expected = initial.sha256.unwrap();
    let other_source = other.path().join("ca.crt");
    let other_query = query_ca(&other_source).unwrap();
    assert_eq!(other_query.view.status, CaTrustStatus::NotTrusted);
    // This root predates the application-owned store's install attempt. Keep its separate receipt
    // solely for test cleanup, never give it to that store or let the application claim it.
    let preexisting = install_ca_with_expected(&other_source, &other_query.sha256).unwrap();
    let result = std::panic::catch_unwind(|| {
        assert_eq!(preexisting.disposition, CaInstallDisposition::Added);
        let installed = store.install(&expected).unwrap();
        assert_eq!(installed.status, CaTrustStatus::Trusted);
        assert_eq!(installed.ownership, CaOwnershipStatus::Owned);
        let bytes = std::fs::read(store.receipt_path()).unwrap();
        let restarted = OwnedCaTrustStore::new(root.path());
        let status = restarted.query().unwrap();
        assert_eq!(status.status, CaTrustStatus::Trusted);
        assert_eq!(status.ownership, CaOwnershipStatus::Owned);
        assert_eq!(status.receipt_sha256.as_deref(), Some(expected.as_str()));
        assert_eq!(
            restarted.install(&expected).unwrap().ownership,
            CaOwnershipStatus::Owned
        );
        assert_eq!(std::fs::read(store.receipt_path()).unwrap(), bytes);
        assert!(restarted.remove("stale confirmation").is_err());
        assert_eq!(
            restarted.query().unwrap().ownership,
            CaOwnershipStatus::Owned
        );
        let existing = preexisting_store.install(&other_query.sha256).unwrap();
        assert_eq!(existing.status, CaTrustStatus::Trusted);
        assert_eq!(existing.ownership, CaOwnershipStatus::Untracked);
        assert!(!preexisting_store.has_receipt().unwrap());
        assert!(preexisting_store.remove(&other_query.sha256).is_err());
        assert_eq!(
            query_ca(&other_source).unwrap().view.status,
            CaTrustStatus::Trusted
        );
    });
    // Always attempt both precise cleanups before resuming a captured assertion failure.
    let own_cleanup = store.has_receipt().and_then(|exists| {
        if exists {
            store.remove(&expected).map(Some)
        } else {
            Ok(None)
        }
    });
    let existing_cleanup = remove_ca(&preexisting);
    if let Some(cleanup) = own_cleanup.unwrap() {
        assert_eq!(cleanup.disposition, CaRemovalDisposition::Removed);
    }
    assert_eq!(
        existing_cleanup.unwrap().disposition,
        CaRemovalDisposition::Removed
    );
    assert_eq!(store.query().unwrap().status, CaTrustStatus::NotTrusted);
    assert_eq!(
        query_ca(&other_source).unwrap().view.status,
        CaTrustStatus::NotTrusted
    );
    assert!(!store.has_receipt().unwrap());
    result.unwrap();
}
