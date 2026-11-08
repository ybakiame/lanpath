use super::model::*;
use lanpath_common::LanpathResult;
use std::net::SocketAddr;
use std::sync::Mutex;

#[cfg(any(windows, test))]
pub(crate) mod coordination;
mod disabled;

pub(crate) trait OperationGuard {}
impl<T> OperationGuard for T {}

/// Backend methods must preserve errors; writes are not a multi-value compare-and-swap.
pub(crate) trait SessionBackend: Send + Sync {
    fn coordinate(&self) -> LanpathResult<Box<dyn OperationGuard>>;
    fn claim(&self) -> LanpathResult<()>;
    fn release(&self);
    fn snapshot(&self) -> LanpathResult<Snapshot>;
    fn apply(&self, effective: &EffectiveProxy) -> LanpathResult<()>;
    fn restore_values(&self, original: &Snapshot) -> LanpathResult<()>;
    fn notify(&self) -> LanpathResult<()>;
    fn load(&self) -> LanpathResult<Option<RecoveryIntent>>;
    fn save(&self, intent: &RecoveryIntent) -> LanpathResult<()>;
    fn clear(&self) -> LanpathResult<()>;
    fn validate(&self, snapshot: &Snapshot, policy: AcquisitionPolicy) -> LanpathResult<()>;
}

struct State {
    intent: Option<RecoveryIntent>,
    healthy: bool,
}

/// Framework-independent, explicit WinINet ownership lifecycle.
///
/// No destructor mutates global settings. Keep the referenced core alive until `restore`
/// succeeds. Call `recover_abandoned` explicitly before starting/reaping an old core.
/// Recovery never overwrites unrecognised/external edits. A verified direct state may
/// finish a formerly direct session without changing the user's disabled settings.
pub struct SystemProxySession {
    backend: Box<dyn SessionBackend>,
    state: Mutex<State>,
    owner: String,
    label: String,
}

impl SystemProxySession {
    /// Uses a fixed, current-user-protected coordination directory, never an app data root.
    #[cfg(windows)]
    pub fn windows(owner_label: &str) -> LanpathResult<Self> {
        Ok(Self::new(
            Box::new(super::windows::WindowsBackend::new()?),
            owner_label,
        ))
    }

    /// The independent guardian uses the GUI's opaque identity while being the
    /// sole owner of the lease, mutations, readbacks and durable checkpoints.
    #[cfg(windows)]
    pub fn windows_with_owner(owner_label: &str, owner: &str) -> LanpathResult<Self> {
        let parsed = uuid::Uuid::parse_str(owner).map_err(|_| error("invalid owner identity"))?;
        if parsed.is_nil() || parsed.to_string() != owner {
            return Err(error("invalid owner identity"));
        }
        let mut session = Self::windows(owner_label)?;
        session.owner = owner.to_owned();
        Ok(session)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn new(backend: Box<dyn SessionBackend>, label: &str) -> Self {
        Self {
            backend,
            state: Mutex::new(State {
                intent: None,
                healthy: false,
            }),
            owner: uuid::Uuid::new_v4().to_string(),
            label: label.to_owned(),
        }
    }

    pub fn status(&self) -> LanpathResult<SystemProxyOwnership> {
        let state = self
            .state
            .lock()
            .map_err(|_| error("session lock poisoned"))?;
        let _guard = self.backend.coordinate()?;
        Ok(match state.intent.as_ref() {
            None if self.backend.load()?.is_some() => SystemProxyOwnership::RecoveryRequired,
            None => SystemProxyOwnership::Unowned,
            Some(intent)
                if state.healthy && intent.observed.as_ref() == Some(&self.backend.snapshot()?) =>
            {
                SystemProxyOwnership::Owned
            }
            Some(_) => SystemProxyOwnership::RecoveryRequired,
        })
    }

    /// A read error is never converted into a false/unowned result.
    pub fn effective_enabled(&self) -> LanpathResult<bool> {
        let _guard = self.backend.coordinate()?;
        Ok(self.backend.snapshot()?.effective.flags & PROXY != 0)
    }

    /// The opaque journal identity, not a PID or app-data path.
    pub fn owner_id(&self) -> &str {
        &self.owner
    }

    pub fn acquire(&self, addr: SocketAddr) -> LanpathResult<()> {
        self.acquire_with_policy(addr, AcquisitionPolicy::RequireDirect)
    }

    pub fn acquire_with_policy(
        &self,
        addr: SocketAddr,
        policy: AcquisitionPolicy,
    ) -> LanpathResult<()> {
        if !addr.ip().is_loopback() || addr.port() == 0 {
            return Err(error(
                "the owned proxy must reference a nonzero loopback core endpoint",
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| error("session lock poisoned"))?;
        let _guard = self.backend.coordinate()?;
        if state.intent.is_some() {
            return Err(error(
                "restore the existing session before acquiring another endpoint",
            ));
        }
        self.backend.claim()?;
        let original = match (|| {
            if self.backend.load()?.is_some() {
                return Err(error(
                    "an existing recovery journal requires explicit abandoned recovery",
                ));
            }
            let original = self.backend.snapshot()?;
            self.backend.validate(&original, policy)?;
            Ok(original)
        })() {
            Ok(snapshot) => snapshot,
            Err(e) => {
                self.backend.release();
                return Err(e);
            }
        };
        let intent = RecoveryIntent {
            version: 1,
            owner: self.owner.clone(),
            owner_label: self.label.clone(),
            original,
            target: EffectiveProxy::target(addr),
            observed: None,
            restoring: false,
        };
        if let Err(e) = self.backend.save(&intent) {
            self.backend.release();
            return Err(e);
        }
        state.intent = Some(intent);
        state.healthy = false;
        // Recheck after durable intent, immediately before mutation. This narrows, but does
        // not eliminate, races with non-cooperating writers (WinINet has no CAS API).
        let before = self.backend.snapshot()?;
        let intent = state
            .intent
            .as_ref()
            .ok_or_else(|| error("missing recovery intent"))?;
        if before != intent.original {
            return Err(error(
                "baseline changed before apply; recovery evidence retained",
            ));
        }
        let result = self.backend.apply(&intent.target);
        let observed = self.backend.snapshot()?;
        let intent = state
            .intent
            .as_mut()
            .ok_or_else(|| error("missing recovery intent"))?;
        // Record any partial result while we still own the exclusive cooperative lease.
        // Unknown external edits are never granted restoration authority.
        if observed.guards == intent.original.guards
            && (observed.effective == intent.target || observed == intent.original)
        {
            intent.observed = Some(observed.clone());
            self.backend.save(intent)?;
        }
        result?;
        if observed.effective != intent.target || observed.guards != intent.original.guards {
            return Err(error(
                "apply readback differs from requested default-LAN settings; core must remain running",
            ));
        }
        if intent.observed.is_none() {
            return Err(error("apply state is unknown; recovery evidence retained"));
        }
        self.backend.notify()?;
        if self.backend.snapshot()? != observed {
            return Err(error(
                "settings changed during apply notification; core must remain running",
            ));
        }
        state.healthy = true;
        Ok(())
    }

    /// Without an owned session this performs no registry, WinINet or journal write.
    /// Restore the exact baseline, or release a verified manually disabled session
    /// without overwriting inactive addresses, bypasses or connection blobs.
    pub fn restore(&self) -> LanpathResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| error("session lock poisoned"))?;
        if state.intent.is_none() {
            return Ok(());
        }
        state.healthy = false;
        let _guard = self.backend.coordinate()?;
        let intent = state
            .intent
            .as_mut()
            .ok_or_else(|| error("missing recovery intent"))?;
        self.restore_intent(intent)?;
        state.intent = None;
        self.backend.release();
        Ok(())
    }

    /// Explicit recovery only succeeds after acquiring the crashed owner's lease.
    /// Active owners (including other roots/logon sessions) cannot be displaced.
    pub fn recover_abandoned(&self) -> LanpathResult<bool> {
        self.recover_abandoned_matching(None)
    }

    /// Recover only this watched session's authenticated journal. A missing or successor
    /// journal is not authority to write, even after the watched process has exited.
    pub fn recover_abandoned_owner(&self, expected_owner: &str) -> LanpathResult<bool> {
        self.recover_abandoned_matching(Some(expected_owner))
    }

    /// Read-only normal-shutdown check; a successor's evidence must remain untouched.
    pub fn has_recovery_for_owner(&self, expected_owner: &str) -> LanpathResult<bool> {
        let _guard = self.backend.coordinate()?;
        Ok(self
            .backend
            .load()?
            .is_some_and(|intent| intent.owner == expected_owner))
    }

    fn recover_abandoned_matching(&self, expected_owner: Option<&str>) -> LanpathResult<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| error("session lock poisoned"))?;
        if state.intent.is_some() {
            return Err(error("this session already owns recovery state"));
        }
        let _guard = self.backend.coordinate()?;
        self.backend.claim()?;
        let loaded = self.backend.load();
        let mut intent = match loaded {
            Ok(Some(intent)) => intent,
            Ok(None) => {
                self.backend.release();
                return Ok(false);
            }
            Err(e) => {
                self.backend.release();
                return Err(e);
            }
        };
        if intent.version != 1 {
            self.backend.release();
            return Err(error("unsupported recovery journal version"));
        }
        if expected_owner.is_some_and(|owner| owner != intent.owner) {
            self.backend.release();
            return Ok(false);
        }
        // Adopt evidence before attempting any writes so failure remains actionable.
        state.intent = Some(intent.clone());
        let result = self.restore_intent(&mut intent);
        state.intent = Some(intent);
        result?;
        state.intent = None;
        self.backend.release();
        Ok(true)
    }

    fn restore_intent(&self, intent: &mut RecoveryIntent) -> LanpathResult<()> {
        let current = self.backend.snapshot()?;
        if current.guards != intent.original.guards {
            return Err(error("policy/PAC/scope changed; refusing restoration"));
        }
        // Returning to the original effective state is sufficient even when Windows
        // rewrote connection blobs. An active successor still requires an exact match.
        if current.effective != intent.original.effective
            && intent.observed.as_ref() != Some(&current)
        {
            if disabled::verified_direct(intent, &current) {
                self.backend
                    .validate(&current, AcquisitionPolicy::RequireDirect)?;
                // The exception grants no registry write authority. Check the exact
                // durable owner/checkpoint and reread before retiring only its journal.
                if self.backend.load()?.as_ref() != Some(&*intent)
                    || self.backend.snapshot()? != current
                {
                    return Err(error(
                        "disabled proxy state or recovery owner changed; evidence retained",
                    ));
                }
                self.backend.clear()?;
                tracing::info!(
                    "system proxy already disabled; released owned recovery state without modifying settings"
                );
                return Ok(());
            }
            return Err(error(
                "external change or interrupted unknown apply; refusing restoration",
            ));
        }
        intent.restoring = true;
        self.backend.save(intent)?;
        if current != intent.original {
            let applied = self.backend.apply(&intent.original.effective);
            let intermediate = self.backend.snapshot()?;
            // Only a recognised original/owned effective configuration can grant
            // restoration authority. Persist this stage so notification/type failures
            // are retryable without accepting arbitrary external edits.
            if intermediate.guards == intent.original.guards
                && intermediate.effective == intent.original.effective
            {
                intent.observed = Some(intermediate.clone());
                self.backend.save(intent)?;
            }
            applied?;
            if intermediate.effective != intent.original.effective {
                return Err(error(
                    "restoration apply readback differs; ownership retained",
                ));
            }
            let restored = self.backend.restore_values(&intent.original);
            let after = self.backend.snapshot()?;
            if after.guards == intent.original.guards
                && after.effective == intent.original.effective
                && after.values.len() == intermediate.values.len()
                && after
                    .values
                    .iter()
                    .zip(&intermediate.values)
                    .zip(&intent.original.values)
                    .all(|((value, owned), original)| value == owned || value == original)
            {
                intent.observed = Some(after);
                self.backend.save(intent)?;
            }
            restored?;
        }
        if self.backend.snapshot()? != intent.original {
            return Err(error(
                "restoration readback failed; ownership and core retained",
            ));
        }
        self.backend.notify()?;
        if self.backend.snapshot()? != intent.original {
            return Err(error(
                "restoration notification changed settings; ownership retained",
            ));
        }
        self.backend.clear()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
