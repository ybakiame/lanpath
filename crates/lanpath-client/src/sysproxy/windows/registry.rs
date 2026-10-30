use super::super::model::*;
use super::protection::wide;
use lanpath_common::LanpathResult;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::*;

const SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
const CONNECTIONS: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections";
const POLICY: &str = r"Software\Policies\Microsoft\Windows\CurrentVersion\Internet Settings";
const PANEL_POLICY: &str = r"Software\Policies\Microsoft\Internet Explorer\Control Panel";
const TOUCHED: [(&str, &str); 5] = [
    (SETTINGS, "ProxyEnable"),
    (SETTINGS, "ProxyServer"),
    (SETTINGS, "ProxyOverride"),
    (CONNECTIONS, "DefaultConnectionSettings"),
    (CONNECTIONS, "SavedLegacySettings"),
];
const GUARDED: [(&str, &str); 12] = [
    (SETTINGS, "AutoConfigURL"),
    (SETTINGS, "AutoDetect"),
    (POLICY, "ProxySettingsPerUser"),
    (POLICY, "ProxyEnable"),
    (POLICY, "ProxyServer"),
    (POLICY, "ProxyOverride"),
    (POLICY, "AutoConfigURL"),
    (POLICY, "AutoDetect"),
    (PANEL_POLICY, "Proxy"),
    (PANEL_POLICY, "Autoconfig"),
    (POLICY, "EnableAutoProxyResultCache"),
    (POLICY, "DisableCachingOfSSLPages"),
];

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

fn open(root: HKEY, path: &str, access: u32) -> LanpathResult<Option<Key>> {
    let mut key = null_mut();
    let result = unsafe { RegOpenKeyExW(root, wide(path).as_ptr(), 0, access, &mut key) };
    match result {
        ERROR_SUCCESS => Ok(Some(Key(key))),
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
        code => Err(error(format!("registry scope access failed ({code})"))),
    }
}

fn read(root: HKEY, path: &str, name: &str) -> LanpathResult<Option<TypedValue>> {
    let Some(key) = open(root, path, KEY_QUERY_VALUE | KEY_WOW64_64KEY)? else {
        return Ok(None);
    };
    let name = wide(name);
    let mut kind = 0;
    let mut length = 0;
    let code = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            null_mut(),
            &mut length,
        )
    };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if code != ERROR_SUCCESS || length > 1024 * 1024 {
        return Err(error(format!(
            "registry value query failed or too large ({code})"
        )));
    }
    let mut bytes = vec![0; length as usize];
    let code = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            bytes.as_mut_ptr(),
            &mut length,
        )
    };
    if code != ERROR_SUCCESS {
        return Err(error(format!("registry changed during snapshot ({code})")));
    }
    bytes.truncate(length as usize);
    Ok(Some(TypedValue { kind, bytes }))
}

pub(super) fn values() -> LanpathResult<Vec<Option<TypedValue>>> {
    TOUCHED
        .iter()
        .map(|(path, name)| read(HKEY_CURRENT_USER, path, name))
        .collect()
}

pub(super) fn guards() -> LanpathResult<Vec<Option<TypedValue>>> {
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .flat_map(|root| {
            GUARDED
                .iter()
                .map(move |(path, name)| read(root, path, name))
        })
        .collect()
}

pub(super) fn validate(snapshot: &Snapshot, policy: AcquisitionPolicy) -> LanpathResult<()> {
    if snapshot.values.len() != TOUCHED.len() || snapshot.guards.len() != GUARDED.len() * 2 {
        return Err(error("invalid registry snapshot shape"));
    }
    let effective = &snapshot.effective;
    if effective.flags == 0
        || effective.flags & !(DIRECT | PROXY) != 0
        || !effective.pac_url.is_empty()
    {
        return Err(error(
            "PAC, autodiscovery, or unsupported effective default-LAN scope; no mutation performed",
        ));
    }
    if policy == AcquisitionPolicy::RequireDirect && effective.flags & PROXY != 0 {
        return Err(error(
            "foreign active manual proxy; explicit takeover consent is required",
        ));
    }
    for (index, value) in snapshot.guards.iter().enumerate() {
        let slot = index % GUARDED.len();
        // AutoConfigURL and AutoDetect are unsupported even when currently inactive.
        // A configured policy is rejected rather than guessing user/machine precedence.
        if value.is_some() && slot < 10 {
            return Err(error(
                "PAC/autodiscovery or proxy policy registry values are configured; unsupported baseline",
            ));
        }
    }
    for (index, value) in snapshot.values.iter().enumerate() {
        if let Some(value) = value {
            let valid = match index {
                0 => {
                    value.kind == REG_DWORD
                        && value.bytes.len() == 4
                        && matches!(
                            u32::from_le_bytes([
                                value.bytes[0],
                                value.bytes[1],
                                value.bytes[2],
                                value.bytes[3]
                            ]),
                            0 | 1
                        )
                }
                1 | 2 => value.kind == REG_SZ && valid_string(&value.bytes),
                _ => value.kind == REG_BINARY && value.bytes.len() >= 12,
            };
            if !valid {
                return Err(error(
                    "unsupported typed proxy registry baseline; no mutation performed",
                ));
            }
        }
    }
    if let Some(value) = &snapshot.values[0] {
        let enabled = value.bytes[0] == 1;
        if enabled != (effective.flags & PROXY != 0) {
            return Err(error(
                "effective and typed proxy enable state disagree; unsupported baseline",
            ));
        }
    } else if effective.flags & PROXY != 0 {
        return Err(error(
            "enabled effective proxy has no supported per-user typed baseline",
        ));
    }
    Ok(())
}

fn valid_string(bytes: &[u8]) -> bool {
    if bytes.len() < 2 || bytes.len() % 2 != 0 || !bytes.ends_with(&[0, 0]) {
        return false;
    }
    let words = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect::<Vec<_>>();
    !words[..words.len() - 1].contains(&0) && String::from_utf16(&words[..words.len() - 1]).is_ok()
}

pub(super) fn restore(snapshot: &Snapshot) -> LanpathResult<()> {
    if snapshot.values.len() != TOUCHED.len() {
        return Err(error("invalid restoration snapshot"));
    }
    for ((path, name), value) in TOUCHED.iter().zip(&snapshot.values) {
        let key = open(HKEY_CURRENT_USER, path, KEY_SET_VALUE | KEY_WOW64_64KEY)?;
        let Some(key) = key else {
            if value.is_none() {
                continue;
            }
            return Err(error(
                "original registry scope disappeared during restoration",
            ));
        };
        let name = wide(name);
        let code = unsafe {
            match value {
                Some(value) => RegSetValueExW(
                    key.0,
                    name.as_ptr(),
                    0,
                    value.kind,
                    value.bytes.as_ptr(),
                    value.bytes.len() as u32,
                ),
                None => RegDeleteValueW(key.0, name.as_ptr()),
            }
        };
        if code != ERROR_SUCCESS && !(value.is_none() && code == ERROR_FILE_NOT_FOUND) {
            return Err(error(format!(
                "typed restoration failed ({code}); recovery journal retained"
            )));
        }
        let code = unsafe { RegFlushKey(key.0) };
        if code != ERROR_SUCCESS {
            return Err(error(format!("registry restoration flush failed ({code})")));
        }
    }
    Ok(())
}
