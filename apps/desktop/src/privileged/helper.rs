use super::{
    core_channel::CoreChannel,
    firewall::Firewall,
    ipc::Pipe,
    journal::{self, Journal, Record},
    managed_protocol::Event,
    network,
    protocol::*,
    security,
    staging::{Stage, TrustedBundle},
};
use lanpath_common::{LanpathResult, LifecycleGuard, ProcessIdentity, WindowsProcess};
use std::{
    io,
    os::windows::io::AsHandle,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Console::*, Threading::*},
    UI::WindowsAndMessaging::{SW_HIDE, ShowWindow},
};
#[path = "session_stop.rs"]
mod stop;

struct Console;
impl Console {
    fn private() -> LanpathResult<Self> {
        unsafe {
            FreeConsole();
        }
        if unsafe { AllocConsole() } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let window = unsafe { GetConsoleWindow() };
        if !window.is_null() {
            unsafe {
                ShowWindow(window, SW_HIDE);
            }
        }
        Ok(Self)
    }
    fn attach(pid: u32) -> LanpathResult<Self> {
        unsafe {
            FreeConsole();
        }
        if unsafe { AttachConsole(pid) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(Self)
    }
}
impl Drop for Console {
    fn drop(&mut self) {
        unsafe {
            FreeConsole();
        }
    }
}
struct Running {
    process: WindowsProcess,
    child: Child,
    channel: CoreChannel,
    _job: LifecycleGuard,
    _console: Console,
}
struct Service {
    journal: Journal,
    trusted: TrustedBundle,
    parent: ProcessIdentity,
    owner_sid: String,
    root_id: uuid::Uuid,
    record: Option<Record>,
    stage: Option<Stage>,
    running: Option<Running>,
}
impl Service {
    fn new(
        trusted: TrustedBundle,
        parent: ProcessIdentity,
        owner_sid: String,
        root_id: uuid::Uuid,
    ) -> LanpathResult<Self> {
        let journal = Journal::acquire()?;
        let record = journal.load()?;
        if record
            .as_ref()
            .is_some_and(|record| record.owner_sid != owner_sid || record.root_id != root_id)
        {
            return Err(failure(
                "machine TUN journal belongs to another Windows user or data root; no recovery or status mutation authorized",
            ));
        }
        Ok(Self {
            journal,
            trusted,
            parent,
            owner_sid,
            root_id,
            record,
            stage: None,
            running: None,
        })
    }
    fn status(&mut self) -> LanpathResult<BrokerStatus> {
        if let Some(record) = self.record.as_mut()
            && record.core.is_some()
        {
            let observed =
                network::remaining(&record.prepared, record.adapter.as_ref(), &record.observed)?;
            record.observed = observed;
            if let Some(running) = &self.running
                && running.process.exited()?
                && record.state == SessionState::Running
            {
                record.state = SessionState::RecoveryRequired;
                record.uncertain = true;
                record.detail =
                    "core exited without a graceful helper stop receipt; recovery required".into();
            }
            self.journal.save(record)?;
        }
        Ok(match &self.record {
            Some(record) => BrokerStatus {
                state: record.state.clone(),
                prepared: Some(record.prepared.clone()),
                core: record.core.clone(),
                observed: record.observed.clone(),
                detail: record.detail.clone(),
            },
            None => BrokerStatus {
                state: SessionState::Idle,
                prepared: None,
                core: None,
                observed: Vec::new(),
                detail: "authenticated helper ready; no machine TUN session owned".into(),
            },
        })
    }
    fn prepare(
        &mut self,
        config: serde_json::Value,
        resources: Vec<Resource>,
        selected_core_sha256: String,
    ) -> LanpathResult<PreparedSession> {
        if selected_core_sha256.len() != 64
            || !selected_core_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(failure(
                "selected core SHA-256 must be 64 hexadecimal characters",
            ));
        }
        // The selection only checks the already verified bundle; it never authorizes another core.
        if !selected_core_sha256.eq_ignore_ascii_case(&self.trusted.core_sha256) {
            return Err(failure(
                "selected core SHA-256 does not match the trusted product bundle core",
            ));
        }
        if self.record.is_some() {
            return Err(failure(
                "existing machine TUN session must be stopped or recovered before preparing another",
            ));
        }
        let session = uuid::Uuid::new_v4().to_string();
        let prepared = PreparedSession {
            interface_name: journal::interface_name(&session)?,
            session,
            adapter_guid: uuid::Uuid::new_v4(),
        };
        if !network::remaining(&prepared, None, &[])?.is_empty() {
            return Err(failure(
                "managed interface name or GUID already exists; no adapter may be adopted",
            ));
        }
        let sessions = self.journal.root().join("sessions");
        security::protected_directory(&sessions)?;
        let mut stage = Stage::create(
            &sessions,
            &prepared.session,
            &prepared.interface_name,
            &mut self.trusted,
            config,
            &resources,
        )?;
        if stage.cache_enabled {
            super::core_cache::load(
                self.journal.root(),
                &self.owner_sid,
                self.root_id,
                &stage.path,
            )?;
        }
        let record = Record {
            schema: 3,
            owner_sid: self.owner_sid.clone(),
            root_id: self.root_id,
            prepared: prepared.clone(),
            state: SessionState::Prepared,
            parent: self.parent.clone(),
            helper: ProcessIdentity::capture(std::process::id())?,
            core: None,
            core_sha256: self.trusted.core_sha256.clone(),
            managed_source_digest: crate::product::CORE_MANAGED_SOURCE_DIGEST.to_owned(),
            cache_enabled: stage.cache_enabled,
            creation_authorized: false,
            adapter: None,
            firewall_needed: stage.firewall_needed,
            firewall: None,
            ready: false,
            observed: Vec::new(),
            forced: false,
            uncertain: false,
            detail: "immutable protected configuration prepared; core not executed".into(),
        };
        self.journal.save(&record)?;
        stage.retain_for_recovery();
        self.stage = Some(stage);
        self.record = Some(record);
        Ok(prepared)
    }
    fn start(&mut self) -> LanpathResult<BrokerStatus> {
        match self.start_managed() {
            Ok(status) => Ok(status),
            Err(error) => match self.stop(false) {
                Ok(receipt) => Err(failure(format!(
                    "managed TUN startup failed: {error}; stop {:?}, cleanup proven={}: {}",
                    receipt.kind, receipt.precise_cleanup_proven, receipt.detail
                ))),
                Err(stop) => Err(failure(format!(
                    "managed TUN startup failed: {error}; recovery retained: {stop}"
                ))),
            },
        }
    }
    fn start_managed(&mut self) -> LanpathResult<BrokerStatus> {
        let record = self
            .record
            .as_mut()
            .ok_or_else(|| failure("no prepared TUN configuration"))?;
        if record.state != SessionState::Prepared || self.running.is_some() {
            return Err(failure("TUN session is not in prepared state"));
        }
        if !network::remaining(&record.prepared, None, &[])?.is_empty() {
            return Err(failure(
                "managed interface appeared before execution; refusing adoption",
            ));
        }
        let stage = self.stage.as_ref().ok_or_else(|| {
            failure("prepared session belongs to a previous helper; recover it first")
        })?;
        let channel =
            CoreChannel::create(&record.prepared, &record.helper, record.firewall_needed)?;
        // Durable intent precedes execution; adapter configuration remains blocked
        // until the authenticated core's native identity receipt has been fsynced.
        record.state = SessionState::RecoveryRequired;
        record.uncertain = true;
        record.detail =
            "managed core launch intent persisted; no creation permission acknowledged".into();
        self.journal.save(record)?;
        let console = Console::private()?;
        let mut command = Command::new(&stage.core_path);
        command
            .args(["run", "-c"])
            .arg(stage.path.join("config.json"))
            .arg("-D")
            .arg(&stage.path)
            .current_dir(&stage.path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let system = security::windows_directory()?;
        command
            .env_clear()
            .env("SystemRoot", &system)
            .env("WINDIR", &system)
            .env("TEMP", &stage.path)
            .env("TMP", &stage.path)
            .env("LANPATH_MANAGED_SESSION", channel.environment());
        let (child, job) = lanpath_common::spawn_guarded_console(&mut command)?;
        let process = WindowsProcess::duplicate(child.as_handle())?;
        record.core = Some(process.identity()?);
        self.running = Some(Running {
            process,
            child,
            channel,
            _job: job,
            _console: console,
        });
        self.journal.save(record)?;
        let running = self
            .running
            .as_mut()
            .ok_or_else(|| failure("core ownership missing"))?;
        running.channel.authenticate(&running.process)?;
        while !running.channel.complete() {
            match running.channel.receive()? {
                Event::Hello => {
                    if !network::remaining(&record.prepared, None, &[])?.is_empty() {
                        return Err(failure(
                            "managed adapter conflict before creation permission",
                        ));
                    }
                    record.creation_authorized = true;
                    record.detail =
                        "authenticated managed core authorized to create one fresh adapter".into();
                }
                Event::AdapterCreated { guid, luid, index } => {
                    let receipt = AdapterReceipt { guid, luid, index };
                    let observed = network::created(&record.prepared, &receipt)?;
                    record.adapter = Some(receipt);
                    record.observed = vec![observed];
                    record.detail = "native created GUID/LUID/index verified; configuration permission persisted".into();
                }
                Event::Firewall => {
                    let firewall = Firewall::open()?;
                    let plan = firewall.prepare(
                        uuid::Uuid::parse_str(&record.prepared.session)
                            .map_err(|_| failure("invalid session nonce"))?,
                        &stage.core_path,
                    )?;
                    record.firewall = Some(plan.snapshot);
                    self.journal.save(record)?;
                    firewall.install(
                        &plan.rule,
                        record
                            .firewall
                            .as_ref()
                            .ok_or_else(|| failure("firewall intent missing"))?,
                    )?;
                    record.detail =
                        "owned application-bound firewall rule installed and fully read back"
                            .into();
                }
                Event::Ready => {
                    record.ready = true;
                    record.uncertain = false;
                    record.state = SessionState::Running;
                    record.detail = "complete managed Box.Start confirmed after durable native adapter/firewall receipts".into();
                }
            }
            self.journal.save(record)?;
            running.channel.acknowledge()?;
        }
        if running.process.exited()? {
            return Err(failure(
                "managed core exited during readiness acknowledgement",
            ));
        }
        self.status()
    }
}

pub(super) fn serve(
    mut pipe: Pipe,
    trusted: TrustedBundle,
    parent: ProcessIdentity,
    owner_sid: String,
    root_id: uuid::Uuid,
) -> LanpathResult<()> {
    let mut service = match Service::new(trusted, parent, owner_sid, root_id) {
        Ok(service) => service,
        Err(error) => {
            write_frame(&mut pipe, 0, Response::Error(error.to_string()))?;
            return Err(error);
        }
    };
    write_frame(&mut pipe, 0, Response::Status(service.status()?))?;
    let mut sequence = 1u64;
    let result = loop {
        let request: Request = match read_frame(&mut pipe, sequence) {
            Ok(request) => request,
            Err(error) => break Err(error),
        };
        let response = match request {
            Request::Prepare {
                config,
                resources,
                selected_core_sha256,
            } => service
                .prepare(config, resources, selected_core_sha256)
                .map(Response::Prepared),
            Request::Start => service.start().map(Response::Status),
            Request::Status => service.status().map(Response::Status),
            Request::Stop => service.stop(false).map(Response::Stopped),
            Request::Recover => service.stop(true).map(Response::Stopped),
        }
        .unwrap_or_else(|error| Response::Error(error.to_string()));
        if let Err(error) = write_frame(&mut pipe, sequence, response) {
            break Err(error);
        }
        sequence = match sequence.checked_add(1) {
            Some(sequence) => sequence,
            None => break Err(failure("privileged sequence overflow")),
        };
    };
    // EOF, authenticated parent death, malformed frames, and IPC failure all use the same
    // bounded graceful stop path. Job close remains fail-closed containment, not cleanup proof.
    if let Err(error) = service.stop(false) {
        return Err(failure(format!(
            "helper channel closed; owned stop unresolved: {error}"
        )));
    }
    result
}
