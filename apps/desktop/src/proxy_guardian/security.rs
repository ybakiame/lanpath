use super::protocol::{UserIdentity, failure};
use crate::windows_access::UserSid;
use lanpath_common::LanpathResult;
use std::{
    ffi::OsStr,
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    ptr,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    System::{JobObjects::IsProcessInJob, Threading::*},
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

fn information<T: Copy>(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> LanpathResult<T> {
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let mut returned = 0;
    let length = size_of::<T>() as u32;
    if unsafe {
        GetTokenInformation(
            token,
            class,
            value.as_mut_ptr().cast(),
            length,
            &mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    if returned != length {
        return Err(failure("unexpected token identity size"));
    }
    Ok(unsafe { value.assume_init() })
}

pub(super) fn user(process: HANDLE) -> LanpathResult<UserIdentity> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    let token = owned(token)?;
    let token = token.as_raw_handle();
    let statistics: TOKEN_STATISTICS = information(token, TokenStatistics)?;
    let elevation: TOKEN_ELEVATION = information(token, TokenElevation)?;
    Ok(UserIdentity {
        sid: UserSid::from_token(token)?.text()?,
        session: information(token, TokenSessionId)?,
        logon: (
            statistics.AuthenticationId.LowPart,
            statistics.AuthenticationId.HighPart,
        ),
        elevated: elevation.TokenIsElevated != 0,
    })
}

pub(super) fn in_job(process: HANDLE) -> LanpathResult<bool> {
    let mut result = 0;
    if unsafe { IsProcessInJob(process, ptr::null_mut(), &mut result) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(result != 0)
}
