use super::*;

/// Explicit reproduction fixture: an owned stopped TUN session whose device is
/// nonpresent but whose logical interface row remains. No process or device is
/// stopped, removed, adopted, or reconfigured by this test.
#[test]
#[ignore = "requires LANPATH_TEST_TUN_ROOT and an owned stopped phantom TUN session"]
fn stopped_owned_phantom_has_no_active_network_effects() {
    let root = std::path::PathBuf::from(std::env::var_os("LANPATH_TEST_TUN_ROOT").unwrap());
    let root_id = uuid::Uuid::parse_str(
        std::fs::read_to_string(root.join(".gpui-root-id"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    let path = security::machine_location()
        .unwrap()
        .join("journals")
        .join("tun.json");
    let record: super::super::journal::Record =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let owner = security::token_identity(unsafe {
        windows_sys::Win32::System::Threading::GetCurrentProcess()
    })
    .unwrap();
    assert_eq!(record.owner_sid, owner.sid);
    assert_eq!(record.root_id, root_id);
    assert!(
        record.forced && !record.ready,
        "fixture must be a stopped failed startup"
    );
    let original = record.core.as_ref().unwrap();
    match lanpath_common::WindowsProcess::open(original.pid, false) {
        Ok(process) => {
            let actual = process.identity().unwrap();
            assert!(actual.created != original.created || process.exited().unwrap());
        }
        Err(error) => assert_eq!(
            error.raw_os_error(),
            Some(windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER as i32)
        ),
    }
    let receipt = record.adapter.as_ref().unwrap();
    assert_eq!(receipt.guid, record.prepared.adapter_guid);
    let remaining = remaining(&record.prepared, Some(receipt), &record.observed).unwrap();
    assert!(
        remaining.is_empty(),
        "stopped fixture still reports network effects"
    );
}
