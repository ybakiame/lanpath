use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub(crate) const DIRECT: u32 = 1;
pub(crate) const PROXY: u32 = 2;

/// Default acquisition never replaces somebody else's active manual proxy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AcquisitionPolicy {
    #[default]
    RequireDirect,
    /// Explicit consent to replace a supported manual-only baseline and restore it later.
    ExplicitTakeover,
}

/// Ownership is independent from the user's desired on/off setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemProxyOwnership {
    Unowned,
    Owned,
    /// An interrupted operation, external edit, or failed notification needs attention.
    RecoveryRequired,
}

/// Effective WinINet default-LAN configuration (not WinHTTP or per-RAS settings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EffectiveProxy {
    pub flags: u32,
    pub server: String,
    pub bypass: String,
    pub pac_url: String,
}

impl EffectiveProxy {
    pub fn target(addr: SocketAddr) -> Self {
        Self {
            flags: DIRECT | PROXY,
            server: format!("http={addr};https={addr}"),
            bypass: String::new(),
            pac_url: String::new(),
        }
    }
}

/// Preserve absence and exact registry type/data, including connection blobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TypedValue {
    pub kind: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub effective: EffectiveProxy,
    pub values: Vec<Option<TypedValue>>,
    /// Any supported policy/scope value changing invalidates restoration authority.
    pub guards: Vec<Option<TypedValue>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RecoveryIntent {
    pub version: u32,
    pub owner: String,
    pub owner_label: String,
    pub original: Snapshot,
    pub target: EffectiveProxy,
    pub observed: Option<Snapshot>,
    pub restoring: bool,
}

pub(crate) fn error(message: impl Into<String>) -> lanpath_common::LanpathError {
    lanpath_common::LanpathError::Client(format!("system_proxy: {}", message.into()))
}
