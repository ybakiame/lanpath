use super::*;

impl Service {
    pub(super) fn stop(&mut self, recovering: bool) -> LanpathResult<StopReceipt> {
        let Some(record) = self.record.as_mut() else {
            return Ok(StopReceipt {
                kind: StopKind::AlreadyStopped,
                process_exited: true,
                precise_cleanup_proven: true,
                observed_remaining: Vec::new(),
                detail: "no machine TUN session is owned".into(),
            });
        };
        let had_effects = record.core.is_some() || record.state != SessionState::Prepared;
        let mut graceful = !had_effects;
        let mut exited = record.core.is_none();
        if let Some(running) = self.running.as_mut() {
            exited = running.process.exited()?;
            if !exited {
                let delivered =
                    unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, running.child.id()) } != 0;
                if delivered {
                    graceful = unsafe { WaitForSingleObject(running.process.handle(), 8_000) }
                        == WAIT_OBJECT_0;
                }
                if !graceful {
                    record.forced = true;
                    record.state = SessionState::RecoveryRequired;
                    record.detail = "graceful stop deadline expired or Ctrl-Break could not be delivered; forced termination intent persisted".into();
                    self.journal.save(record)?;
                    running.process.terminate_and_wait(5_000)?;
                }
                exited = running.process.exited()?;
            } else if had_effects && !record.forced {
                record.uncertain = true;
            }
            if exited {
                running.child.wait()?;
            }
        } else if let Some(identity) = &record.core {
            // A protected journal is authority only for this original process, never its reused PID.
            match WindowsProcess::open(identity.pid, true) {
                Ok(process) => {
                    let actual = process.identity()?;
                    if actual.created != identity.created {
                        // PID reuse proves the old process is gone, not authority to touch its replacement.
                        exited = true;
                    } else {
                        if actual != *identity {
                            return Err(failure(
                                "recorded core image identity changed; no process touched",
                            ));
                        }
                        let _image = crate::privileged::staging::lock_parent_image(
                            &identity.exe,
                            &record.core_sha256,
                        )?;
                        if process.exited()? {
                            exited = true;
                        } else {
                            if !recovering {
                                return Err(failure(
                                    "previous helper session requires explicit recovery",
                                ));
                            }
                            let attached = Console::attach(identity.pid);
                            if attached.is_ok()
                                && unsafe {
                                    GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, identity.pid)
                                } != 0
                            {
                                graceful = unsafe { WaitForSingleObject(process.handle(), 8_000) }
                                    == WAIT_OBJECT_0;
                            }
                            if !graceful {
                                record.forced = true;
                                self.journal.save(record)?;
                                process.terminate_and_wait(5_000)?;
                            }
                            exited = process.exited()?;
                        }
                    }
                }
                Err(e) if e.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) => {
                    exited = true;
                    record.uncertain = true;
                }
                Err(e) => {
                    return Err(failure(format!(
                        "cannot inspect original core; journal retained: {e}"
                    )));
                }
            }
        } else if had_effects {
            record.uncertain = true;
        }
        if exited {
            self.running = None;
        }
        let firewall_error = if exited {
            record
                .firewall
                .as_ref()
                .and_then(|expected| {
                    Firewall::open()
                        .and_then(|firewall| firewall.remove(expected))
                        .err()
                })
                .map(|error| error.to_string())
        } else {
            None
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        let observation_error = loop {
            let error = match network::remaining(
                &record.prepared,
                record.adapter.as_ref(),
                &record.observed,
            ) {
                Ok(observed) => {
                    record.observed = observed;
                    None
                }
                Err(error) => Some(error.to_string()),
            };
            if !exited
                || (error.is_none() && record.observed.is_empty())
                || Instant::now() >= deadline
            {
                break error;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let unknown_launch = had_effects && record.core.is_none();
        let unreceipted_creation = record.creation_authorized && record.adapter.is_none();
        let network_unresolved = unknown_launch
            || unreceipted_creation
            || !exited
            || !record.observed.is_empty()
            || observation_error.is_some()
            || firewall_error.is_some();
        let cache_error = if !network_unresolved && record.cache_enabled && had_effects {
            crate::privileged::core_cache::save(
                self.journal.root(),
                &self.owner_sid,
                self.root_id,
                &self
                    .journal
                    .root()
                    .join("sessions")
                    .join(&record.prepared.session),
            )
            .err()
            .map(|error| error.to_string())
        } else {
            None
        };
        let unresolved = network_unresolved || cache_error.is_some();
        let kind = StopKind::completed(unresolved, record.forced, had_effects, graceful);
        let detail = if let Some(error) = observation_error {
            format!(
                "process exit={exited}; native network observation failed: {error}; recovery retained"
            )
        } else if let Some(error) = firewall_error {
            format!(
                "process exit={exited}; owned firewall cleanup unresolved: {error}; recovery retained"
            )
        } else if let Some(error) = cache_error {
            format!(
                "native network cleanup verified, but managed cache commit failed: {error}; stage and journal retained"
            )
        } else if unresolved {
            "owned process exit, launch identity, or native adapter/address/route/DNS absence remains unproven; journal and stage retained; no network deletions attempted".into()
        } else if had_effects {
            "original managed core exited; native adapter/address/route/DNS absence and owned unchanged firewall removal verified; driver packages and global DNS cache are not restored".into()
        } else {
            "prepared session cancelled before core execution; no privileged network effects occurred".into()
        };
        let receipt = StopReceipt {
            kind,
            process_exited: exited,
            precise_cleanup_proven: !unresolved,
            observed_remaining: record.observed.clone(),
            detail: detail.clone(),
        };
        record.state = if unresolved {
            SessionState::RecoveryRequired
        } else {
            SessionState::Stopped
        };
        record.detail = detail;
        self.journal.save(record)?;
        if !unresolved {
            let path = self
                .journal
                .root()
                .join("sessions")
                .join(&record.prepared.session);
            self.stage = None;
            if path.exists() {
                std::fs::remove_dir_all(path)?;
            }
            self.journal.clear()?;
            self.record = None;
        }
        Ok(receipt)
    }
}
