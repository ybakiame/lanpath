use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use serde_json::Value;

use super::{DesktopRuntime, system_control::broker_request};
use crate::privileged::{Broker, SessionState, snapshot_resources};

impl DesktopRuntime {
    pub(super) async fn start_tun(&self, mut config: Value, binary: &Path) -> Result<(), String> {
        if self.core_override().is_some() {
            return Err("TUN 不接受环境变量指定的核心；请选择产品签名清单中的核心".into());
        }
        // Caller-controlled output paths never cross the elevation boundary. The
        // helper substitutes its own protected log/cache paths after admission.
        if let Some(log) = config.get_mut("log").and_then(Value::as_object_mut) {
            log.insert("output".into(), Value::String("stderr".into()));
        }
        if let Some(cache) = config
            .pointer_mut("/experimental/cache_file")
            .and_then(Value::as_object_mut)
        {
            cache.insert("path".into(), Value::String(String::new()));
        }
        let resources = snapshot_resources(self.business()?.data_dir(), &mut config)
            .map_err(|error| error.to_string())?;
        let binary = binary.to_path_buf();
        let (_, selected) =
            tokio::task::spawn_blocking(move || crate::distribution::signature::digest(&binary))
                .await
                .map_err(|error| format!("核心摘要任务失败：{error}"))?
                .map_err(|error| error.to_string())?;
        let root_id = self
            ._data_root
            .as_ref()
            .ok_or("TUN 缺少独占数据目录身份")?
            .id();
        let broker = Arc::new(Mutex::new(
            tokio::task::spawn_blocking(move || Broker::launch(root_id))
                .await
                .map_err(|error| format!("TUN 授权任务失败：{error}"))?
                .map_err(|error| error.to_string())?,
        ));
        // Retain the connection before prepare/start: partial attempts remain stoppable.
        *self.broker.lock().await = Some(broker.clone());
        let status = broker_request(broker.clone(), Broker::status).await?;
        if status.state != SessionState::Idle {
            let receipt = broker_request(broker.clone(), Broker::recover).await?;
            if !receipt.process_exited
                || !receipt.precise_cleanup_proven
                || !receipt.observed_remaining.is_empty()
            {
                return Err(format!("遗留 TUN 尚未证明恢复：{}", receipt.detail));
            }
        }
        broker_request(broker.clone(), move |broker| {
            broker.prepare(config, resources, selected)
        })
        .await?;
        let status = broker_request(broker, Broker::start).await?;
        if status.state != SessionState::Running {
            return Err(format!("TUN 未进入运行状态：{}", status.detail));
        }
        Ok(())
    }
}
