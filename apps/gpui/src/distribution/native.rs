use super::invalid;
use crate::data_root::DataMode;
use lanpath_common::LanpathResult;
use std::path::Path;

/// Only web URLs are opened: local files use the separate path API.
pub(crate) fn open_url(value: &str) -> LanpathResult<()> {
    let url = reqwest::Url::parse(value).map_err(|_| invalid("invalid external URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid(
            "external URLs require HTTP(S), a host and no credentials",
        ));
    }
    open_target(std::ffi::OsStr::new(url.as_str()))
}

pub(crate) fn open_path(path: &Path) -> LanpathResult<()> {
    // Absolute paths cannot be interpreted as command-line switches by openers.
    let path = path.canonicalize()?;
    open_target(path.as_os_str())
}

#[cfg(windows)]
fn open_target(target: &std::ffi::OsStr) -> LanpathResult<()> {
    use std::os::windows::ffi::OsStrExt;
    let target: Vec<u16> = target.encode_wide().chain(Some(0)).collect();
    if target[..target.len() - 1].contains(&0) {
        return Err(invalid("opener target contains NUL"));
    }
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    // ShellExecute receives the target as one filename/URL, never command text.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result <= 32 {
        return Err(invalid(format!("Windows opener failed with code {result}")));
    }
    Ok(())
}

#[cfg(not(windows))]
fn open_target(target: &std::ffi::OsStr) -> LanpathResult<()> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open")
        .arg("--")
        .arg(target)
        .status()?;
    #[cfg(not(target_os = "macos"))]
    let status = std::process::Command::new("xdg-open")
        .arg(target)
        .status()?;
    if !status.success() {
        return Err(invalid(format!("native opener failed: {status}")));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationPolicy {
    StandardIdentity,
    /// Portable mode does not opt into a per-process notification identity.
    PortableDisabled,
}

pub(crate) struct NotificationService {
    policy: NotificationPolicy,
}
impl NotificationService {
    pub(crate) fn new(mode: DataMode) -> LanpathResult<Self> {
        let policy = match mode {
            DataMode::Standard => NotificationPolicy::StandardIdentity,
            DataMode::Portable => NotificationPolicy::PortableDisabled,
        };
        #[cfg(windows)]
        if policy == NotificationPolicy::StandardIdentity {
            use windows::{
                Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID, core::HSTRING,
            };
            unsafe {
                SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(crate::product::APP_ID))
            }
            .map_err(|e| invalid(format!("setting GPUI notification identity failed: {e}")))?;
        }
        Ok(Self { policy })
    }
    pub(crate) fn notify(&self, title: &str, body: &str) -> LanpathResult<()> {
        if self.policy == NotificationPolicy::PortableDisabled {
            return Err(invalid(
                "Portable notifications are disabled: no OS identity is registered",
            ));
        }
        if title.is_empty()
            || title.len() > 1024
            || body.len() > 8192
            || title.contains('\0')
            || body.contains('\0')
        {
            return Err(invalid(
                "notification text is empty, oversized or contains NUL",
            ));
        }
        show_notification(title, body)
    }
}

#[cfg(windows)]
struct ToastApartment;
#[cfg(windows)]
impl ToastApartment {
    fn initialize() -> LanpathResult<Self> {
        use windows::Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::WinRT::{RO_INIT_MULTITHREADED, RO_INIT_SINGLETHREADED, RoInitialize},
        };
        let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let initialized = match initialized {
            Err(error) if error.code() == RPC_E_CHANGED_MODE => unsafe {
                RoInitialize(RO_INIT_SINGLETHREADED)
            },
            result => result,
        };
        initialized.map_err(|error| {
            invalid(format!(
                "initializing notification apartment failed: {error}"
            ))
        })?;
        Ok(Self)
    }
}
#[cfg(windows)]
impl Drop for ToastApartment {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::WinRT::RoUninitialize();
        }
    }
}

#[cfg(windows)]
fn show_notification(title: &str, body: &str) -> LanpathResult<()> {
    let _apartment = ToastApartment::initialize()?;
    use windows::{
        Data::Xml::Dom::XmlDocument,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
        core::HSTRING,
    };
    let result = (|| -> windows::core::Result<()> {
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
            xml_text(title), xml_text(body))))?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
            crate::product::APP_ID,
        ))?;
        notifier.Show(&toast)
    })();
    result.map_err(|e| invalid(format!("GPUI Windows notification failed: {e}")))
}

#[cfg(not(windows))]
fn show_notification(title: &str, body: &str) -> LanpathResult<()> {
    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("notify-send")
            .args([
                "--app-name",
                crate::product::DISPLAY_NAME,
                "--",
                title,
                body,
            ])
            .status()?;
        if status.success() {
            return Ok(());
        }
        Err(invalid(format!("native notification failed: {status}")))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (title, body);
        Err(invalid(
            "native notifications are unsupported on this platform",
        ))
    }
}

#[cfg(any(windows, test))]
fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dangerous_url_schemes_are_not_opened() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://user:secret@example.com/",
        ] {
            assert!(open_url(url).is_err());
        }
    }
    #[test]
    fn toast_content_is_data_not_xml() {
        assert_eq!(xml_text("<x>&\"'"), "&lt;x&gt;&amp;&quot;&apos;");
    }
}
