use lanpath_common::error::LanpathResult;
use std::path::Path;

#[cfg(test)]
#[path = "protection_tests.rs"]
mod tests;

#[cfg(any(windows, test))]
fn require_persistent_acls(flags: u32) -> LanpathResult<()> {
    const FILE_PERSISTENT_ACLS: u32 = 0x8;
    if flags & FILE_PERSISTENT_ACLS == 0 {
        return Err(super::error(
            "CA private keys require a filesystem with persistent Windows ACLs (e.g. NTFS); move portable data explicitly, never regenerate the identity",
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn protect(path: &Path, directory: bool) -> LanpathResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )
    .map_err(|e| super::io_error("restrict permissions", e))
}
#[cfg(not(any(unix, windows)))]
pub(crate) fn protect(_path: &Path, _directory: bool) -> LanpathResult<()> {
    Err(super::error("This platform cannot protect CA private keys"))
}
#[cfg(windows)]
pub(crate) fn protect(path: &Path, directory: bool) -> LanpathResult<()> {
    use std::{
        os::windows::ffi::OsStrExt,
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::{Authorization::*, *},
        Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let path =
        std::path::absolute(path).map_err(|e| super::io_error("resolve protected path", e))?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if wide[..wide.len() - 1].contains(&0) {
        return Err(super::error("NUL in CA path"));
    }
    // No plaintext-key fallback on FAT/exFAT or other portable volumes without persistent ACLs.
    let mut volume = vec![0u16; 32768];
    let mut flags = 0;
    unsafe {
        if GetVolumePathNameW(wide.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) == 0
            || GetVolumeInformationW(
                volume.as_ptr(),
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                &mut flags,
                null_mut(),
                0,
            ) == 0
        {
            return Err(super::io_error(
                "query private-key filesystem",
                std::io::Error::last_os_error(),
            ));
        }
    }
    require_persistent_acls(flags)?;
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(super::io_error(
            "open account token",
            std::io::Error::last_os_error(),
        ));
    }
    // usize storage guarantees TOKEN_USER/SID alignment.
    let mut needed = 0;
    unsafe {
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
    }
    if needed < std::mem::size_of::<TOKEN_USER>() as u32 {
        let error = std::io::Error::last_os_error();
        unsafe {
            CloseHandle(token);
        }
        return Err(super::io_error("size account token", error));
    }
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
    let token_error = std::io::Error::last_os_error();
    unsafe {
        CloseHandle(token);
    }
    if ok == 0 {
        return Err(super::io_error("read account token", token_error));
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut sid_text = null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid_text) } == 0 {
        return Err(super::io_error(
            "format account SID",
            std::io::Error::last_os_error(),
        ));
    }
    let mut length = 0;
    unsafe {
        while *sid_text.add(length) != 0 {
            length += 1;
        }
    }
    let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(sid_text, length) });
    unsafe {
        LocalFree(sid_text.cast());
    }
    let inherit = if directory { "OICI" } else { "" };
    let sddl: Vec<u16> = format!("D:P(A;{inherit};FA;;;{sid})(A;{inherit};FA;;;SY)")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut descriptor = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(super::io_error(
            "build private-key ACL",
            std::io::Error::last_os_error(),
        ));
    }
    let mut acl = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    let result = unsafe {
        if GetSecurityDescriptorDacl(descriptor, &mut present, &mut acl, &mut defaulted) == 0
            || present == 0
            || acl.is_null()
        {
            Err(super::io_error(
                "read private-key ACL",
                std::io::Error::last_os_error(),
            ))
        } else {
            let status = SetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null(),
            );
            if status == 0 {
                Ok(())
            } else {
                Err(super::io_error(
                    "apply private-key ACL (access required; identity preserved)",
                    std::io::Error::from_raw_os_error(status as i32),
                ))
            }
        }
    };
    unsafe {
        LocalFree(descriptor);
    }
    result
}
