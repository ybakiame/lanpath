use super::*;

/// The native service validates the application before checking policy write
/// access. An ordinary token must reach E_ACCESSDENIED, not E_INVALIDARG, and
/// therefore cannot install this diagnostic rule.
#[test]
#[ignore = "requires an ordinary Windows session with the firewall service; no elevation"]
fn canonical_image_reaches_policy_authorization_without_installing_a_rule() {
    let token = super::super::security::token_identity(unsafe {
        windows_sys::Win32::System::Threading::GetCurrentProcess()
    })
    .unwrap();
    assert_eq!(
        token.elevation,
        windows_sys::Win32::Security::TokenElevationTypeLimited
    );
    let firewall = Firewall::open().unwrap();
    let image = std::env::current_exe().unwrap().canonicalize().unwrap();
    let plan = firewall.prepare(uuid::Uuid::new_v4(), &image).unwrap();
    let error = unsafe { firewall.rules.Add(&plan.rule) }.unwrap_err();
    assert_eq!(error.code(), windows::Win32::Foundation::E_ACCESSDENIED);
}
