use super::{protocol::failure, security};
use crate::windows_access::Descriptor;
use lanpath_common::{LanpathResult, WindowsProcess};
use std::{
    io::{self, Read, Write},
    os::windows::io::{AsRawHandle, OwnedHandle},
    ptr,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{IO::*, Pipes::*, Threading::*},
};

pub(crate) struct Pipe {
    handle: OwnedHandle,
    peer: Option<WindowsProcess>,
    read_deadline: Option<Instant>,
}
impl Pipe {
    pub fn server(name: &str, sid: &str) -> LanpathResult<Self> {
        // 0x12019b deliberately excludes FILE_CREATE_PIPE_INSTANCE (FILE_APPEND_DATA).
        let descriptor = Descriptor::new(&format!(
            "D:P(A;;0x12019b;;;{sid})(A;;0x12019b;;;BA)(A;;0x12019b;;;SY)"
        ))?;
        let handle = unsafe {
            CreateNamedPipeW(
                security::wide(name).as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                0,
                &descriptor.attributes(),
            )
        };
        Ok(Self {
            handle: security::owned(handle)?,
            peer: None,
            read_deadline: None,
        })
    }
    pub fn client(name: &str) -> LanpathResult<Self> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let handle = unsafe {
                CreateFileW(
                    security::wide(name).as_ptr(),
                    0x12019b,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    ptr::null_mut(),
                )
            };
            match security::owned(handle) {
                Ok(handle) => {
                    return Ok(Self {
                        handle,
                        peer: None,
                        read_deadline: None,
                    });
                }
                Err(e)
                    if matches!(e.raw_os_error(), Some(2 | 231)) && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(30))
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
    pub fn handle(&self) -> HANDLE {
        self.handle.as_raw_handle()
    }
    pub fn bind_peer(&mut self, peer: WindowsProcess) {
        self.peer = Some(peer);
    }
    pub fn bound_reads_until(&mut self, deadline: Instant) {
        self.read_deadline = Some(deadline);
    }
    pub fn connect(&mut self, launched: &WindowsProcess) -> LanpathResult<()> {
        self.connect_bounded(launched, Duration::from_secs(120))
    }
    pub fn connect_bounded(
        &mut self,
        launched: &WindowsProcess,
        timeout: Duration,
    ) -> LanpathResult<()> {
        let event = security::owned(unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) })?;
        let mut operation: OVERLAPPED = unsafe { std::mem::zeroed() };
        operation.hEvent = event.as_raw_handle();
        if unsafe { ConnectNamedPipe(self.handle(), &mut operation) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
            return Ok(());
        }
        if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
            return Err(error.into());
        }
        let handles = [event.as_raw_handle(), launched.handle()];
        let result = unsafe {
            WaitForMultipleObjects(
                2,
                handles.as_ptr(),
                0,
                timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32,
            )
        };
        if result != WAIT_OBJECT_0 {
            unsafe {
                CancelIoEx(self.handle(), &operation);
                WaitForSingleObject(event.as_raw_handle(), INFINITE);
            }
            return Err(failure(
                "named pipe peer did not authenticate before launch deadline",
            ));
        }
        let mut transferred = 0;
        if unsafe { GetOverlappedResult(self.handle(), &operation, &mut transferred, 0) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }
    pub fn peer_pid(&self, server: bool) -> LanpathResult<u32> {
        let mut pid = 0;
        let ok = unsafe {
            if server {
                GetNamedPipeServerProcessId(self.handle(), &mut pid)
            } else {
                GetNamedPipeClientProcessId(self.handle(), &mut pid)
            }
        };
        if ok == 0 || pid == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(pid)
    }
    fn transfer(&mut self, buffer: *mut u8, length: usize, reading: bool) -> io::Result<usize> {
        if length == 0 {
            return Ok(0);
        }
        // Never leave an unowned OVERLAPPED operation pending on an error path.
        let peer = self
            .peer
            .as_ref()
            .ok_or_else(|| io::Error::other("pipe peer has not authenticated"))?;
        if reading
            && self
                .read_deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "managed core handshake deadline expired",
            ));
        }
        let event = security::owned(unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) })?;
        let mut operation: OVERLAPPED = unsafe { std::mem::zeroed() };
        operation.hEvent = event.as_raw_handle();
        let mut count = 0;
        let ok = unsafe {
            if reading {
                ReadFile(
                    self.handle(),
                    buffer,
                    length.min(u32::MAX as usize) as u32,
                    &mut count,
                    &mut operation,
                )
            } else {
                WriteFile(
                    self.handle(),
                    buffer,
                    length.min(u32::MAX as usize) as u32,
                    &mut count,
                    &mut operation,
                )
            }
        };
        if ok != 0 {
            return Ok(count as usize);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
            return Err(error);
        }
        let handles = [event.as_raw_handle(), peer.handle()];
        // Idle reads last for the UI session, but peer death interrupts them. Writes are bounded.
        let timeout = if reading {
            self.read_deadline
                .map(|deadline| {
                    deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis()
                        .min(u128::from(u32::MAX - 1)) as u32
                })
                .unwrap_or(INFINITE)
        } else {
            30_000
        };
        let result = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, timeout) };
        if result != WAIT_OBJECT_0 {
            unsafe {
                CancelIoEx(self.handle(), &operation);
                WaitForSingleObject(event.as_raw_handle(), INFINITE);
            }
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "authenticated pipe peer exited or stopped receiving",
            ));
        }
        if unsafe { GetOverlappedResult(self.handle(), &operation, &mut count, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(count as usize)
    }
}
impl Read for Pipe {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.transfer(bytes.as_mut_ptr(), bytes.len(), true)
    }
}
impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.transfer(bytes.as_ptr().cast_mut(), bytes.len(), false)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
