use super::*;

fn pair() -> (TokenIdentity, TokenIdentity) {
    let sid = "S-1-5-21-100-200-300-1001".to_owned();
    (
        TokenIdentity {
            sid: sid.clone(),
            session: 7,
            logon: (11, 0),
            elevation: TokenElevationTypeLimited,
            linked_logon: Some((17, 0)),
        },
        TokenIdentity {
            sid,
            session: 7,
            logon: (17, 0),
            elevation: TokenElevationTypeFull,
            linked_logon: None,
        },
    )
}

#[test]
fn only_the_linked_full_elevation_in_the_original_user_session_is_accepted() {
    let (parent, helper) = pair();
    same_split_user(&parent, &helper).unwrap();
    for boundary in 0..7 {
        let (mut parent, mut helper) = pair();
        match boundary {
            0 => helper.sid = "S-1-5-21-100-200-300-1002".into(),
            1 => helper.session += 1,
            2 => helper.logon = (19, 0),
            3 => helper.logon = parent.logon,
            4 => parent.elevation = TokenElevationTypeFull,
            5 => helper.elevation = TokenElevationTypeDefault,
            6 => parent.linked_logon = None,
            _ => unreachable!(),
        }
        assert!(
            same_split_user(&parent, &helper).is_err(),
            "boundary {boundary}"
        );
    }
}

#[test]
#[ignore = "requires an ordinary Windows split-token session; read-only, no elevation prompt"]
fn actual_windows_linked_pair_is_accepted_without_elevating_the_test_process() {
    let mut token = ptr::null_mut();
    assert_ne!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) },
        0
    );
    let token = owned(token).unwrap();
    let parent = token_identity_from_token(token.as_raw_handle()).unwrap();
    assert_eq!(parent.elevation, TokenElevationTypeLimited);
    let linked: TOKEN_LINKED_TOKEN =
        token_information(token.as_raw_handle(), TokenLinkedToken).unwrap();
    let linked = owned(linked.LinkedToken).unwrap();
    let helper = token_identity_from_token(linked.as_raw_handle()).unwrap();
    same_split_user(&parent, &helper).unwrap();
    let unchanged = token_identity(unsafe { GetCurrentProcess() }).unwrap();
    assert_eq!(unchanged.elevation, TokenElevationTypeLimited);
    println!(
        "OS-linked elevation accepted; distinct authentication IDs: {}; test process remains limited",
        parent.logon != helper.logon
    );
}
