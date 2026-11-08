use super::super::model::{DIRECT, PROXY, RecoveryIntent, Snapshot};

/// Only a committed, previously direct session can be relinquished this way.
/// Inactive strings/blobs are deliberately preserved, never used as permission
/// to rewrite the user's settings. PAC/policy/type validation is still mandatory.
pub(super) fn verified_direct(intent: &RecoveryIntent, current: &Snapshot) -> bool {
    let Some(observed) = &intent.observed else {
        return false;
    };
    let typed_enable = |snapshot: &Snapshot, enabled: bool| {
        snapshot
            .values
            .first()
            .and_then(Option::as_ref)
            .is_some_and(|value| value.kind == 4 && value.bytes == u32::from(enabled).to_le_bytes())
    };
    intent.original.effective.flags == DIRECT
        && intent.original.effective.pac_url.is_empty()
        && intent.target.flags == (DIRECT | PROXY)
        && intent.target.pac_url.is_empty()
        && observed.effective == intent.target
        && observed.guards == intent.original.guards
        && typed_enable(observed, true)
        && current.effective.flags == DIRECT
        && current.effective.pac_url.is_empty()
        && current.guards == intent.original.guards
        && typed_enable(current, false)
}
