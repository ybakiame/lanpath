//! Recovery identities are OS creation identities, never timestamps supplied by callers.
use std::{io, path::PathBuf};

#[cfg(target_os = "macos")]
mod macos;

/// The operation actually needed to stop an owned process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStopKind {
    /// A termination request succeeded and process exit was confirmed.
    Forced,
    /// The process had already exited; no termination request was needed.
    AlreadyStopped,
}

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

    /// Bind creation and the live image to one kernel object; never terminate a reused PID.
    pub fn terminate_verified(&self) -> io::Result<ProcessStopKind> {
        #[cfg(windows)]
        {
            let process = WindowsProcess::open(self.pid, true)?;
            if process.created()? != self.created {
                return Err(io::Error::other("process identity mismatch"));
            }
            if process.exited()? {
                return Ok(ProcessStopKind::AlreadyStopped);
            }
            match process.executable() {
                Ok(exe) if exe == self.exe => process.terminate_and_wait(10_000),
                Ok(_) => Err(io::Error::other("process identity mismatch")),
                // Image queries can stop working between the live check and
                // this query. Only a signalled, creation-matched kernel object
                // proves absence; an arbitrary query failure is not authority.
                Err(_) if process.exited()? => Ok(ProcessStopKind::AlreadyStopped),
                Err(error) => Err(error),
            }
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
            Ok(ProcessStopKind::Forced)
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
        use windows_sys::Win32::System::Threading::GetProcessId;
        Ok(ProcessIdentity {
            pid: unsafe { GetProcessId(self.handle()) },
            created: self.created()?,
            exe: self.executable()?,
        })
    }
    fn created(&self) -> io::Result<u64> {
        use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetProcessTimes};
        let mut times: [FILETIME; 4] = unsafe { std::mem::zeroed() };
        let p = times.as_mut_ptr();
        if unsafe { GetProcessTimes(self.handle(), p, p.add(1), p.add(2), p.add(3)) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((u64::from(times[0].dwHighDateTime) << 32) | u64::from(times[0].dwLowDateTime))
    }
    fn executable(&self) -> io::Result<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::System::Threading::QueryFullProcessImageNameW;
        let mut path = [0u16; 32_768];
        let mut size = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(self.handle(), 0, path.as_mut_ptr(), &mut size) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(PathBuf::from(std::ffi::OsString::from_wide(
            &path[..size as usize],
        )))
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
    pub fn terminate_and_wait(&self, timeout: u32) -> io::Result<ProcessStopKind> {
        use windows_sys::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::*};
        let kind = if self.exited()? {
            ProcessStopKind::AlreadyStopped
        } else if unsafe { TerminateProcess(self.handle(), 1) } != 0 {
            ProcessStopKind::Forced
        } else {
            let error = io::Error::last_os_error();
            if !self.exited()? {
                return Err(error);
            }
            ProcessStopKind::AlreadyStopped
        };
        if unsafe { WaitForSingleObject(self.handle(), timeout) } != WAIT_OBJECT_0 {
            return Err(io::Error::other("process exit unresolved"));
        }
        Ok(kind)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::{ProcessStopKind, WindowsProcess};
    use std::os::windows::io::AsHandle;

    struct ChildFixture(std::process::Child);

    impl ChildFixture {
        fn spawn() -> Self {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "process::identity::tests::stop_child_fixture",
                    "--nocapture",
                ])
                .env("LANPATH_PROCESS_STOP_FIXTURE", "1")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            Self(crate::no_window(&mut command).spawn().unwrap())
        }
    }

    impl Drop for ChildFixture {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn stop_child_fixture() {
        if std::env::var_os("LANPATH_PROCESS_STOP_FIXTURE").is_some() {
            use std::io::Read;
            let _ = std::io::stdin().read_exact(&mut [0]);
        }
    }

    #[test]
    fn native_termination_returns_forced_only_for_the_actual_request() {
        let mut child = ChildFixture::spawn();
        let process = WindowsProcess::duplicate(child.0.as_handle()).unwrap();
        assert_eq!(
            process.terminate_and_wait(10_000).unwrap(),
            ProcessStopKind::Forced
        );
        assert!(!child.0.wait().unwrap().success());
        assert_eq!(
            process.terminate_and_wait(10_000).unwrap(),
            ProcessStopKind::AlreadyStopped
        );
    }

    #[test]
    fn verified_identity_already_exited_is_not_a_forced_stop() {
        let mut child = ChildFixture::spawn();
        let identity = super::ProcessIdentity::capture(child.0.id()).unwrap();
        // Keep an exact kernel handle alive after exit, as stale recovery may encounter.
        let process = WindowsProcess::duplicate(child.0.as_handle()).unwrap();
        let mut reused = identity.clone();
        reused.created ^= 1;
        assert!(reused.terminate_verified().is_err());
        assert!(!process.exited().unwrap());
        drop(child.0.stdin.take());
        assert!(child.0.wait().unwrap().success());
        assert!(reused.terminate_verified().is_err());
        assert_eq!(
            identity.terminate_verified().unwrap(),
            ProcessStopKind::AlreadyStopped
        );
        assert_eq!(
            process.terminate_and_wait(10_000).unwrap(),
            ProcessStopKind::AlreadyStopped
        );
    }

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
