use super::model::*;
use super::session::{OperationGuard, SessionBackend};
use lanpath_common::LanpathResult;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Mutex;
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

mod protection;
mod registry;
mod wininet;

pub(crate) struct WindowsBackend {
    root: PathBuf,
    sid: String,
    lease: Mutex<Option<File>>,
}

impl WindowsBackend {
    pub fn new() -> LanpathResult<Self> {
        let sid = protection::current_sid()?;
        let root = protection::directory(&sid)?;
        Ok(Self {
            root,
            sid,
            lease: Mutex::new(None),
        })
    }

    fn lock_file(&self, name: &str) -> LanpathResult<File> {
        let path = self.root.join(name);
        if path.exists() {
            protection::protect(&path, &self.sid)?;
        }
        // Share mode zero is process/thread independent, cross-root and cross-logon-
        // session for this user; the OS releases it on process death. Never PID heuristics.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .custom_flags(0x0020_0000 | 0x8000_0000)
            .open(&path)
            .map_err(|e| {
                if matches!(e.raw_os_error(), Some(32 | 33)) {
                    return e.into();
                }
                error(format!(
                    "another system-proxy owner/operation is active or lock unavailable: {e}"
                ))
            })?;
        // Children inherit the directory's protected DACL. Validate owner/reparse points
        // before opening; fresh files inherit that DACL without a sharing-violating reopen.
        Ok(file)
    }

    fn journal(&self) -> PathBuf {
        self.root.join("recovery.dpapi")
    }
}

impl SessionBackend for WindowsBackend {
    fn coordinate(&self) -> LanpathResult<Box<dyn OperationGuard>> {
        Ok(Box::new(super::session::coordination::wait(
            || self.lock_file("operation.lock"),
            std::time::Duration::from_secs(30),
        )?))
    }

    fn claim(&self) -> LanpathResult<()> {
        let mut lease = self
            .lease
            .lock()
            .map_err(|_| error("lease lock poisoned"))?;
        if lease.is_none() {
            *lease = Some(self.lock_file("owner.lock")?);
        }
        Ok(())
    }

    fn release(&self) {
        if let Ok(mut lease) = self.lease.lock() {
            *lease = None;
        }
    }

    fn snapshot(&self) -> LanpathResult<Snapshot> {
        let first = Snapshot {
            effective: wininet::query()?,
            values: registry::values()?,
            guards: registry::guards()?,
        };
        let second = Snapshot {
            effective: wininet::query()?,
            values: registry::values()?,
            guards: registry::guards()?,
        };
        if first != second {
            return Err(error(
                "settings changed while taking snapshot; no stable authority",
            ));
        }
        Ok(second)
    }

    fn apply(&self, effective: &EffectiveProxy) -> LanpathResult<()> {
        wininet::apply(effective)
    }
    fn restore_values(&self, original: &Snapshot) -> LanpathResult<()> {
        registry::restore(original)
    }
    fn notify(&self) -> LanpathResult<()> {
        wininet::notify()
    }
    fn validate(&self, snapshot: &Snapshot, policy: AcquisitionPolicy) -> LanpathResult<()> {
        registry::validate(snapshot, policy)
    }

    fn load(&self) -> LanpathResult<Option<RecoveryIntent>> {
        let path = self.journal();
        match std::fs::symlink_metadata(&path) {
            Ok(_) => protection::protect(&path, &self.sid)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let file = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .custom_flags(0x0020_0000)
            .open(path)?;
        if file.metadata()?.len() > 8 * 1024 * 1024 {
            return Err(error("recovery journal too large"));
        }
        let mut bytes = Vec::new();
        file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        let plaintext = protection::crypt(&bytes, false)?;
        let intent: RecoveryIntent = serde_json::from_slice(&plaintext)
            .map_err(|_| error("invalid authenticated recovery journal; evidence retained"))?;
        if intent.version != 1
            || intent.original.values.len() != 5
            || intent.original.guards.len() != 24
        {
            return Err(error("unsupported recovery journal; evidence retained"));
        }
        Ok(Some(intent))
    }

    fn save(&self, intent: &RecoveryIntent) -> LanpathResult<()> {
        let plaintext = serde_json::to_vec(intent)?;
        let encrypted = protection::crypt(&plaintext, true)?;
        let temp = self
            .root
            .join(format!("recovery-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .share_mode(0)
                .custom_flags(0x0020_0000 | 0x8000_0000)
                .open(&temp)?;
            file.write_all(&encrypted)?;
            file.sync_all()?;
            drop(file);
            protection::protect(&temp, &self.sid)?;
            if unsafe {
                MoveFileExW(
                    protection::path_wide(&temp).as_ptr(),
                    protection::path_wide(&self.journal()).as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(error(format!(
                    "durable journal replacement failed: {}",
                    std::io::Error::last_os_error()
                )));
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result
    }

    fn clear(&self) -> LanpathResult<()> {
        let path = self.journal();
        protection::protect(&path, &self.sid)?;
        std::fs::remove_file(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanpath_common::LanpathError;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn isolated_operation_file_serializes_contention_but_owner_lease_never_waits() {
        // Never use WindowsBackend::new(): this test must not touch the product's
        // global directory, journal, WinINet settings or real ownership lease.
        let root = tempfile::tempdir().unwrap();
        let sid = protection::current_sid().unwrap();
        let first = WindowsBackend {
            root: root.path().to_owned(),
            sid: sid.clone(),
            lease: Mutex::new(None),
        };
        let second = WindowsBackend {
            root: root.path().to_owned(),
            sid,
            lease: Mutex::new(None),
        };
        let operation = first.coordinate().unwrap();
        let (blocked_tx, blocked_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut first_attempt = true;
            let lock = super::super::session::coordination::wait(|| {
                let result = second.lock_file("operation.lock");
                if first_attempt {
                    assert!(matches!(&result, Err(LanpathError::Io(error)) if matches!(error.raw_os_error(), Some(32 | 33))));
                    first_attempt = false;
                    blocked_tx.send(()).unwrap();
                }
                result
            }, Duration::from_secs(10)).unwrap();
            drop(lock);
            second
        });
        blocked_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(operation);
        let second = worker.join().unwrap();
        first.claim().unwrap();
        assert!(second.claim().is_err());
        first.release();
        second.claim().unwrap();
        second.release();
    }
}
