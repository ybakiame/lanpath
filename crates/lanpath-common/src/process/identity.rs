//! Recovery identities are OS creation identities, never timestamps supplied by callers.
use std::{io, path::PathBuf};

#[cfg(target_os = "macos")]
mod macos;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub created: u64,
    pub exe: PathBuf,
}

impl ProcessIdentity {
    pub fn capture(pid: u32) -> io::Result<Self> {
        #[cfg(windows)]
        {
            WindowsProcess::open(pid, false)?.identity()
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
            let tail = stat
                .rsplit_once(") ")
                .ok_or_else(|| io::Error::other("invalid process stat"))?
                .1;
            let created = tail
                .split_whitespace()
                .nth(19)
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| io::Error::other("missing process creation identity"))?;
            let exe = std::fs::read_link(format!("/proc/{pid}/exe"))?;
            Ok(Self { pid, created, exe })
        }
        #[cfg(target_os = "macos")]
        {
            macos::capture(pid)
        }
        #[cfg(not(any(
            windows,
            target_os = "linux",
            target_os = "android",
            target_os = "macos"
        )))]
        {
            let _ = pid;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "process creation identity unavailable",
            ))
        }
    }

    /// Validate and terminate using the SAME kernel object. A reused PID is never authority.
    pub fn terminate_verified(&self) -> io::Result<()> {
        #[cfg(windows)]
        {
            let process = WindowsProcess::open(self.pid, true)?;
            if process.identity()? != *self {
                return Err(io::Error::other("process identity mismatch"));
            }
            process.terminate_and_wait(10_000)
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
            let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, self.pid, 0) };
            if raw < 0 {
                return Err(io::Error::last_os_error());
            }
            let fd = unsafe { OwnedFd::from_raw_fd(raw as i32) };
            if Self::capture(self.pid)? != *self {
                return Err(io::Error::other("process identity mismatch"));
            }
            let result = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                )
            };
            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut poll = libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            if unsafe { libc::poll(&mut poll, 1, 10_000) } <= 0 {
                return Err(io::Error::other("process exit unresolved"));
            }
            Ok(())
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "handle-bound recovery unavailable",
            ))
        }
    }
}

#[cfg(windows)]
pub struct WindowsProcess(std::os::windows::io::OwnedHandle);
#[cfg(windows)]
impl WindowsProcess {
    /// Duplicate the already-owned kernel handle, never reopening a potentially reused PID.
    pub fn duplicate(handle: std::os::windows::io::BorrowedHandle<'_>) -> io::Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        use windows_sys::Win32::{
            Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle},
            System::Threading::GetCurrentProcess,
        };
        let mut duplicate = std::ptr::null_mut();
        let current = unsafe { GetCurrentProcess() };
        if unsafe {
            DuplicateHandle(
                current,
                handle.as_raw_handle(),
                current,
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(unsafe {
            std::os::windows::io::OwnedHandle::from_raw_handle(duplicate)
        }))
    }
    pub fn open(pid: u32, terminate: bool) -> io::Result<Self> {
        use std::os::windows::io::FromRawHandle;
        use windows_sys::Win32::System::Threading::*;
        let rights = PROCESS_QUERY_LIMITED_INFORMATION
            | 0x0010_0000
            | if terminate { PROCESS_TERMINATE } else { 0 };
        let h = unsafe { OpenProcess(rights, 0, pid) };
        if h.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(unsafe {
            std::os::windows::io::OwnedHandle::from_raw_handle(h)
        }))
    }
    pub fn handle(&self) -> windows_sys::Win32::Foundation::HANDLE {
        use std::os::windows::io::AsRawHandle;
        self.0.as_raw_handle()
    }
    pub fn identity(&self) -> io::Result<ProcessIdentity> {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::{Foundation::FILETIME, System::Threading::*};
        let mut times: [FILETIME; 4] = unsafe { std::mem::zeroed() };
        let p = times.as_mut_ptr();
        if unsafe { GetProcessTimes(self.handle(), p, p.add(1), p.add(2), p.add(3)) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut path = [0u16; 32_768];
        let mut size = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(self.handle(), 0, path.as_mut_ptr(), &mut size) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(ProcessIdentity {
            pid: unsafe { GetProcessId(self.handle()) },
            created: (u64::from(times[0].dwHighDateTime) << 32) | u64::from(times[0].dwLowDateTime),
            exe: PathBuf::from(std::ffi::OsString::from_wide(&path[..size as usize])),
        })
    }
    pub fn exited(&self) -> io::Result<bool> {
        use windows_sys::Win32::{
            Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::Threading::WaitForSingleObject,
        };
        match unsafe { WaitForSingleObject(self.handle(), 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub fn terminate_and_wait(&self, timeout: u32) -> io::Result<()> {
        use windows_sys::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::*};
        if !self.exited()? && unsafe { TerminateProcess(self.handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { WaitForSingleObject(self.handle(), timeout) } != WAIT_OBJECT_0 {
            return Err(io::Error::other("process exit unresolved"));
        }
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::WindowsProcess;
    use std::os::windows::io::AsHandle;

    #[test]
    fn duplicated_process_handle_survives_original_owner_drop() {
        let original = WindowsProcess::open(std::process::id(), false).unwrap();
        let duplicate = WindowsProcess::duplicate(original.0.as_handle()).unwrap();
        let identity = original.identity().unwrap();
        drop(original);
        assert_eq!(duplicate.identity().unwrap(), identity);
        assert!(!duplicate.exited().unwrap());
    }
}
