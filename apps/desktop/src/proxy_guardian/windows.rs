use super::pipe::Pipe;
use super::{
    protocol::{Arguments, failure, validate_user},
    security, service,
    wire::{self, Response},
};
use lanpath_client::sysproxy::SystemProxySession;
use lanpath_common::{LanpathResult, WindowsProcess};
use std::{os::windows::io::AsHandle, path::Path};
use windows_sys::Win32::{Foundation::HANDLE, System::Threading::GetCurrentProcess};

pub(super) fn same_image(left: &Path, right: &Path) -> bool {
    left.to_str()
        .zip(right.to_str())
        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
}

pub(super) fn watch(args: Arguments) -> LanpathResult<()> {
    crate::windows_access::ensure_unelevated()?;
    let current = unsafe { GetCurrentProcess() };
    if security::in_job(current)? {
        return Err(failure(
            "guardian cannot recover independently inside a Job",
        ));
    }
    // Never reopen a PID: the inherited process object remains the watched GUI.
    let inherited_parent = security::owned(args.parent as HANDLE)?;
    let parent = WindowsProcess::duplicate(inherited_parent.as_handle())?;
    let actual = parent.identity()?;
    args.validate_parent(&actual)?;
    validate_user(&security::user(parent.handle())?, &security::user(current)?)?;
    if !same_image(&actual.exe, &std::env::current_exe()?) {
        return Err(failure(
            "watched GUI and guardian are not the same product image",
        ));
    }
    let mut pipe = Pipe::client(&args.pipe)?;
    if pipe.peer_pid(true)? != actual.pid {
        return Err(failure("guardian pipe server is not the watched GUI"));
    }
    pipe.bind_peer(parent);
    // The independent process owns the actual session and its lease. GUI death
    // cannot interrupt apply/readback/checkpoint or restore/readback/checkpoint.
    let session = SystemProxySession::windows_with_owner(crate::system_proxy::APP_ID, &args.owner)?;
    session.has_recovery_for_owner(&args.owner)?;
    wire::write_frame(&mut pipe, 0, &Response::Ready(args.owner.clone()))?;
    service::serve(&session, &mut pipe)
}
