//! Windows 数据根私有 ACL：从 apps/gpui 迁移，约束不变。
//! 缺失的目录在创建时即私有化，早于锁、身份或业务数据写入；
//! 已存在的根必须已经排除其他非特权账户。

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

/// 持有 LocalAlloc 安全描述符，包括 GetSecurityInfo 返回的描述符。
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
    /// SAFETY: 指针必须是已转移给此 owner 的活动 LocalAlloc 描述符。
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

/// 在其 SID 被 ACL 比较借用期间保留原生 TOKEN_USER 存储。
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

/// 普通权限准入（从 apps/gpui 的 `windows_gui.rs` 移植并改 NSH 文案）：
/// 应用与系统代理守护都必须以普通用户权限运行；提权会改变 ACL/注册表
/// 写入身份，恢复语义不再属于当前用户会话。
pub(crate) fn ensure_unelevated() -> lanpath_common::LanpathResult<()> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    let mut raw = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return Err(lanpath_common::LanpathError::Config(
            "invalid current process token".into(),
        ));
    }
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut returned = 0;
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenElevation,
            (&raw mut elevation).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    if returned != size_of::<TOKEN_ELEVATION>() as u32 || elevation.TokenIsElevated != 0 {
        return Err(lanpath_common::LanpathError::Config(
            "Lanpath 与系统代理守护必须以普通用户权限运行。请关闭此窗口，从资源管理器正常启动 Lanpath，不要使用“以管理员身份运行”。".into(),
        ));
    }
    Ok(())
}

pub(crate) fn private_root(path: &Path, initialize: bool) -> io::Result<DirectoryLocks> {
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
            "Lanpath 数据目录需要具备持久私有 ACL 的文件系统；请使用 NTFS 目录",
        ));
    }
    let created_root = !missing.is_empty();
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
    // 预先创建的空目录（如便携模式解压出的 `data/`）可能继承了磁盘根目录的
    // 宽泛权限；初始化空目录前主动收紧为当前用户私有 DACL。
    if initialize && !created_root {
        apply_private_dacl(&root, &descriptor)?;
    }
    validate_user_root(&root, &user, true)?;
    Ok(directories)
}

fn apply_private_dacl(file: &File, descriptor: &Descriptor) -> io::Result<()> {
    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl = ptr::null_mut();
    if unsafe {
        GetSecurityDescriptorDacl(descriptor.as_raw(), &mut present, &mut dacl, &mut defaulted)
    } == 0
        || present == 0
        || dacl.is_null()
    {
        return Err(io::Error::last_os_error());
    }
    let status = unsafe {
        SetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(())
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
            "Windows 数据根必须属于当前用户并带有显式私有 DACL",
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
                "Windows 数据根上存在不支持的访问规则",
            ));
        }
        let allowed = unsafe { &*pointer.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = (&raw const allowed.SidStart).cast_mut().cast();
        let count = unsafe { *((sid as *const u8).add(1)) } as usize;
        if count > 15
            || 8 + count * 4 > usize::from(header.AceSize) - 8
            || unsafe { IsValidSid(sid) } == 0
        {
            return Err(io::Error::other("Windows 数据 ACL 中的 SID 无效"));
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
                "Windows 数据根授予了其他账户访问权限；请使用当前用户私有目录",
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
            return Err(io::Error::other("Windows 私有数据 DACL 在读回后未被保护"));
        }
        // 只冻结已经校验过的私有 ACL；绝不修复公共或外属目录，
        // 也绝不扩大或替换其访问项。
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
