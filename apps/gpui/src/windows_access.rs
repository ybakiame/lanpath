use crate::windows_file::DirectoryLocks;
use std::{
    fs::{File, OpenOptions},
    io,
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{
        SystemServices::ACCESS_ALLOWED_ACE_TYPE,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

/// Owns a LocalAlloc security descriptor, including descriptors returned by GetSecurityInfo.
pub(crate) struct Descriptor(PSECURITY_DESCRIPTOR);
impl Descriptor {
    pub(crate) fn new(sddl: &str) -> io::Result<Self> {
        let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
    /// SAFETY: pointer must be a live LocalAlloc descriptor transferred to this owner.
    pub(crate) unsafe fn from_raw(pointer: PSECURITY_DESCRIPTOR) -> Self {
        Self(pointer)
    }
    pub(crate) fn as_raw(&self) -> PSECURITY_DESCRIPTOR {
        self.0
    }
    pub(crate) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}
impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

/// Retains the native TOKEN_USER storage while its SID is borrowed by ACL comparisons.
pub(crate) struct UserSid {
    storage: Vec<usize>,
}
impl UserSid {
    pub(crate) fn current() -> io::Result<Self> {
        let mut raw = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            return Err(io::Error::other("invalid current process token"));
        }
        let token = unsafe { OwnedHandle::from_raw_handle(raw) };
        Self::from_token(token.as_raw_handle())
    }
    pub(crate) fn from_token(token: HANDLE) -> io::Result<Self> {
        let mut length = 0;
        unsafe {
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut length);
        }
        if length < size_of::<TOKEN_USER>() as u32 || length > 65536 {
            return Err(io::Error::other("invalid native token user size"));
        }
        let mut storage = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        let capacity = storage.len() * size_of::<usize>();
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                storage.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if length < size_of::<TOKEN_USER>() as u32 || length as usize > capacity {
            return Err(io::Error::other(
                "native token user exceeded its valid allocation bounds",
            ));
        }
        let user = Self { storage };
        let start = user.storage.as_ptr() as usize;
        let sid = user.as_raw() as usize;
        if sid < start || sid > start + length as usize - 8 {
            return Err(io::Error::other("native user SID escaped its token buffer"));
        }
        let count = unsafe { *((sid as *const u8).add(1)) } as usize;
        if count > 15
            || 8 + count * 4 > start + length as usize - sid
            || unsafe { IsValidSid(user.as_raw()) } == 0
        {
            return Err(io::Error::other("invalid native user SID"));
        }
        Ok(user)
    }
    pub(crate) fn as_raw(&self) -> PSID {
        unsafe { (*self.storage.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }
    pub(crate) fn text(&self) -> io::Result<String> {
        let mut pointer = ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(self.as_raw(), &mut pointer) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut length = 0;
        while length < 256 && unsafe { *pointer.add(length) } != 0 {
            length += 1;
        }
        let result = if length == 256 {
            Err(io::Error::other("native SID text exceeds bound"))
        } else {
            String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, length) })
                .map_err(|_| io::Error::other("native SID text contains invalid UTF-16"))
        };
        unsafe {
            LocalFree(pointer.cast());
        }
        result
    }
}

/// Missing directories are private at creation, before locks, identities or secrets
/// are written. Existing roots must already exclude other unprivileged accounts.
pub(crate) fn private_root(path: &Path) -> io::Result<DirectoryLocks> {
    let user = UserSid::current()?;
    let sid = user.text()?;
    let descriptor = Descriptor::new(&format!(
        "O:{sid}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{sid})"
    ))?;
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(existing);
                existing = existing
                    .parent()
                    .ok_or_else(|| io::Error::other("private data path has no existing root"))?;
            }
            Err(error) => return Err(error),
        }
    }
    let mut directories = DirectoryLocks::acquire(existing)?;
    if !directories.supports_private_acls(existing)? {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "GPUI data requires a filesystem with persistent private ACLs; use a current-user private directory and explicit import for migration",
        ));
    }
    for directory in missing.into_iter().rev() {
        let wide: Vec<u16> = directory.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { CreateDirectoryW(wide.as_ptr(), &descriptor.attributes()) } == 0
            && io::Error::last_os_error().raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32)
        {
            return Err(io::Error::last_os_error());
        }
        directories.extend(directory)?;
    }
    let root = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | READ_CONTROL | WRITE_DAC)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    validate_user_root(&root, &user, true)?;
    Ok(directories)
}

fn validate_user_root(file: &File, user: &UserSid, freeze_inheritance: bool) -> io::Result<()> {
    let mut owner = ptr::null_mut();
    let mut acl = ptr::null_mut();
    let mut raw = ptr::null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut raw,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let descriptor = unsafe { Descriptor::from_raw(raw) };
    if owner.is_null() || acl.is_null() || unsafe { EqualSid(owner, user.as_raw()) } == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Windows data root must be owned by its current user and have an explicit private DACL",
        ));
    }
    for index in 0..unsafe { (*acl).AceCount } {
        let mut pointer = ptr::null_mut();
        if unsafe { GetAce(acl, u32::from(index), &mut pointer) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let header = unsafe { &*pointer.cast::<ACE_HEADER>() };
        if header.AceType != ACCESS_ALLOWED_ACE_TYPE as u8 || usize::from(header.AceSize) < 16 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unsupported access rule on the Windows data root",
            ));
        }
        let allowed = unsafe { &*pointer.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = (&raw const allowed.SidStart).cast_mut().cast();
        let count = unsafe { *((sid as *const u8).add(1)) } as usize;
        if count > 15
            || 8 + count * 4 > usize::from(header.AceSize) - 8
            || unsafe { IsValidSid(sid) } == 0
        {
            return Err(io::Error::other("invalid SID in Windows data ACL"));
        }
        let trusted = unsafe {
            EqualSid(sid, user.as_raw()) != 0
                || IsWellKnownSid(sid, WinLocalSystemSid) != 0
                || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
        };
        let creator_owner = header.AceFlags & INHERIT_ONLY_ACE as u8 != 0
            && unsafe { IsWellKnownSid(sid, WinCreatorOwnerSid) } != 0;
        if !trusted && !creator_owner {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Windows data root grants access to another account; use a private GPUI directory",
            ));
        }
    }
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(descriptor.as_raw(), &mut control, &mut revision) }
        == 0
    {
        return Err(io::Error::last_os_error());
    }
    if control & SE_DACL_PROTECTED == 0 {
        if !freeze_inheritance {
            return Err(io::Error::other(
                "Windows private data DACL was not protected after readback",
            ));
        }
        // Freeze only the already validated private ACL; never repair a public or
        // foreign-owned directory, and never widen or replace its access entries.
        let status = unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                acl,
                ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        drop(descriptor);
        return validate_user_root(file, user, false);
    }
    Ok(())
}
