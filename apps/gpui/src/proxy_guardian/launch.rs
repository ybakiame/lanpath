use super::{
    protocol::{failure, validate_user},
    rpc::Client,
    security,
    windows::same_image,
    wire::{Request, Response},
};
use crate::privileged::Pipe;
use lanpath_client::sysproxy::SystemProxyOwnership;
use lanpath_common::{LanpathResult, WindowsProcess};
use std::{
    io,
    os::windows::io::{AsHandle, AsRawHandle},
    ptr,
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::*, System::Threading::*};

const HANDSHAKE_MS: u32 = 10_000;

pub(crate) struct Guardian {
    process: WindowsProcess,
    rpc: Client<Pipe>,
}

impl Guardian {
    pub(crate) fn arm(owner: &str) -> LanpathResult<Self> {
        crate::windows_gui::ensure_unelevated()?;
        let current = unsafe { GetCurrentProcess() };
        let parent_user = security::user(current)?;
        let parent = WindowsProcess::open(std::process::id(), false)?;
        let identity = parent.identity()?;
        let executable = std::env::current_exe()?;
        if !same_image(&identity.exe, &executable) {
            return Err(failure("current image identity mismatch"));
        }
        let pipe_name = format!(r"\\.\pipe\LanpathGPUI-Proxy-{}", uuid::Uuid::new_v4());
        let mut pipe = Pipe::server(&pipe_name, &parent_user.sid)?;
        // Only the exact GUI process/query handle is inherited. The authenticated
        // named pipe has no filesystem/command authority and accepts no remote peers.
        let mut inherited_parent = ptr::null_mut();
        if unsafe {
            DuplicateHandle(
                current,
                parent.handle(),
                current,
                &mut inherited_parent,
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                1,
                0,
            )
        } == 0
        {
            return Err(io::Error::last_os_error().into());
        }
        let inherited_parent = security::owned(inherited_parent)?;
        let handles = [inherited_parent.as_raw_handle()];
        let list = AttributeList::new(&handles)?;
        let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = list.pointer();
        let image = security::wide(&executable);
        let mut command = security::wide(format!(
            "\"{}\" --proxy-guardian {} {} {} {} {}",
            executable.display(),
            owner,
            identity.pid,
            identity.created,
            handles[0] as usize,
            pipe_name,
        ));
        let mut flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW;
        if security::in_job(current)? {
            // No fallback: a restrictive containing Job must fail before WinINet apply.
            flags |= CREATE_BREAKAWAY_FROM_JOB;
        }
        let mut launched: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        // Never suspend: GUI death between creation and ResumeThread would orphan a
        // suspended guardian forever. The child authenticates and verifies its Job
        // independence before signalling ready; the GUI mutates nothing before ready.
        if unsafe {
            CreateProcessW(
                image.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                flags,
                ptr::null(),
                ptr::null(),
                &startup.StartupInfo,
                &mut launched,
            )
        } == 0
        {
            return Err(io::Error::last_os_error().into());
        }
        let process_handle = security::owned(launched.hProcess)?;
        let _thread = security::owned(launched.hThread)?;
        let process = match WindowsProcess::duplicate(process_handle.as_handle()) {
            Ok(process) => process,
            Err(error) => {
                // A failed handle duplication must not leave an untracked child.
                if unsafe { TerminateProcess(process_handle.as_raw_handle(), 1) } == 0
                    || unsafe { WaitForSingleObject(process_handle.as_raw_handle(), HANDSHAKE_MS) }
                        != WAIT_OBJECT_0
                {
                    return Err(failure(format!(
                        "guardian handle setup failed: {error}; exact-child cleanup unresolved"
                    )));
                }
                return Err(error.into());
            }
        };
        let handshake = (|| {
            if security::in_job(process.handle())? {
                return Err(failure(
                    "guardian inherited a Job; independent recovery unavailable",
                ));
            }
            validate_user(&parent_user, &security::user(process.handle())?)?;
            let guardian_identity = process.identity()?;
            if !same_image(&guardian_identity.exe, &identity.exe) {
                return Err(failure("guardian image mismatch"));
            }
            pipe.connect_bounded(&process, Duration::from_millis(u64::from(HANDSHAKE_MS)))?;
            if pipe.peer_pid(false)? != guardian_identity.pid {
                return Err(failure("guardian pipe client is not the launched process"));
            }
            pipe.bind_peer(WindowsProcess::duplicate(process_handle.as_handle())?);
            pipe.bound_reads_until(Instant::now() + Duration::from_millis(u64::from(HANDSHAKE_MS)));
            let rpc = Client::accept_ready(pipe, owner)?;
            if process.exited()? {
                return Err(failure("guardian exited before acquisition"));
            }
            Ok(rpc)
        })();
        let rpc = match handshake {
            Ok(rpc) => rpc,
            Err(error) => {
                // No acquisition RPC was sent. Terminate only this exact child.
                if let Err(cleanup) = process.terminate_and_wait(HANDSHAKE_MS) {
                    return Err(failure(format!(
                        "{error}; exact-child cleanup unresolved: {cleanup}"
                    )));
                }
                return Err(error);
            }
        };
        Ok(Self { process, rpc })
    }

    fn request(&mut self, request: Request) -> LanpathResult<Response> {
        if self.process.exited()? {
            return Err(failure(
                "guardian exited; ownership requires explicit recovery",
            ));
        }
        self.rpc
            .transport()?
            .bound_reads_until(Instant::now() + Duration::from_secs(45));
        self.rpc.request(request)
    }

    pub(crate) fn acquire(&mut self, port: u16) -> LanpathResult<()> {
        self.request(Request::Acquire(port)).map(|_| ())
    }

    pub(crate) fn recover_abandoned(&mut self) -> LanpathResult<()> {
        self.request(Request::RecoverAbandoned).map(|_| ())
    }

    pub(crate) fn recover_watched(&mut self) -> LanpathResult<()> {
        self.request(Request::RecoverWatched).map(|_| ())
    }

    pub(crate) fn status(&mut self) -> LanpathResult<SystemProxyOwnership> {
        if self.process.exited()? {
            return Ok(SystemProxyOwnership::RecoveryRequired);
        }
        match self.request(Request::Status)? {
            Response::Status(status) => Ok(status),
            _ => Err(failure("guardian status response mismatch")),
        }
    }

    /// A timeout/disconnection is not rollback. Keep core/MITM and this process
    /// tracked until it exits; the caller must then verify matching journal recovery.
    pub(crate) fn shutdown(&mut self) -> LanpathResult<()> {
        if !self.process.exited()? && self.rpc.connected() {
            self.request(Request::Stop)?;
        }
        if unsafe { WaitForSingleObject(self.process.handle(), HANDSHAKE_MS) } != WAIT_OBJECT_0 {
            return Err(failure("guardian restoration/exit deadline expired"));
        }
        Ok(())
    }
}

struct AttributeList(Vec<usize>);
impl AttributeList {
    fn new(handles: &[HANDLE]) -> LanpathResult<Self> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut bytes);
        }
        if bytes == 0 || bytes > 65536 {
            return Err(failure("invalid process attribute-list size"));
        }
        let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        let pointer = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(pointer, 1, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let list = Self(storage);
        if unsafe {
            UpdateProcThreadAttribute(
                list.pointer(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(handles),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error().into());
        }
        Ok(list)
    }
    fn pointer(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_ptr().cast_mut().cast()
    }
}
impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer());
        }
    }
}
