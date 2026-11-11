//! Narrow, product-bound Windows TUN helper. Dispatch before constructing GUI or stores.
mod config;
mod input_paths;
#[cfg(any(windows, test))]
mod managed_protocol;
mod protocol;
pub use config::snapshot_resources;
pub(crate) use input_paths::input_paths;
pub use protocol::{BrokerStatus, SessionState, StopKind, StopReceipt};
#[cfg(not(windows))]
use protocol::{PreparedSession, Resource};
#[cfg(windows)]
mod broker_staging;
#[cfg(windows)]
mod core_cache;
#[cfg(windows)]
mod core_channel;
#[cfg(windows)]
mod file_operation;
#[cfg(windows)]
mod firewall;
#[cfg(windows)]
mod helper;
#[cfg(windows)]
mod ipc;
#[cfg(windows)]
mod journal;
#[cfg(windows)]
mod launch;
#[cfg(windows)]
mod network;
#[cfg(windows)]
mod security;
#[cfg(windows)]
mod staging;
#[cfg(windows)]
mod staging_lock;
#[cfg(windows)]
pub(crate) use ipc::Pipe;
#[cfg(windows)]
pub use launch::Broker;

pub fn dispatch_from_env() -> lanpath_common::LanpathResult<bool> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !args.first().is_some_and(|s| s == "--privileged-helper") {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        launch::dispatch(args)?;
        Ok(true)
    }
    #[cfg(not(windows))]
    {
        Err(protocol::failure(
            "the privileged TUN helper is supported only on Windows",
        ))
    }
}

#[cfg(not(windows))]
pub struct Broker;
#[cfg(not(windows))]
impl Broker {
    pub fn launch(_: uuid::Uuid) -> lanpath_common::LanpathResult<Self> {
        Err(protocol::failure(
            "Windows TUN elevation is unavailable on this platform",
        ))
    }
    pub fn prepare(
        &mut self,
        _: serde_json::Value,
        _: Vec<Resource>,
        _: String,
    ) -> lanpath_common::LanpathResult<PreparedSession> {
        Err(protocol::failure(
            "Windows TUN elevation is unavailable on this platform",
        ))
    }
    pub fn start(&mut self) -> lanpath_common::LanpathResult<BrokerStatus> {
        self.status()
    }
    pub fn status(&mut self) -> lanpath_common::LanpathResult<BrokerStatus> {
        Err(protocol::failure(
            "Windows TUN elevation is unavailable on this platform",
        ))
    }
    pub fn stop(&mut self) -> lanpath_common::LanpathResult<StopReceipt> {
        Err(protocol::failure(
            "Windows TUN elevation is unavailable on this platform",
        ))
    }
    pub fn recover(&mut self) -> lanpath_common::LanpathResult<StopReceipt> {
        self.stop()
    }
}

#[cfg(test)]
mod tests;
