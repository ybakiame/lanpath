use super::*;
use std::sync::Arc;

struct Pause {
    entered: std::sync::mpsc::Sender<()>,
    resume: std::sync::mpsc::Receiver<()>,
}

fn checkpoint_pause(pause: Option<Pause>) {
    if let Some(pause) = pause {
        pause.entered.send(()).unwrap();
        pause
            .resume
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
    }
}

#[derive(Clone, Copy, Default)]
enum ApplyFailure {
    #[default]
    None,
    AfterTarget,
    UnknownPartial,
}

struct Fixture {
    current: Snapshot,
    journal: Option<RecoveryIntent>,
    leased: bool,
    apply_failure: ApplyFailure,
    restore_failure: bool,
    notify_failure: bool,
    load_failure: bool,
    clear_failure: bool,
    load_pause: Option<Pause>,
    read_failure: bool,
    global_writes: usize,
    journal_writes: usize,
    apply_pause: Option<Pause>,
    typed_restore_pause: Option<Pause>,
}

fn baseline() -> Snapshot {
    Snapshot {
        effective: EffectiveProxy {
            flags: DIRECT,
            server: "old-inactive:81".into(),
            bypass: "<local>".into(),
            pac_url: String::new(),
        },
        values: vec![
            Some(TypedValue {
                kind: 4,
                bytes: 0u32.to_le_bytes().to_vec(),
            }),
            None,
            None,
            Some(TypedValue {
                kind: 3,
                bytes: vec![0; 12],
            }),
            None,
        ],
        guards: vec![None; 24],
    }
}

fn fixture() -> Arc<Mutex<Fixture>> {
    Arc::new(Mutex::new(Fixture {
        current: baseline(),
        journal: None,
        leased: false,
        apply_failure: ApplyFailure::None,
        restore_failure: false,
        notify_failure: false,
        load_failure: false,
        clear_failure: false,
        load_pause: None,
        read_failure: false,
        global_writes: 0,
        journal_writes: 0,
        apply_pause: None,
        typed_restore_pause: None,
    }))
}

impl SessionBackend for Arc<Mutex<Fixture>> {
    fn coordinate(&self) -> LanpathResult<Box<dyn OperationGuard>> {
        Ok(Box::new(()))
    }
    fn claim(&self) -> LanpathResult<()> {
        let mut fixture = self.lock().unwrap();
        if fixture.leased {
            return Err(error("active owner"));
        }
        fixture.leased = true;
        Ok(())
    }
    fn release(&self) {
        self.lock().unwrap().leased = false;
    }
    fn snapshot(&self) -> LanpathResult<Snapshot> {
        let fixture = self.lock().unwrap();
        if fixture.read_failure {
            return Err(error("read denied"));
        }
        Ok(fixture.current.clone())
    }
    fn apply(&self, effective: &EffectiveProxy) -> LanpathResult<()> {
        let mut fixture = self.lock().unwrap();
        fixture.global_writes += 1;
        fixture.current.effective = effective.clone();
        fixture.current.values[0] = Some(TypedValue {
            kind: 4,
            bytes: u32::from(effective.flags & PROXY != 0)
                .to_le_bytes()
                .to_vec(),
        });
        // WinINet also changes connection blobs; typed restoration must restore absence/data.
        fixture.current.values[3] = Some(TypedValue {
            kind: 3,
            bytes: vec![7; 12],
        });
        let result = match std::mem::take(&mut fixture.apply_failure) {
            ApplyFailure::None => Ok(()),
            ApplyFailure::AfterTarget => Err(error("apply returned failure after target mutation")),
            ApplyFailure::UnknownPartial => {
                fixture.current.effective.server = "unrecognised:99".into();
                Err(error("unknown partial apply"))
            }
        };
        let pause = fixture.apply_pause.take();
        drop(fixture);
        checkpoint_pause(pause);
        result
    }
    fn restore_values(&self, original: &Snapshot) -> LanpathResult<()> {
        let mut fixture = self.lock().unwrap();
        fixture.global_writes += 1;
        if fixture.restore_failure {
            fixture.current.values[0] = original.values[0].clone();
            return Err(error("typed restore failed partway"));
        }
        fixture.current.values = original.values.clone();
        let pause = fixture.typed_restore_pause.take();
        drop(fixture);
        checkpoint_pause(pause);
        Ok(())
    }
    fn notify(&self) -> LanpathResult<()> {
        let fixture = self.lock().unwrap();
        if fixture.notify_failure {
            Err(error("notification failed"))
        } else {
            Ok(())
        }
    }
    fn load(&self) -> LanpathResult<Option<RecoveryIntent>> {
        let mut fixture = self.lock().unwrap();
        if fixture.load_failure {
            return Err(error("journal inaccessible"));
        }
        let journal = fixture.journal.clone();
        let pause = fixture.load_pause.take();
        drop(fixture);
        checkpoint_pause(pause);
        Ok(journal)
    }
    fn save(&self, intent: &RecoveryIntent) -> LanpathResult<()> {
        let mut fixture = self.lock().unwrap();
        fixture.journal_writes += 1;
        fixture.journal = Some(intent.clone());
        Ok(())
    }
    fn clear(&self) -> LanpathResult<()> {
        let mut fixture = self.lock().unwrap();
        if fixture.clear_failure {
            return Err(error("journal clear failed"));
        }
        fixture.journal_writes += 1;
        fixture.journal = None;
        Ok(())
    }
    fn validate(&self, original: &Snapshot, policy: AcquisitionPolicy) -> LanpathResult<()> {
        if policy == AcquisitionPolicy::RequireDirect && original.effective.flags & PROXY != 0 {
            Err(error("foreign manual proxy"))
        } else {
            Ok(())
        }
    }
}

fn endpoint() -> SocketAddr {
    "127.0.0.1:18080".parse().unwrap()
}

fn manually_disable_owned_proxy(fixture: &Arc<Mutex<Fixture>>) {
    let mut state = fixture.lock().unwrap();
    state.current.effective.flags = DIRECT;
    state.current.values[0] = Some(TypedValue {
        kind: 4,
        bytes: 0u32.to_le_bytes().to_vec(),
    });
    // Windows leaves the owned server/bypass in place and rewrites connection blobs.
    state.current.values[3] = Some(TypedValue {
        kind: 3,
        bytes: vec![9; 12],
    });
}

#[test]
fn manually_disabled_owned_proxy_releases_ownership_without_rewriting_settings() {
    let fixture = fixture();
    let owner = session(&fixture);
    owner.acquire(endpoint()).unwrap();
    manually_disable_owned_proxy(&fixture);
    let before = fixture.lock().unwrap().current.clone();
    let writes = fixture.lock().unwrap().global_writes;
    assert_eq!(
        owner.status().unwrap(),
        SystemProxyOwnership::RecoveryRequired
    );
    owner.restore().unwrap();
    assert_eq!(owner.status().unwrap(), SystemProxyOwnership::Unowned);
    let state = fixture.lock().unwrap();
    assert_eq!(state.current, before);
    assert_eq!(state.global_writes, writes);
    assert!(state.journal.is_none());
    assert!(!state.leased);
}
fn session(fixture: &Arc<Mutex<Fixture>>) -> SystemProxySession {
    SystemProxySession::new(Box::new(fixture.clone()), "test-client")
}

#[test]
fn stop_without_session_never_writes_global_or_journal_state() {
    let fixture = fixture();
    let session = session(&fixture);
    session.restore().unwrap();
    let fixture = fixture.lock().unwrap();
    assert_eq!(fixture.global_writes, 0);
    assert_eq!(fixture.journal_writes, 0);
    assert!(!fixture.leased);
}

#[test]
fn failed_apply_with_recognised_readback_restores_exact_typed_baseline() {
    let fixture = fixture();
    fixture.lock().unwrap().apply_failure = ApplyFailure::AfterTarget;
    let session = session(&fixture);
    assert!(session.acquire(endpoint()).is_err());
    assert_eq!(
        session.status().unwrap(),
        SystemProxyOwnership::RecoveryRequired
    );
    assert!(fixture.lock().unwrap().journal.is_some());
    session.restore().unwrap();
    let fixture = fixture.lock().unwrap();
    assert_eq!(fixture.current, baseline());
    assert!(fixture.journal.is_none());
    assert!(!fixture.leased);
}

#[test]
fn unknown_partial_apply_preserves_evidence_and_refuses_destructive_restore() {
    let fixture = fixture();
    fixture.lock().unwrap().apply_failure = ApplyFailure::UnknownPartial;
    let session = session(&fixture);
    assert!(session.acquire(endpoint()).is_err());
    let before = fixture.lock().unwrap().global_writes;
    assert!(session.restore().is_err());
    let fixture = fixture.lock().unwrap();
    assert_eq!(fixture.global_writes, before);
    assert!(fixture.journal.is_some());
    assert!(fixture.leased);
}

#[test]
fn external_proxy_or_policy_edit_invalidates_restore_authority() {
    for policy_edit in [false, true] {
        let fixture = fixture();
        let session = session(&fixture);
        session.acquire(endpoint()).unwrap();
        {
            let mut fixture = fixture.lock().unwrap();
            if policy_edit {
                fixture.current.guards[2] = Some(TypedValue {
                    kind: 4,
                    bytes: vec![0; 4],
                });
            } else {
                fixture.current.effective.bypass = "external.example".into();
            }
        }
        assert_eq!(
            session.status().unwrap(),
            SystemProxyOwnership::RecoveryRequired
        );
        let before = fixture.lock().unwrap().global_writes;
        assert!(session.restore().is_err());
        let fixture = fixture.lock().unwrap();
        assert_eq!(fixture.global_writes, before);
        assert!(fixture.journal.is_some());
        assert!(fixture.leased);
    }
}

#[test]
fn partial_typed_restore_can_retry_without_losing_original_snapshot() {
    let fixture = fixture();
    let session = session(&fixture);
    session.acquire(endpoint()).unwrap();
    fixture.lock().unwrap().restore_failure = true;
    assert!(session.restore().is_err());
    assert_eq!(
        session.status().unwrap(),
        SystemProxyOwnership::RecoveryRequired
    );
    assert!(fixture.lock().unwrap().leased);
    fixture.lock().unwrap().restore_failure = false;
    session.restore().unwrap();
    assert_eq!(fixture.lock().unwrap().current, baseline());
    assert_eq!(session.status().unwrap(), SystemProxyOwnership::Unowned);
}

#[test]
fn failed_notification_retains_ownership_until_verified_retry() {
    let fixture = fixture();
    let session = session(&fixture);
    session.acquire(endpoint()).unwrap();
    fixture.lock().unwrap().notify_failure = true;
    assert!(session.restore().is_err());
    assert!(fixture.lock().unwrap().leased);
    assert!(fixture.lock().unwrap().journal.is_some());
    fixture.lock().unwrap().notify_failure = false;
    session.restore().unwrap();
    assert!(fixture.lock().unwrap().journal.is_none());
}

#[test]
fn abandoned_recovery_cannot_displace_active_owner_and_surfaces_journal_errors() {
    let fixture = fixture();
    let owner = session(&fixture);
    owner.acquire(endpoint()).unwrap();
    let replacement = session(&fixture);
    assert!(replacement.recover_abandoned().is_err());
    assert_eq!(owner.status().unwrap(), SystemProxyOwnership::Owned);
    // Simulate the OS releasing the crashed process's exclusive file handle.
    fixture.lock().unwrap().leased = false;
    fixture.lock().unwrap().load_failure = true;
    assert!(replacement.recover_abandoned().is_err());
    assert!(!fixture.lock().unwrap().leased);
    assert!(fixture.lock().unwrap().journal.is_some());
    fixture.lock().unwrap().load_failure = false;
    assert!(replacement.recover_abandoned().unwrap());
    assert_eq!(fixture.lock().unwrap().current, baseline());
    assert!(fixture.lock().unwrap().journal.is_none());
}

#[test]
fn abandoned_unknown_state_is_adopted_for_attention_not_overwritten() {
    let fixture = fixture();
    let owner = session(&fixture);
    owner.acquire(endpoint()).unwrap();
    {
        let mut fixture = fixture.lock().unwrap();
        fixture.leased = false;
        fixture.current.effective.server = "other-owner:88".into();
    }
    let replacement = session(&fixture);
    let before = fixture.lock().unwrap().global_writes;
    assert!(replacement.recover_abandoned().is_err());
    assert_eq!(
        replacement.status().unwrap(),
        SystemProxyOwnership::RecoveryRequired
    );
    assert_eq!(fixture.lock().unwrap().global_writes, before);
    assert!(fixture.lock().unwrap().leased);
}

#[test]
fn read_failure_is_unknown_not_disabled_or_unowned() {
    let fixture = fixture();
    let session = session(&fixture);
    session.acquire(endpoint()).unwrap();
    fixture.lock().unwrap().read_failure = true;
    assert!(session.effective_enabled().is_err());
    assert!(session.status().is_err());
    assert!(session.restore().is_err());
    assert!(fixture.lock().unwrap().journal.is_some());
}

#[test]
fn ready_guardian_recovers_only_its_exact_typed_journal() {
    let fixture = fixture();
    let owner = session(&fixture);
    owner.acquire(endpoint()).unwrap();
    let guardian = session(&fixture);
    assert!(guardian.has_recovery_for_owner(owner.owner_id()).unwrap());
    fixture.lock().unwrap().leased = false; // Guardian itself crashed; explicit fallback.
    assert!(guardian.recover_abandoned_owner(owner.owner_id()).unwrap());
    let state = fixture.lock().unwrap();
    assert_eq!(state.current, baseline());
    assert!(state.journal.is_none());
    assert!(!state.leased);
}

#[test]
fn delayed_guardian_preserves_active_and_abandoned_successor_ownership() {
    let fixture = fixture();
    let watched = session(&fixture);
    watched.acquire(endpoint()).unwrap();
    let watched_owner = watched.owner_id().to_owned();
    watched.restore().unwrap();
    let successor = session(&fixture);
    successor.acquire(endpoint()).unwrap();
    let guardian = session(&fixture);
    assert!(!guardian.has_recovery_for_owner(&watched_owner).unwrap());
    let before = {
        let state = fixture.lock().unwrap();
        (
            state.current.clone(),
            state.global_writes,
            state.journal_writes,
        )
    };
    assert!(guardian.recover_abandoned_owner(&watched_owner).is_err());
    fixture.lock().unwrap().leased = false;
    assert!(!guardian.recover_abandoned_owner(&watched_owner).unwrap());
    let state = fixture.lock().unwrap();
    assert_eq!(state.current, before.0);
    assert_eq!(state.global_writes, before.1);
    assert_eq!(state.journal_writes, before.2);
    assert_eq!(state.journal.as_ref().unwrap().owner, successor.owner_id());
    assert!(!state.leased);
}

#[test]
fn matching_guardian_refuses_external_effective_typed_and_policy_edits() {
    for edit in 0..3 {
        let fixture = fixture();
        let owner = session(&fixture);
        owner.acquire(endpoint()).unwrap();
        {
            let mut state = fixture.lock().unwrap();
            state.leased = false;
            match edit {
                0 => state.current.effective.bypass = "external.example".into(),
                1 => {
                    state.current.values[1] = Some(TypedValue {
                        kind: 1,
                        bytes: vec![42],
                    })
                }
                _ => {
                    state.current.guards[0] = Some(TypedValue {
                        kind: 4,
                        bytes: vec![1; 4],
                    })
                }
            }
        }
        let guardian = session(&fixture);
        let before = {
            let state = fixture.lock().unwrap();
            (
                state.current.clone(),
                state.global_writes,
                state.journal_writes,
            )
        };
        assert!(guardian.recover_abandoned_owner(owner.owner_id()).is_err());
        let state = fixture.lock().unwrap();
        assert_eq!(state.current, before.0);
        assert_eq!(state.global_writes, before.1);
        assert_eq!(state.journal_writes, before.2);
        assert_eq!(state.journal.as_ref().unwrap().owner, owner.owner_id());
    }
}

#[test]
fn guardian_accepts_manually_restored_effective_baseline_despite_connection_blob_changes() {
    let fixture = fixture();
    let owner = session(&fixture);
    owner.acquire(endpoint()).unwrap();
    {
        let mut state = fixture.lock().unwrap();
        state.leased = false;
        state.current.effective = baseline().effective;
        state.current.values[3] = Some(TypedValue {
            kind: 3,
            bytes: vec![9; 12],
        });
    }
    let guardian = session(&fixture);
    assert!(guardian.recover_abandoned_owner(owner.owner_id()).unwrap());
    let state = fixture.lock().unwrap();
    assert_eq!(state.current, baseline());
    assert!(state.journal.is_none());
    assert!(!state.leased);
}

#[test]
fn guardian_lifetime_finishes_mutation_checkpoints_after_gui_caller_disappears() {
    // These barriers stop the real session engine after a backend mutation and before
    // its readback/checkpoint. Only the GUI response receiver disappears; the owner
    // thread (the independent guardian) remains alive and must complete restoration.
    for boundary in 0..3 {
        let fixture = fixture();
        let guardian = session(&fixture);
        if boundary != 0 {
            guardian.acquire(endpoint()).unwrap();
            assert_eq!(guardian.status().unwrap(), SystemProxyOwnership::Owned);
        }
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let pause = Pause {
            entered: entered_tx,
            resume: resume_rx,
        };
        {
            let mut state = fixture.lock().unwrap();
            if boundary == 2 {
                state.typed_restore_pause = Some(pause);
            } else {
                state.apply_pause = Some(pause);
            }
        }
        let (response_tx, gui_response) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            if boundary == 0 {
                guardian.acquire(endpoint()).unwrap();
            } else {
                guardian.restore().unwrap();
            }
            // Models the service's failed response-write path, after the complete
            // synchronous mutation/readback/durable-checkpoint operation.
            assert!(response_tx.send(()).is_err());
            guardian.restore().unwrap();
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        {
            let state = fixture.lock().unwrap();
            assert!(state.leased);
            let journal = state.journal.as_ref().unwrap();
            if boundary == 0 {
                assert!(journal.observed.is_none());
                assert_ne!(state.current, baseline());
            } else if boundary == 1 {
                assert_ne!(journal.observed.as_ref(), Some(&state.current));
            } else {
                assert_eq!(state.current, baseline());
                assert_ne!(journal.observed.as_ref(), Some(&state.current));
            }
        }
        drop(gui_response); // GUI death cannot terminate the guardian's owner thread.
        resume_tx.send(()).unwrap();
        worker.join().unwrap();
        let state = fixture.lock().unwrap();
        assert_eq!(state.current, baseline());
        assert!(state.journal.is_none());
        assert!(!state.leased);
    }
}

#[test]
fn disabled_state_preserves_windows_rewritten_inactive_fields_and_allows_a_new_session() {
    let fixture = fixture();
    let owner = session(&fixture);
    owner.acquire(endpoint()).unwrap();
    manually_disable_owned_proxy(&fixture);
    {
        let mut state = fixture.lock().unwrap();
        state.current.effective.server.clear();
        state.current.effective.bypass = "<local>".into();
        state.current.values[1] = Some(TypedValue {
            kind: 1,
            bytes: vec![0, 0],
        });
        state.current.values[2] = Some(TypedValue {
            kind: 1,
            bytes: "<local>\0"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect(),
        });
    }
    let before = fixture.lock().unwrap().current.clone();
    let writes = fixture.lock().unwrap().global_writes;
    owner.restore().unwrap();
    assert_eq!(fixture.lock().unwrap().current, before);
    assert_eq!(fixture.lock().unwrap().global_writes, writes);
    let next = session(&fixture);
    next.acquire(endpoint()).unwrap();
    next.restore().unwrap();
    assert_eq!(fixture.lock().unwrap().current, before);
}

#[test]
fn abandoned_disabled_session_releases_only_the_matching_owner() {
    let fixture = fixture();
    let owner = session(&fixture);
    owner.acquire(endpoint()).unwrap();
    manually_disable_owned_proxy(&fixture);
    fixture.lock().unwrap().leased = false;
    let before = fixture.lock().unwrap().current.clone();
    let writes = fixture.lock().unwrap().global_writes;
    let replacement = session(&fixture);
    assert!(
        !replacement
            .recover_abandoned_owner("not-this-owner")
            .unwrap()
    );
    assert!(fixture.lock().unwrap().journal.is_some());
    assert!(
        replacement
            .recover_abandoned_owner(owner.owner_id())
            .unwrap()
    );
    let state = fixture.lock().unwrap();
    assert_eq!(state.current, before);
    assert_eq!(state.global_writes, writes);
    assert!(state.journal.is_none());
    assert!(!state.leased);
}

#[test]
fn disabled_release_requires_committed_ownership_and_unchanged_supported_direct_scope() {
    for edit in 0..9 {
        let fixture = fixture();
        let owner = session(&fixture);
        if edit == 0 {
            let mut state = fixture.lock().unwrap();
            state.current.effective.flags = DIRECT | PROXY;
            state.current.effective.server = "foreign-before-acquire:88".into();
        }
        owner
            .acquire_with_policy(endpoint(), AcquisitionPolicy::ExplicitTakeover)
            .unwrap();
        manually_disable_owned_proxy(&fixture);
        {
            let mut state = fixture.lock().unwrap();
            state.leased = false;
            match edit {
                0 => {} // Never reactivate an explicitly replaced foreign baseline.
                1 => state.current.effective.flags = DIRECT | PROXY,
                2 => state.current.effective.flags = DIRECT | 8,
                3 => state.current.effective.pac_url = "https://policy.invalid/pac".into(),
                4 => {
                    state.current.guards[0] = Some(TypedValue {
                        kind: 1,
                        bytes: vec![0, 0],
                    })
                }
                5 => state.current.values[0] = None,
                6 => {
                    state.current.values[0] = Some(TypedValue {
                        kind: 1,
                        bytes: vec![0, 0],
                    })
                }
                7 => state.journal.as_mut().unwrap().observed = None,
                _ => {
                    state
                        .journal
                        .as_mut()
                        .unwrap()
                        .observed
                        .as_mut()
                        .unwrap()
                        .effective
                        .server = "unknown-checkpoint:99".into()
                }
            }
        }
        let before = fixture.lock().unwrap().current.clone();
        let writes = fixture.lock().unwrap().global_writes;
        let replacement = session(&fixture);
        assert!(
            replacement
                .recover_abandoned_owner(owner.owner_id())
                .is_err(),
            "edit {edit}"
        );
        let state = fixture.lock().unwrap();
        assert_eq!(state.current, before, "edit {edit}");
        assert_eq!(state.global_writes, writes, "edit {edit}");
        assert!(state.journal.is_some());
    }
}

#[test]
fn disabled_release_retains_evidence_on_failed_clear_or_changed_durable_owner() {
    for change_owner in [false, true] {
        let fixture = fixture();
        let owner = session(&fixture);
        owner.acquire(endpoint()).unwrap();
        manually_disable_owned_proxy(&fixture);
        {
            let mut state = fixture.lock().unwrap();
            if change_owner {
                state.journal.as_mut().unwrap().owner = "successor".into();
            } else {
                state.clear_failure = true;
            }
        }
        let before = fixture.lock().unwrap().current.clone();
        let writes = fixture.lock().unwrap().global_writes;
        assert!(owner.restore().is_err());
        assert_eq!(fixture.lock().unwrap().current, before);
        assert_eq!(fixture.lock().unwrap().global_writes, writes);
        assert!(fixture.lock().unwrap().journal.is_some());
        assert!(fixture.lock().unwrap().leased);
        if !change_owner {
            fixture.lock().unwrap().clear_failure = false;
            owner.restore().unwrap();
            assert!(fixture.lock().unwrap().journal.is_none());
            assert_eq!(fixture.lock().unwrap().current, before);
        }
    }
}

#[test]
fn reenabled_proxy_during_disabled_release_is_not_cleared_or_overwritten() {
    let fixture = fixture();
    let owner = session(&fixture);
    owner.acquire(endpoint()).unwrap();
    manually_disable_owned_proxy(&fixture);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    fixture.lock().unwrap().load_pause = Some(Pause {
        entered: entered_tx,
        resume: resume_rx,
    });
    let writes = fixture.lock().unwrap().global_writes;
    let worker = std::thread::spawn(move || owner.restore());
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    {
        let mut state = fixture.lock().unwrap();
        state.current.effective.flags = DIRECT | PROXY;
        state.current.effective.server = "external-successor:88".into();
    }
    let before = fixture.lock().unwrap().current.clone();
    resume_tx.send(()).unwrap();
    assert!(worker.join().unwrap().is_err());
    let state = fixture.lock().unwrap();
    assert_eq!(state.current, before);
    assert_eq!(state.global_writes, writes);
    assert!(state.journal.is_some());
    assert!(state.leased);
}
