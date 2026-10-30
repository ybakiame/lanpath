use super::{
    protocol::{InterfaceObservation, PreparedSession, SessionState, failure},
    security, staging,
};
use lanpath_common::{LanpathResult, ProcessIdentity};
use serde::{Deserialize, Serialize};
use std::{
    io,
    os::windows::io::OwnedHandle,
    path::{Path, PathBuf},
};
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*, System::Threading::*};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub schema: u32,
    pub owner_sid: String,
    pub root_id: uuid::Uuid,
    pub prepared: PreparedSession,
    pub state: SessionState,
    pub parent: ProcessIdentity,
    pub helper: ProcessIdentity,
    pub core: Option<ProcessIdentity>,
    pub core_sha256: String,
    pub managed_source_digest: String,
    pub cache_enabled: bool,
    pub creation_authorized: bool,
    pub adapter: Option<super::protocol::AdapterReceipt>,
    pub firewall_needed: bool,
    pub firewall: Option<super::firewall::Snapshot>,
    pub ready: bool,
    pub observed: Vec<InterfaceObservation>,
    pub forced: bool,
    pub detail: String,
    pub uncertain: bool,
}
pub(super) struct Journal {
    root: PathBuf,
    path: PathBuf,
    _machine_lock: OwnedHandle,
}
impl Journal {
    pub fn acquire() -> LanpathResult<Self> {
        let root = security::machine_root()?;
        let descriptor =
            crate::windows_access::Descriptor::new("O:BAG:BAD:P(A;;GA;;;SY)(A;;GA;;;BA)")?;
        let handle = unsafe {
            CreateMutexExW(
                &descriptor.attributes(),
                security::wide(r"Global\com.ybakiame.lanpath.gpui.Tun").as_ptr(),
                CREATE_MUTEX_INITIAL_OWNER,
                SYNCHRONIZE | MUTEX_MODIFY_STATE,
            )
        };
        let error = unsafe { GetLastError() };
        let machine_lock = security::owned(handle)?;
        if error == ERROR_ALREADY_EXISTS {
            return Err(failure(
                "a machine TUN helper or conflicting mutex already exists; no network changes made",
            ));
        }
        let journals = root.join("journals");
        security::protected_directory(&journals)?;
        Ok(Self {
            path: journals.join("tun.json"),
            root,
            _machine_lock: machine_lock,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn load(&self) -> LanpathResult<Option<Record>> {
        let bytes = match staging::read_bounded(&self.path, 2 * 1024 * 1024) {
            Ok(bytes) => bytes,
            Err(lanpath_common::LanpathError::Io(error))
                if error.kind() == io::ErrorKind::NotFound =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let record: Record = serde_json::from_slice(&bytes)?;
        if record.schema != 3
            || record.managed_source_digest != crate::product::CORE_MANAGED_SOURCE_DIGEST
            || record.root_id.is_nil()
            || record.prepared.adapter_guid.is_nil()
            || uuid::Uuid::parse_str(&record.prepared.session).is_err()
            || record.prepared.interface_name != interface_name(&record.prepared.session)?
        {
            return Err(failure("invalid machine TUN journal; record retained"));
        }
        if record.adapter.as_ref().is_some_and(|adapter| {
            !record.creation_authorized
                || record.core.is_none()
                || adapter.guid != record.prepared.adapter_guid
                || adapter.luid == 0
                || adapter.index == 0
        }) || record.firewall.is_some() && (!record.firewall_needed || record.adapter.is_none())
            || record.ready
                && (record.adapter.is_none() || record.firewall_needed && record.firewall.is_none())
        {
            return Err(failure(
                "inconsistent managed-core effect journal; record retained",
            ));
        }
        if let Some(core) = &record.core {
            let expected = self
                .root
                .join("sessions")
                .join(&record.prepared.session)
                .join("sing-box.exe");
            if core.exe != expected {
                return Err(failure(
                    "machine journal core image is outside protected session staging",
                ));
            }
        }
        Ok(Some(record))
    }
    pub fn save(&self, record: &Record) -> LanpathResult<()> {
        let directory = self
            .path
            .parent()
            .ok_or_else(|| failure("invalid machine journal path"))?;
        let temporary = directory.join(format!("tun-{}.json", uuid::Uuid::new_v4()));
        staging::write_new(&temporary, &serde_json::to_vec(record)?)?;
        if unsafe {
            MoveFileExW(
                security::wide(&temporary).as_ptr(),
                security::wide(&self.path).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }
    pub fn clear(&self) -> LanpathResult<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
pub(super) fn interface_name(session: &str) -> LanpathResult<String> {
    let id = uuid::Uuid::parse_str(session).map_err(|_| failure("invalid TUN session ID"))?;
    Ok(format!("LP-GPUI-{}", id.simple()))
}
