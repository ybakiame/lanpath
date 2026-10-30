use lanpath_common::{LanpathError, LanpathResult};
use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
use windows_sys::Win32::{Security::*, System::Threading::*, UI::WindowsAndMessaging::*};

pub(crate) fn ensure_unelevated() -> LanpathResult<()> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
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
        return Err(LanpathError::Config("GPUI 与脚本必须以普通用户权限运行。请关闭此窗口，从资源管理器正常启动 Lanpath GPUI，不要使用“以管理员身份运行”。TUN 会单独请求窄权限辅助进程。".into()));
    }
    Ok(())
}

pub(crate) fn show_startup_error(message: &str) {
    let title: Vec<u16> = crate::product::DISPLAY_NAME
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
