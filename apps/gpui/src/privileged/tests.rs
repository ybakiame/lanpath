use super::{config, protocol::*};
use serde_json::json;

fn document() -> serde_json::Value {
    json!({"inbounds":[
        {"type":"mixed","tag":"mixed-in","listen":"127.0.0.1","listen_port":17899},
        {"type":"tun","tag":"tun-in","auto_route":false}],
        "outbounds":[{"type":"direct","tag":"direct"}]})
}

fn checked_prepare(
    value: &mut serde_json::Value,
    root: &std::path::Path,
) -> lanpath_common::LanpathResult<()> {
    config::admit(value, &[], root, "owned-test")?;
    crate::configuration::check_privileged_document(root, value)
}

#[test]
fn helper_rejects_global_clock_and_proxy_mutations_including_go_aliases() {
    let dir = tempfile::tempdir().unwrap();
    // A complete harmless request must pass, so a missing required inbound
    // cannot accidentally make the negative capability cases pass.
    checked_prepare(&mut document(), dir.path()).unwrap();
    for name in ["write_to_system", "WRITE_TO_SYSTEM"] {
        let mut value = document();
        value["ntp"] = json!({"enabled":true,"server":"127.0.0.1","write_to_system":false});
        value["ntp"][name] = true.into();
        assert!(
            checked_prepare(&mut value, dir.path()).is_err(),
            "clock capability admitted: {name}"
        );
    }
    for name in ["set_system_proxy", "SET_SYSTEM_PROXY"] {
        let mut value = document();
        value["inbounds"][0][name] = true.into();
        assert!(
            checked_prepare(&mut value, dir.path()).is_err(),
            "proxy capability admitted: {name}"
        );
    }
}

#[test]
fn helper_rejects_case_aliased_privileged_files_and_unknown_api_listeners() {
    let dir = tempfile::tempdir().unwrap();
    checked_prepare(&mut document(), dir.path()).unwrap();
    let mut value = document();
    value["outbounds"][0]["certificate_PATH"] = "C:/unmanaged/ca.crt".into();
    assert!(checked_prepare(&mut value, dir.path()).is_err());
    let mut value = document();
    value["experimental"] = json!({"v2ray_api":{"listen":"0.0.0.0:19091"}});
    assert!(checked_prepare(&mut value, dir.path()).is_err());
}

#[test]
fn framed_requests_reject_replays_versions_and_unbounded_lengths() {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, 7, Request::Status).unwrap();
    assert!(matches!(
        read_frame::<Request>(&mut bytes.as_slice(), 7).unwrap(),
        Request::Status
    ));
    assert!(read_frame::<Request>(&mut bytes.as_slice(), 6).is_err());
    for length in [0, MAX_FRAME as u32 + 1] {
        assert!(read_frame::<Request>(&mut length.to_le_bytes().as_slice(), 0).is_err());
    }
    let payload =
        serde_json::to_vec(&json!({"version":VERSION+1,"sequence":7,"body":{"command":"Status"}}))
            .unwrap();
    let mut future = (payload.len() as u32).to_le_bytes().to_vec();
    future.extend_from_slice(&payload);
    assert!(read_frame::<Request>(&mut future.as_slice(), 7).is_err());
}

#[test]
fn stop_kind_does_not_claim_graceful_exit_for_a_previously_exited_core() {
    assert_eq!(
        StopKind::completed(false, false, true, false),
        StopKind::AlreadyStopped
    );
    assert_eq!(
        StopKind::completed(false, false, true, true),
        StopKind::Graceful
    );
    assert_eq!(
        StopKind::completed(false, false, false, true),
        StopKind::AlreadyStopped
    );
}

#[test]
fn stop_kind_keeps_unresolved_recovery_and_forced_stop_precedence() {
    assert_eq!(
        StopKind::completed(true, true, true, true),
        StopKind::RecoveryRequired
    );
    assert_eq!(
        StopKind::completed(true, false, true, true),
        StopKind::RecoveryRequired
    );
    assert_eq!(
        StopKind::completed(false, true, true, false),
        StopKind::Forced
    );
}
