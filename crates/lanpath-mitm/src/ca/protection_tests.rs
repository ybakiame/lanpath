use super::*;

#[test]
fn portable_volumes_without_persistent_acls_fail_closed() {
    for flags in [0, 1, 0x0004_0000] {
        let error = require_persistent_acls(flags).unwrap_err().to_string();
        assert!(error.contains("persistent Windows ACLs"), "{error}");
        assert!(error.contains("never regenerate"), "{error}");
    }
    require_persistent_acls(0x8).unwrap();
    require_persistent_acls(0x8 | 0x0004_0000).unwrap();
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::{CaStore, FileCaStore};
    use std::{os::windows::ffi::OsStrExt, ptr::null_mut};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::{Authorization::*, *},
        Storage::FileSystem::FILE_ALL_ACCESS,
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe fn sid_text(sid: PSID) -> String {
        let mut text = null_mut();
        assert_ne!(unsafe { ConvertSidToStringSidW(sid, &mut text) }, 0);
        let mut length = 0;
        unsafe {
            while *text.add(length) != 0 {
                length += 1;
            }
        }
        let result =
            String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) }).unwrap();
        unsafe {
            LocalFree(text.cast());
        }
        result
    }
    fn current_account() -> String {
        let mut token = null_mut();
        assert_ne!(
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) },
            0
        );
        let mut needed = 0;
        unsafe {
            GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
        }
        assert!(needed as usize >= std::mem::size_of::<TOKEN_USER>());
        let mut buffer = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * std::mem::size_of::<usize>()) as u32,
                &mut needed,
            )
        };
        unsafe {
            CloseHandle(token);
        }
        assert_ne!(ok, 0);
        unsafe { sid_text((*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid) }
    }
    fn assert_private_acl(path: &Path, directory: bool) {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor = null_mut();
        let mut acl = null_mut();
        assert_eq!(
            unsafe {
                GetNamedSecurityInfoW(
                    wide.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    &mut acl,
                    null_mut(),
                    &mut descriptor,
                )
            },
            0
        );
        assert!(!acl.is_null());
        let result = std::panic::catch_unwind(|| {
            let mut control = 0;
            let mut revision = 0;
            assert_ne!(
                unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) },
                0
            );
            assert_ne!(
                control & SE_DACL_PROTECTED,
                0,
                "inherited broad access must be disabled"
            );
            assert_eq!(unsafe { (*acl).AceCount }, 2);
            let mut trustees = Vec::new();
            for index in 0..2 {
                let mut pointer = null_mut();
                assert_ne!(unsafe { GetAce(acl, index, &mut pointer) }, 0);
                let ace = unsafe { &*pointer.cast::<ACCESS_ALLOWED_ACE>() };
                assert_eq!(
                    ace.Header.AceType, 0,
                    "only ACCESS_ALLOWED_ACE entries are expected"
                );
                assert_eq!(ace.Mask, FILE_ALL_ACCESS);
                let expected_flags = if directory {
                    (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8
                } else {
                    0
                };
                assert_eq!(ace.Header.AceFlags, expected_flags);
                trustees
                    .push(unsafe { sid_text(std::ptr::addr_of!(ace.SidStart).cast_mut().cast()) });
            }
            trustees.sort();
            let mut expected = vec![current_account(), "S-1-5-18".to_owned()];
            expected.sort();
            assert_eq!(
                trustees, expected,
                "no Everyone/Users/Administrators grant may remain"
            );
        });
        unsafe {
            LocalFree(descriptor);
        }
        result.unwrap();
    }
    #[test]
    fn portable_ca_directory_and_key_have_only_account_and_system_access() {
        let root = tempfile::tempdir().unwrap();
        let ca_dir = root.path().join("portable").join("data").join("certs");
        let store = FileCaStore::new(&ca_dir);
        let original = store.load_or_generate().unwrap();
        assert_private_acl(&ca_dir, true);
        assert_private_acl(&store.key_path(), false);
        assert_private_acl(&ca_dir.join(".ca.lock"), false);
        assert_eq!(store.load_or_generate().unwrap(), original);
        assert_private_acl(&store.key_path(), false);
        assert!(std::fs::read_to_string(store.key_path()).unwrap() == original.key_pem);
    }
}
