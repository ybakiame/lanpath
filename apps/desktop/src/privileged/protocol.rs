use lanpath_common::{LanpathError, LanpathResult, ProcessIdentity};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Write};

pub const VERSION: u32 = 3;
pub const MAX_FRAME: usize = 48 * 1024 * 1024;
pub const MAX_CONFIG: usize = 2 * 1024 * 1024;
pub const MAX_RESOURCES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedSession {
    pub session: String,
    pub interface_name: String,
    pub adapter_guid: uuid::Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Idle,
    Prepared,
    Running,
    RecoveryRequired,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceObservation {
    pub name: String,
    pub guid: String,
    pub luid: u64,
    pub index: u32,
    pub addresses: Vec<String>,
    pub routes: Vec<String>,
    pub dns_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdapterReceipt {
    pub guid: uuid::Uuid,
    pub luid: u64,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerStatus {
    pub state: SessionState,
    pub prepared: Option<PreparedSession>,
    pub core: Option<ProcessIdentity>,
    pub observed: Vec<InterfaceObservation>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopKind {
    Graceful,
    Forced,
    AlreadyStopped,
    RecoveryRequired,
}

#[cfg(any(windows, test))]
impl StopKind {
    pub(super) fn completed(
        unresolved: bool,
        forced: bool,
        had_effects: bool,
        graceful: bool,
    ) -> Self {
        if unresolved {
            Self::RecoveryRequired
        } else if forced {
            Self::Forced
        } else if had_effects && graceful {
            Self::Graceful
        } else {
            Self::AlreadyStopped
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopReceipt {
    pub kind: StopKind,
    pub process_exited: bool,
    /// Requires a durable managed-core creation receipt and independent cleanup observations.
    pub precise_cleanup_proven: bool,
    pub observed_remaining: Vec<InterfaceObservation>,
    pub detail: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Frame<T> {
    pub version: u32,
    pub sequence: u64,
    pub body: T,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", deny_unknown_fields)]
pub(super) enum Request {
    Prepare {
        config: Value,
        resources: Vec<Resource>,
        selected_core_sha256: String,
    },
    Start,
    Stop,
    Status,
    Recover,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "result", content = "payload", deny_unknown_fields)]
pub(super) enum Response {
    Prepared(PreparedSession),
    Status(BrokerStatus),
    Stopped(StopReceipt),
    Error(String),
}

pub(super) fn failure(message: impl Into<String>) -> LanpathError {
    LanpathError::Core(message.into())
}

pub(super) fn write_frame<T: Serialize>(
    writer: &mut impl Write,
    sequence: u64,
    body: T,
) -> LanpathResult<()> {
    let bytes = serde_json::to_vec(&Frame {
        version: VERSION,
        sequence,
        body,
    })?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(failure("privileged frame exceeds bound"));
    }
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

pub(super) fn read_frame<T: for<'de> Deserialize<'de>>(
    reader: &mut impl Read,
    sequence: u64,
) -> LanpathResult<T> {
    let mut size = [0; 4];
    reader.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err(failure("invalid privileged frame length"));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes)?;
    let frame: Frame<T> = serde_json::from_slice(&bytes)?;
    if frame.version != VERSION || frame.sequence != sequence {
        return Err(failure("privileged protocol version/sequence mismatch"));
    }
    Ok(frame.body)
}
