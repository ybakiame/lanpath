use super::{ipc::Pipe, protocol::*, security, staging};
use lanpath_common::{LanpathResult, WindowsProcess};
use serde_json::Value;
use std::{ffi::OsString, io, os::windows::io::AsHandle, path::Path};
use windows_sys::Win32::{
    Foundation::*,
    Security::TokenElevationTypeLimited,
    System::{LibraryLoader::*, Threading::*},
    UI::{Shell::*, WindowsAndMessaging::SW_HIDE},
};

pub struct Broker {
    pipe: Pipe,
    sequence: u64,
    _helper: WindowsProcess,
}
impl Broker {
    pub fn launch(root_id: uuid::Uuid) -> LanpathResult<Self> {
        if root_id.is_nil() {
            return Err(failure(
                "TUN requires a persistent non-nil data-root identity",
            ));
        }
        let parent = WindowsProcess::open(std::process::id(), false)?;
        let parent_identity = parent.identity()?;
        let token = security::token_identity(parent.handle())?;
        if token.elevation != TokenElevationTypeLimited {
            return Err(failure(
                "TUN helper requires an unelevated split-token Windows user",
            ));
        }
        let trusted = staging::trusted_bundle()?;
        // Keep the protected image, all signed files and ancestors held through UAC.
        let _protected_bundle = super::broker_staging::ensure(&trusted, &token.sid)?;
        let protected = _protected_bundle.root.join(crate::product::BINARY_NAME);
        let pipe_name = format!(r"\\.\pipe\LanpathGPUI-{}", uuid::Uuid::new_v4());
        let mut pipe = Pipe::server(&pipe_name, &token.sid)?;
        let parameters = format!(
            "--privileged-helper {} {} {} {}",
            pipe_name, parent_identity.pid, parent_identity.created, root_id
        );
        let helper = elevate(&protected, &parameters)?;
        let expected = helper.identity()?;
        security::same_split_user(&token, &security::token_identity(helper.handle())?)?;
        pipe.connect(&helper)?;
        let actual = security::process_for_pipe(pipe.peer_pid(false)?, &expected)?;
        if !same_image_path(&actual.identity()?.exe, &protected) {
            return Err(failure("helper process image path mismatch"));
        }
        pipe.bind_peer(actual);
        let ready: Response = read_frame(&mut pipe, 0)?;
        match ready {
            Response::Status(_) => Ok(Self {
                pipe,
                sequence: 0,
                _helper: helper,
            }),
            Response::Error(message) => Err(failure(message)),
            _ => Err(failure("invalid helper greeting")),
        }
    }
    fn request(&mut self, request: Request) -> LanpathResult<Response> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| failure("privileged sequence overflow"))?;
        write_frame(&mut self.pipe, self.sequence, request)?;
        match read_frame(&mut self.pipe, self.sequence)? {
            Response::Error(message) => Err(failure(message)),
            response => Ok(response),
        }
    }
    pub fn prepare(
        &mut self,
        config: Value,
        resources: Vec<Resource>,
        selected_core_sha256: String,
    ) -> LanpathResult<PreparedSession> {
        match self.request(Request::Prepare {
            config,
            resources,
            selected_core_sha256,
        })? {
            Response::Prepared(session) => Ok(session),
            _ => Err(failure("invalid prepare response")),
        }
    }
    pub fn start(&mut self) -> LanpathResult<BrokerStatus> {
        match self.request(Request::Start)? {
            Response::Status(status) => Ok(status),
            _ => Err(failure("invalid start response")),
        }
    }
    pub fn status(&mut self) -> LanpathResult<BrokerStatus> {
        match self.request(Request::Status)? {
            Response::Status(status) => Ok(status),
            _ => Err(failure("invalid status response")),
        }
    }
    pub fn stop(&mut self) -> LanpathResult<StopReceipt> {
        match self.request(Request::Stop)? {
            Response::Stopped(receipt) => Ok(receipt),
            _ => Err(failure("invalid stop response")),
        }
    }
    pub fn recover(&mut self) -> LanpathResult<StopReceipt> {
        match self.request(Request::Recover)? {
            Response::Stopped(receipt) => Ok(receipt),
            _ => Err(failure("invalid recovery response")),
        }
    }
}

fn same_image_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(a), Ok(b)) => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
        _ => false,
    }
}
fn elevate(image: &Path, parameters: &str) -> LanpathResult<WindowsProcess> {
    let file = security::wide(image);
    let parameters = security::wide(parameters);
    let directory = security::wide(
        image
            .parent()
            .ok_or_else(|| failure("helper executable has no parent"))?,
    );
    let verb = security::wide("runas");
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.lpDirectory = directory.as_ptr();
    info.nShow = SW_HIDE;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = io::Error::last_os_error();
        return Err(failure(
            if error.raw_os_error() == Some(ERROR_CANCELLED as i32) {
                "TUN elevation was cancelled".into()
            } else {
                format!("cannot launch privileged helper: {error}")
            },
        ));
    }
    let handle = security::owned(info.hProcess)?;
    Ok(WindowsProcess::duplicate(handle.as_handle())?)
}

pub(super) fn dispatch(args: Vec<OsString>) -> LanpathResult<()> {
    // Restrict delayed DLL loads. Pre-main loading must already be protected by staging.
    if unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    if args.len() != 5 {
        return Err(failure("invalid privileged helper arguments"));
    }
    let name = args[1]
        .to_str()
        .ok_or_else(|| failure("invalid pipe encoding"))?;
    let prefix = r"\\.\pipe\LanpathGPUI-";
    if !name
        .strip_prefix(prefix)
        .is_some_and(|suffix| uuid::Uuid::parse_str(suffix).is_ok())
    {
        return Err(failure("invalid product pipe name"));
    }
    let pid: u32 = args[2]
        .to_str()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| failure("invalid parent PID"))?;
    let created: u64 = args[3]
        .to_str()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| failure("invalid parent creation identity"))?;
    let root_id = args[4]
        .to_str()
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .filter(|id| !id.is_nil())
        .ok_or_else(|| failure("invalid data-root identity"))?;
    let executable = std::env::current_exe()?;
    let trusted = staging::trusted_bundle_at(&executable)?;
    let helper_token = security::token_identity(unsafe { GetCurrentProcess() })?;
    super::broker_staging::validate_location(
        executable
            .parent()
            .ok_or_else(|| failure("helper image has no parent"))?,
        &trusted.image_sha256,
        &helper_token.sid,
    )?;
    let mut pipe = Pipe::client(name)?;
    if pipe.peer_pid(true)? != pid {
        return Err(failure("named pipe server is not the launching UI"));
    }
    let parent = WindowsProcess::open(pid, false)?;
    let actual = parent.identity()?;
    if actual.pid != pid || actual.created != created {
        return Err(failure("launching UI process identity changed"));
    }
    let parent_token = security::token_identity(parent.handle())?;
    security::same_split_user(&parent_token, &helper_token)?;
    let _parent_image = staging::lock_parent_image(&actual.exe, &trusted.image_sha256)?;
    pipe.bind_peer(parent);
    super::helper::serve(pipe, trusted, actual, parent_token.sid, root_id)
}
