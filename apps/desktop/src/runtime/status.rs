use super::DesktopRuntime;
use lanpath_client::sysproxy::SystemProxyOwnership;

#[cfg(test)]
mod real_core_tests;

pub(crate) struct RuntimeStatus {
    pub running: bool,
    pub recovery_required: bool,
    pub detail: Option<String>,
}

impl DesktopRuntime {
    pub(crate) fn poll_lifecycle(&self, recover_if_exited: bool) -> Result<RuntimeStatus, String> {
        self.executor()?.block_on(async {
            let _lease = self.lifecycle_gate.lock().await;
            let running = self.running_async().await?;
            let retained = self.has_retained_resources().await;
            let detail = if recover_if_exited && !running && retained {
                Some(match self.stop_locked().await {
                    Ok(Some(kind)) => {
                        format!("核心意外退出；停止结果 {kind:?}，自有控制已停止并恢复")
                    }
                    Ok(None) => "核心意外退出；自有控制已停止并恢复，未采集退出方式".into(),
                    Err(error) => format!("核心状态待恢复：{error}"),
                })
            } else {
                None
            };
            let recovery_required = self.recovery_required_async(running).await?;
            Ok(RuntimeStatus {
                running,
                recovery_required,
                detail,
            })
        })
    }

    pub(super) async fn recovery_required_async(&self, running: bool) -> Result<bool, String> {
        #[cfg(windows)]
        let ownership = self.proxy_ownership()?;
        #[cfg(not(windows))]
        let ownership = SystemProxyOwnership::Unowned;
        Ok(requires_recovery(
            running,
            ownership,
            self.has_retained_resources().await,
        ))
    }

    async fn has_retained_resources(&self) -> bool {
        self.manager.lock().await.is_some()
            || self.broker.lock().await.is_some()
            || self.mitm.lock().await.is_some()
            || self.script_tasks.lock().await.is_some()
    }
}

/// A healthy live session owns resources by design. Once the core stops, those
/// same resources are obligations until verified cleanup; explicit proxy faults
/// require recovery regardless of whether the core is still running.
fn requires_recovery(running: bool, ownership: SystemProxyOwnership, retained: bool) -> bool {
    ownership == SystemProxyOwnership::RecoveryRequired
        || (!running && (ownership == SystemProxyOwnership::Owned || retained))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_running_core_can_own_proxy_mitm_helper_and_tasks() {
        assert!(!requires_recovery(true, SystemProxyOwnership::Owned, true));
        assert!(!requires_recovery(
            true,
            SystemProxyOwnership::Unowned,
            true
        ));
        assert!(!requires_recovery(
            true,
            SystemProxyOwnership::Unowned,
            false
        ));
    }

    #[test]
    fn stopped_core_keeps_ownership_and_retained_resources_actionable() {
        assert!(requires_recovery(false, SystemProxyOwnership::Owned, false));
        assert!(requires_recovery(
            false,
            SystemProxyOwnership::Unowned,
            true
        ));
        assert!(!requires_recovery(
            false,
            SystemProxyOwnership::Unowned,
            false
        ));
    }

    #[test]
    fn explicit_proxy_conflict_is_never_hidden_by_a_live_core() {
        for running in [false, true] {
            for retained in [false, true] {
                assert!(requires_recovery(
                    running,
                    SystemProxyOwnership::RecoveryRequired,
                    retained
                ));
            }
        }
    }
}
