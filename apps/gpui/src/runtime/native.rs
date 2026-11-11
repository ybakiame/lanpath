use std::sync::{Arc, atomic::Ordering};

use lanpath_script::Notifier;
use tokio::sync::OwnedMutexGuard;

use super::DesktopRuntime;
use crate::{
    data_root::{DataMode, DataRoot},
    distribution::{instance::Activation, native::NotificationService},
};

/// Retain the proven stopped state while preparing a data migration.
pub(crate) struct StoppedLease {
    _lease: OwnedMutexGuard<()>,
}

impl DesktopRuntime {
    pub(crate) fn data_root(&self) -> Result<&DataRoot, String> {
        self._data_root
            .as_deref()
            .ok_or_else(|| self.unavailable_data_error())
    }

    pub(crate) fn data_mode(&self) -> Result<DataMode, String> {
        self.data_mode.ok_or_else(|| self.unavailable_data_error())
    }

    pub(crate) fn saved_proxy_port_changed(&self, mixed: u16) {
        self.mixed_port.store(mixed, Ordering::Release);
    }

    pub(crate) fn poll_activation(&self) -> Result<bool, String> {
        let instance = self
            .instance
            .lock()
            .map_err(|_| "实例激活状态锁不可用".to_string())?;
        Ok(instance.as_ref().and_then(|instance| instance.try_recv())
            == Some(Activation::ShowWindow))
    }

    pub(crate) fn stop_for_migration(&self) -> Result<StoppedLease, String> {
        self.executor()?.block_on(async {
            let lease = self.lifecycle_gate.clone().lock_owned().await;
            let _ = self.stop_locked().await?;
            Ok(StoppedLease { _lease: lease })
        })
    }

    pub(super) fn notifier(&self) -> Result<Arc<dyn Notifier>, String> {
        let service =
            NotificationService::new(self.data_mode()?).map_err(|error| error.to_string())?;
        Ok(Arc::new(NativeNotifier { service }))
    }
}

struct NativeNotifier {
    service: NotificationService,
}
impl Notifier for NativeNotifier {
    fn notify(&self, title: &str, subtitle: &str, body: &str, options: Option<serde_json::Value>) {
        if options.as_ref().is_some_and(|value| {
            !value.is_null() && value.as_object().is_none_or(|object| !object.is_empty())
        }) {
            tracing::warn!("原生通知尚不支持附件或点击动作；未执行这些附加动作");
        }
        let message;
        let body = if subtitle.is_empty() {
            body
        } else {
            message = format!("{subtitle}\n{body}");
            &message
        };
        if let Err(error) = self.service.notify(title, body) {
            tracing::error!(%error, "原生脚本通知未被系统接受");
        }
    }
}
