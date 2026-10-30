use super::DesktopRuntime;

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
            let retained = self.manager.lock().await.is_some()
                || self.broker.lock().await.is_some()
                || self.mitm.lock().await.is_some();
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
            let recovery_required = self.proxy_recovery_pending()?
                || (!running && self.manager.lock().await.is_some())
                || self.broker.lock().await.is_some()
                || self.mitm.lock().await.is_some();
            Ok(RuntimeStatus {
                running,
                recovery_required,
                detail,
            })
        })
    }
}
