use super::{
    ipc::Pipe,
    managed_protocol::{self, Event, Handshake},
    protocol::{PreparedSession, failure},
    security,
};
use lanpath_common::{LanpathResult, ProcessIdentity, WindowsProcess};
use std::time::{Duration, Instant};
use windows_sys::Win32::{Security::TokenElevationTypeFull, System::Threading::GetCurrentProcess};

pub(super) struct CoreChannel {
    pipe: Pipe,
    handshake: Handshake,
    environment: String,
}
impl CoreChannel {
    pub fn create(
        prepared: &PreparedSession,
        helper: &ProcessIdentity,
        firewall_needed: bool,
    ) -> LanpathResult<Self> {
        let nonce = uuid::Uuid::parse_str(&prepared.session)
            .map_err(|_| failure("invalid managed session nonce"))?;
        let name = format!(r"\\.\pipe\LanpathGPUI-Core-{}", prepared.session);
        // Only elevated Administrators/SYSTEM can connect. This is not the GUI pipe.
        let pipe = Pipe::server(&name, "BA")?;
        let environment = serde_json::to_string(&serde_json::json!({
            "version": managed_protocol::VERSION, "pipe": name, "nonce": nonce,
            "interface": prepared.interface_name, "adapter_guid": prepared.adapter_guid,
            "helper_pid": helper.pid, "helper_created": helper.created,
        }))?;
        Ok(Self {
            pipe,
            handshake: Handshake::new(nonce, prepared.adapter_guid, firewall_needed)?,
            environment,
        })
    }
    pub fn environment(&self) -> &str {
        &self.environment
    }
    pub fn authenticate(&mut self, launched: &WindowsProcess) -> LanpathResult<()> {
        self.pipe
            .connect_bounded(launched, Duration::from_secs(15))?;
        let expected = launched.identity()?;
        let peer = security::process_for_pipe(self.pipe.peer_pid(false)?, &expected)?;
        let own = security::token_identity(unsafe { GetCurrentProcess() })?;
        let token = security::token_identity(peer.handle())?;
        if own.elevation != TokenElevationTypeFull || token != own {
            return Err(failure(
                "managed core does not share the elevated helper user/logon/session/token boundary",
            ));
        }
        self.pipe.bind_peer(peer);
        self.pipe
            .bound_reads_until(Instant::now() + Duration::from_secs(40));
        Ok(())
    }
    pub fn receive(&mut self) -> LanpathResult<Event> {
        self.handshake.receive(&mut self.pipe)
    }
    pub fn acknowledge(&mut self) -> LanpathResult<()> {
        self.handshake.acknowledge(&mut self.pipe)
    }
    pub fn complete(&self) -> bool {
        self.handshake.complete()
    }
}
