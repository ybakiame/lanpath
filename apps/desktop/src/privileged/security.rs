use super::protocol::failure;
use crate::windows_access::{Descriptor, UserSid};
use lanpath_common::{LanpathResult, WindowsProcess};
use std::{
    ffi::OsStr,
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{Com::CoTaskMemFree, Threading::*},
    UI::Shell::*,
};

pub(super) fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
pub(super) fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TokenIdentity {
    pub sid: String,
    pub session: u32,
    pub logon: (u32, i32),
    pub elevation: i32,
    // Only a limited token needs its OS-linked full-token logon identity.
    linked_logon: Option<(u32, i32)>,
}

fn token_information<T: Copy>(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> LanpathResult<T> {
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let expected = size_of::<T>() as u32;
    let mut length = 0;
    if unsafe {
        GetTokenInformation(
            token,
            class,
            value.as_mut_ptr().cast(),
            expected,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    if length != expected {
        return Err(failure("unexpected fixed token information size"));
    }
    // All callers pair a fixed-layout Windows token structure with its class.
    Ok(unsafe { value.assume_init() })
}

fn token_identity_from_token(token: HANDLE) -> LanpathResult<TokenIdentity> {
    let statistics: TOKEN_STATISTICS = token_information(token, TokenStatistics)?;
    let elevation = token_information(token, TokenElevationType)?;
    let linked_logon = if elevation == TokenElevationTypeLimited {
        let linked: TOKEN_LINKED_TOKEN = token_information(token, TokenLinkedToken)?;
        let linked = owned(linked.LinkedToken)?;
        let statistics: TOKEN_STATISTICS =
            token_information(linked.as_raw_handle(), TokenStatistics)?;
        Some((
            statistics.AuthenticationId.LowPart,
            statistics.AuthenticationId.HighPart,
        ))
    } else {
        None
    };
    Ok(TokenIdentity {
        sid: UserSid::from_token(token)?.text()?,
        session: token_information(token, TokenSessionId)?,
        logon: (
            statistics.AuthenticationId.LowPart,
            statistics.AuthenticationId.HighPart,
        ),
        elevation,
        linked_logon,
    })
}

pub(super) fn token_identity(process: HANDLE) -> LanpathResult<TokenIdentity> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    let token = owned(token)?;
    token_identity_from_token(token.as_raw_handle())
}

pub(super) fn same_split_user(parent: &TokenIdentity, helper: &TokenIdentity) -> LanpathResult<()> {
    // Split tokens may have different AuthenticationIds. Bind the helper to
    // the full token Windows actually links to this limited UI token instead.
    if parent.sid != helper.sid
        || parent.session != helper.session
        || parent.elevation != TokenElevationTypeLimited
        || helper.elevation != TokenElevationTypeFull
        || parent.linked_logon != Some(helper.logon)
    {
        return Err(failure(
            "TUN requires the same user's split-token elevation in the original logon session; alternate administrator credentials are not supported",
        ));
    }
    Ok(())
}

pub(super) unsafe fn read_wide(value: *const u16) -> String {
    if value.is_null() {
        return String::new();
    }
    let mut length = 0;
    while length < 32768 && unsafe { *value.add(length) } != 0 {
        length += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(value, length) })
}

fn known_folder(id: &windows_sys::core::GUID) -> LanpathResult<PathBuf> {
    let mut path = ptr::null_mut();
    let status = unsafe { SHGetKnownFolderPath(id, 0, ptr::null_mut(), &mut path) };
    if status < 0 {
        return Err(failure(format!(
            "cannot locate protected Windows folder: {status:#x}"
        )));
    }
    let root = PathBuf::from(unsafe { read_wide(path) });
    unsafe {
        CoTaskMemFree(path.cast());
    }
    Ok(root)
}
pub(super) fn machine_location() -> LanpathResult<PathBuf> {
    Ok(known_folder(&FOLDERID_ProgramData)?
        .join("LanpathGPUI")
        .join("privileged"))
}
pub(super) fn code_location() -> LanpathResult<PathBuf> {
    known_folder(&FOLDERID_ProgramFiles)
}

pub(super) fn machine_root() -> LanpathResult<PathBuf> {
    let privileged = machine_location()?;
    protected_directory(
        privileged
            .parent()
            .ok_or_else(|| failure("missing machine root"))?,
    )?;
    protected_directory(&privileged)?;
    Ok(privileged)
}

pub(super) fn protected_directory(path: &Path) -> LanpathResult<()> {
    let descriptor =
        Descriptor::new("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)")?;
    if unsafe { CreateDirectoryW(wide(path).as_ptr(), &descriptor.attributes()) } == 0
        && io::Error::last_os_error().raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32)
    {
        return Err(io::Error::last_os_error().into());
    }
    validate_protected(path)
}

pub(super) fn private_directory(path: &Path) -> LanpathResult<()> {
    let descriptor = Descriptor::new("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)")?;
    if unsafe { CreateDirectoryW(wide(path).as_ptr(), &descriptor.attributes()) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    validate_protected(path)
}

pub(super) fn ensure_private_directory(path: &Path) -> LanpathResult<()> {
    let descriptor = Descriptor::new("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)")?;
    if unsafe { CreateDirectoryW(wide(path).as_ptr(), &descriptor.attributes()) } == 0
        && io::Error::last_os_error().raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32)
    {
        return Err(io::Error::last_os_error().into());
    }
    validate_directory(path, true, None)
}

/// Reject pre-planted directories, unprotected DACLs and every non-admin/SYSTEM allow ACE.
pub(super) fn validate_protected(path: &Path) -> LanpathResult<()> {
    validate_directory(path, false, None)
}
fn validate_directory(path: &Path, private: bool, installer: Option<PSID>) -> LanpathResult<()> {
    validate_object(path, private, installer, true)
}
fn validate_object(
    path: &Path,
    private: bool,
    installer: Option<PSID>,
    directory: bool,
) -> LanpathResult<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    use std::os::windows::fs::MetadataExt;
    if (if directory {
        !metadata.is_dir()
    } else {
        !metadata.is_file()
    }) || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(failure(
            "protected staging has an unexpected or reparse object",
        ));
    }
    let mut owner = ptr::null_mut();
    let mut acl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let result = unsafe {
        GetNamedSecurityInfoW(
            wide(path).as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32).into());
    }
    let descriptor = unsafe { Descriptor::from_raw(descriptor) };
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(descriptor.as_raw(), &mut control, &mut revision) }
        == 0
        || installer.is_none() && control & SE_DACL_PROTECTED == 0
        || acl.is_null()
        || owner.is_null()
    {
        return Err(failure("staging owner/DACL is missing or not protected"));
    }
    let privileged_sid = |sid| unsafe {
        IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
            || IsWellKnownSid(sid, WinLocalSystemSid) != 0
            || installer.is_some_and(|trusted| EqualSid(sid, trusted) != 0)
    };
    if !privileged_sid(owner) {
        return Err(failure(
            "staging owner is not an authorized Windows administrator/system principal",
        ));
    }
    for index in 0..unsafe { (*acl).AceCount } {
        let mut ace = ptr::null_mut();
        if unsafe { GetAce(acl, index as u32, &mut ace) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        if header.AceType != ACCESS_ALLOWED_ACE_TYPE as u8 {
            return Err(failure("unexpected staging access rule"));
        }
        let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = (&raw const allowed.SidStart).cast_mut().cast();
        let creator_owner = installer.is_some()
            && header.AceFlags & INHERIT_ONLY_ACE as u8 != 0
            && unsafe { IsWellKnownSid(sid, WinCreatorOwnerSid) } != 0;
        const MUTATING: u32 = FILE_WRITE_DATA
            | FILE_APPEND_DATA
            | FILE_WRITE_EA
            | FILE_WRITE_ATTRIBUTES
            | FILE_DELETE_CHILD
            | DELETE
            | WRITE_DAC
            | WRITE_OWNER
            | GENERIC_WRITE
            | GENERIC_ALL;
        if !privileged_sid(sid) && !creator_owner && (private || allowed.Mask & MUTATING != 0) {
            return Err(failure(
                "staging grants disallowed mutation or private access",
            ));
        }
    }
    Ok(())
}

/// Program Files code can inherit its OS-owned read-only ACL. Unlike private
/// session data, it also trusts the native Windows servicing identity.
pub(super) struct CodeDirectoryPolicy {
    installer: [u32; 17],
}
impl CodeDirectoryPolicy {
    pub fn new() -> LanpathResult<Self> {
        let mut installer = [0u32; 17];
        let mut bytes = SECURITY_MAX_SID_SIZE;
        let mut domain = [0u16; 32];
        let mut domain_length = domain.len() as u32;
        let mut kind = 0;
        if unsafe {
            LookupAccountNameW(
                ptr::null(),
                wide(r"NT SERVICE\TrustedInstaller").as_ptr(),
                installer.as_mut_ptr().cast(),
                &mut bytes,
                domain.as_mut_ptr(),
                &mut domain_length,
                &mut kind,
            )
        } == 0
        {
            return Err(io::Error::last_os_error().into());
        }
        if bytes > SECURITY_MAX_SID_SIZE
            || unsafe { IsValidSid(installer.as_mut_ptr().cast()) } == 0
        {
            return Err(failure("Windows servicing SID could not be established"));
        }
        Ok(Self { installer })
    }
    pub fn validate(&self, path: &Path) -> LanpathResult<()> {
        validate_directory(path, false, Some(self.installer.as_ptr().cast_mut().cast()))
    }
    pub fn validate_file(&self, path: &Path) -> LanpathResult<()> {
        validate_object(
            path,
            false,
            Some(self.installer.as_ptr().cast_mut().cast()),
            false,
        )
    }
}

pub(super) fn process_for_pipe(
    pid: u32,
    expected: &lanpath_common::ProcessIdentity,
) -> LanpathResult<WindowsProcess> {
    let process = WindowsProcess::open(pid, false)?;
    if process.identity()? != *expected {
        return Err(failure("named pipe peer process identity mismatch"));
    }
    Ok(process)
}

pub(super) fn windows_directory() -> LanpathResult<PathBuf> {
    let mut buffer = [0u16; 32768];
    let count = unsafe {
        windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW(
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    if count == 0 {
        return Err(io::Error::last_os_error().into());
    }
    if count >= buffer.len() {
        return Err(failure("Windows directory exceeds bound"));
    }
    let value = String::from_utf16(&buffer[..count])
        .map_err(|_| failure("Windows directory contains invalid UTF-16"))?;
    Ok(PathBuf::from(value))
}

#[cfg(test)]
#[path = "security_token_tests.rs"]
mod token_tests;
