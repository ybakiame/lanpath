use super::super::model::{EffectiveProxy, error};
use lanpath_common::LanpathResult;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::Networking::WinInet::*;

fn option(id: u32) -> INTERNET_PER_CONN_OPTIONW {
    INTERNET_PER_CONN_OPTIONW {
        dwOption: id,
        Value: INTERNET_PER_CONN_OPTIONW_0 { dwValue: 0 },
    }
}

fn list(options: &mut [INTERNET_PER_CONN_OPTIONW]) -> INTERNET_PER_CONN_OPTION_LISTW {
    INTERNET_PER_CONN_OPTION_LISTW {
        dwSize: size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
        pszConnection: null_mut(), // documented default LAN, never a RAS connection
        dwOptionCount: options.len() as u32,
        dwOptionError: 0,
        pOptions: options.as_mut_ptr(),
    }
}

pub(super) fn query() -> LanpathResult<EffectiveProxy> {
    let mut options = [
        option(INTERNET_PER_CONN_FLAGS_UI),
        option(INTERNET_PER_CONN_PROXY_SERVER),
        option(INTERNET_PER_CONN_PROXY_BYPASS),
        option(INTERNET_PER_CONN_AUTOCONFIG_URL),
    ];
    let mut settings = list(&mut options);
    let mut length = size_of_val(&settings) as u32;
    let ok = unsafe {
        InternetQueryOptionW(
            null(),
            INTERNET_OPTION_PER_CONNECTION_OPTION,
            (&mut settings as *mut INTERNET_PER_CONN_OPTION_LISTW).cast(),
            &mut length,
        )
    };
    if ok == 0 {
        return Err(error(format!(
            "WinINet default-LAN query: {}",
            std::io::Error::last_os_error()
        )));
    }
    // Query allocates the strings with GlobalAlloc. Free all, including on invalid UTF-16.
    let strings = options[1..]
        .iter()
        .map(|opt| unsafe {
            let ptr = opt.Value.pszValue;
            let result = super::protection::read_wide(ptr);
            if !ptr.is_null() {
                GlobalFree(ptr.cast());
            }
            result
        })
        .collect::<Vec<_>>();
    let mut strings = strings.into_iter();
    Ok(EffectiveProxy {
        flags: unsafe { options[0].Value.dwValue },
        server: strings
            .next()
            .ok_or_else(|| error("missing WinINet server"))??,
        bypass: strings
            .next()
            .ok_or_else(|| error("missing WinINet bypass"))??,
        pac_url: strings
            .next()
            .ok_or_else(|| error("missing WinINet PAC"))??,
    })
}

pub(super) fn apply(effective: &EffectiveProxy) -> LanpathResult<()> {
    let mut server = super::protection::wide(&effective.server);
    let mut bypass = super::protection::wide(&effective.bypass);
    let mut options = [
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_FLAGS,
            Value: INTERNET_PER_CONN_OPTIONW_0 {
                dwValue: effective.flags,
            },
        },
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_PROXY_SERVER,
            Value: INTERNET_PER_CONN_OPTIONW_0 {
                pszValue: server.as_mut_ptr(),
            },
        },
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_PROXY_BYPASS,
            Value: INTERNET_PER_CONN_OPTIONW_0 {
                pszValue: bypass.as_mut_ptr(),
            },
        },
    ];
    let settings = list(&mut options);
    if unsafe {
        InternetSetOptionW(
            null(),
            INTERNET_OPTION_PER_CONNECTION_OPTION,
            (&settings as *const INTERNET_PER_CONN_OPTION_LISTW).cast(),
            size_of_val(&settings) as u32,
        )
    } == 0
    {
        return Err(error(format!(
            "WinINet default-LAN apply: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

pub(super) fn notify() -> LanpathResult<()> {
    for option in [INTERNET_OPTION_SETTINGS_CHANGED, INTERNET_OPTION_REFRESH] {
        if unsafe { InternetSetOptionW(null(), option, null(), 0) } == 0 {
            return Err(error(format!(
                "WinINet notification {option}: {}",
                std::io::Error::last_os_error()
            )));
        }
    }
    let setting =
        super::protection::wide("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings");
    let mut result = 0;
    if unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SendMessageTimeoutW(
            windows_sys::Win32::UI::WindowsAndMessaging::HWND_BROADCAST,
            windows_sys::Win32::UI::WindowsAndMessaging::WM_SETTINGCHANGE,
            0,
            setting.as_ptr() as isize,
            windows_sys::Win32::UI::WindowsAndMessaging::SMTO_ABORTIFHUNG,
            2000,
            &mut result,
        )
    } == 0
    {
        return Err(error(format!(
            "settings broadcast failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}
