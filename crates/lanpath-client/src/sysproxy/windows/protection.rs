use super::super::model::error;
use lanpath_common::LanpathResult;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
use windows_sys::Win32::Security::Authorization::*;
use windows_sys::Win32::Security::Cryptography::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath};

pub(super) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub(super) fn path_wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub(super) unsafe fn read_wide(ptr: *const u16) -> LanpathResult<String> {
    if ptr.is_null() {
        return Ok(String::new());
    }
    let mut length = 0;
    while length < 32768 && unsafe { *ptr.add(length) } != 0 {
        length += 1;
    }
    if length == 32768 {
        return Err(error("native string exceeds supported length"));
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(ptr, length) })
        .map_err(|_| error("invalid UTF-16 in native settings"))
}

struct LocalMemory(*mut core::ffi::c_void);
impl Drop for LocalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

pub(super) fn current_sid() -> LanpathResult<String> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(error(format!(
            "current-user token: {}",
            std::io::Error::last_os_error()
        )));
    }
    let mut length = 0;
    unsafe {
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut length);
    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    };
    unsafe {
        CloseHandle(token);
    }
    if ok == 0 {
        return Err(error("current-user SID query failed"));
    }
    let sid = unsafe { (*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    sid_string(sid)
}

fn sid_string(sid: PSID) -> LanpathResult<String> {
    let mut text = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(error("SID conversion failed"));
    }
    let _memory = LocalMemory(text.cast());
    unsafe { read_wide(text) }
}

pub(super) fn directory(sid: &str) -> LanpathResult<PathBuf> {
    let mut native = null_mut();
    let result =
        unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, null_mut(), &mut native) };
    if result < 0 {
        return Err(error(format!("LocalAppData lookup failed ({result:#x})")));
    }
    let text = unsafe { read_wide(native) };
    unsafe {
        CoTaskMemFree(native.cast());
    }
    let root = PathBuf::from(std::ffi::OsString::from_wide(
        &text?.encode_utf16().collect::<Vec<_>>(),
    ))
    .join("LanpathSystemProxyOwnership");
    match std::fs::create_dir(&root) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    protect(&root, sid)?;
    Ok(root)
}

/// Reject reparse points and foreign owners before setting a protected same-user DACL.
/// DPAPI additionally authenticates/encrypts journal contents for the current user.
pub(super) fn protect(path: &Path, sid: &str) -> LanpathResult<()> {
    use std::os::windows::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_attributes() & 0x400 != 0 {
        return Err(error("reparse point in ownership store"));
    }
    let path = path_wide(path);
    let mut owner = null_mut();
    let mut descriptor = null_mut();
    let code = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    let _memory = LocalMemory(descriptor);
    if code != 0 {
        if matches!(code, 32 | 33) {
            return Err(std::io::Error::from_raw_os_error(code as i32).into());
        }
        return Err(error(format!("ownership store ACL query failed ({code})")));
    }
    if sid_string(owner)? != sid {
        return Err(error("ownership store is owned by another user"));
    }
    let sddl = wide(&format!("D:P(A;OICI;FA;;;{sid})"));
    let mut private = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut private,
            null_mut(),
        )
    } == 0
    {
        return Err(error("private journal ACL construction failed"));
    }
    let _private = LocalMemory(private);
    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl = null_mut();
    if unsafe { GetSecurityDescriptorDacl(private, &mut present, &mut dacl, &mut defaulted) } == 0
        || present == 0
    {
        return Err(error("private journal ACL extraction failed"));
    }
    let code = unsafe {
        SetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            dacl,
            null_mut(),
        )
    };
    if code != 0 {
        if matches!(code, 32 | 33) {
            return Err(std::io::Error::from_raw_os_error(code as i32).into());
        }
        return Err(error(format!(
            "private journal ACL application failed ({code})"
        )));
    }
    Ok(())
}

pub(super) fn crypt(bytes: &[u8], encrypt: bool) -> LanpathResult<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes
            .len()
            .try_into()
            .map_err(|_| error("journal too large"))?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                wide("Lanpath current-user system proxy recovery v1").as_ptr(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(error(format!(
            "current-user DPAPI journal protection failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    let _memory = LocalMemory(output.pbData.cast());
    Ok(unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec())
}
