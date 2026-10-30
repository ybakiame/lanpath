use super::{protocol::failure, security};
use lanpath_common::LanpathResult;
use std::{
    io,
    os::windows::io::{AsRawHandle, OwnedHandle},
};
use windows_sys::Win32::{
    Foundation::{WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
};

/// Session-independent serialization of this user's immutable code cache. An
/// abandoned owner is safe: the next owner validates every final/partial file.
pub(super) struct StagingLock(OwnedHandle);
impl StagingLock {
    pub fn acquire(name: &str, sid: &str) -> LanpathResult<Self> {
        let descriptor = crate::windows_access::Descriptor::new(&format!(
            "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{sid})"
        ))?;
        let attributes = descriptor.attributes();
        let name = security::wide(format!(r"Global\{name}"));
        let handle = security::owned(unsafe { CreateMutexW(&attributes, 0, name.as_ptr()) })?;
        match unsafe { WaitForSingleObject(handle.as_raw_handle(), 300_000) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(handle)),
            WAIT_TIMEOUT => Err(failure(
                "another Lanpath code-staging operation did not finish within five minutes",
            )),
            _ => Err(io::Error::last_os_error().into()),
        }
    }
}
impl Drop for StagingLock {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0.as_raw_handle());
        }
    }
}
