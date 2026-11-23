//! 运行时连接追踪与本地统计存储衔接。
//!
//! 核心启动并启用 Clash API 后自动拉起 `ConnectionTrackerHandle`（WS 推送优先，
//! HTTP 轮询兜底），将增量字节与关闭连接持久化到 `<data_root>/runtime/db/stats.db`；
//! 核心停止、异常退出或运行时释放时及时终止追踪任务。

use std::sync::Arc;

use lanpath_client::{
    ActiveConnections, ClientConfig, ConnectionView, StatsStore, start_connection_tracker,
};
use lanpath_common::LanpathResult;

use super::CoreRuntime;

impl CoreRuntime {
    /// 获取或惰性打开本数据根下的 SQLite 流量统计库（位于 `runtime/db/stats.db`，不纳入备份）。
    pub(crate) async fn stats_store(&self) -> LanpathResult<Arc<StatsStore>> {
        let db_dir = self.root.path().join("runtime/db");
        let store = self
            .stats
            .get_or_try_init(|| async move { StatsStore::open(&db_dir).await.map(Arc::new) })
            .await?;
        Ok(Arc::clone(store))
    }

    /// 返回当前运行会话的 Clash API 端口与密钥（未运行或未启用时返回 `None`）。
    pub(crate) fn clash_api_endpoint(&self) -> Option<(u16, String)> {
        self.active.lock().ok().and_then(|active| {
            active.as_ref().and_then(|info| {
                info.clash_api_port
                    .map(|port| (port, info.config.clash_api_secret.clone()))
            })
        })
    }

    /// 核心就绪后按配置启动后台连接追踪（未开启 Clash API 时跳过）。
    pub(super) async fn start_tracker(&self, config: &ClientConfig) {
        self.stop_tracker().await;
        if !config.clash_api_enabled {
            return;
        }
        let sink = match self.stats_store().await {
            Ok(store) => Some(store),
            Err(error) => {
                tracing::warn!(
                    target: "lanpath_desktop_nsh::core_runtime",
                    %error,
                    "打开流量统计库失败，连接追踪仅保留内存快照"
                );
                None
            }
        };
        let handle =
            start_connection_tracker(config.clash_api_port, config.clash_api_secret.clone(), sink);
        let mut slot = self.tracker.lock().await;
        *slot = Some(handle);
    }

    /// 停止并回收后台连接追踪任务。
    pub(super) async fn stop_tracker(&self) {
        let handle = {
            let mut slot = self.tracker.lock().await;
            slot.take()
        };
        if let Some(handle) = handle {
            handle.stop().await;
        }
    }

    /// 读取当前活跃连接快照（追踪器未就绪时回退为单次 HTTP 查询）。
    pub(crate) async fn active_connections(&self) -> Option<ActiveConnections> {
        let guard = self.tracker.lock().await;
        if let Some(handle) = guard.as_ref() {
            return Some(handle.active().await);
        }
        drop(guard);
        let (port, secret) = self.clash_api_endpoint()?;
        let conns = lanpath_client::clash_get_connections(port, &secret)
            .await
            .ok()?;
        let upload_total = conns.iter().map(|c| c.upload).sum();
        let download_total = conns.iter().map(|c| c.download).sum();
        Some(ActiveConnections {
            connections: conns,
            upload_total,
            download_total,
        })
    }

    /// 读取追踪器内存中的最近已关闭连接（最新在前）。
    pub(crate) async fn closed_connections(&self) -> Vec<ConnectionView> {
        let guard = self.tracker.lock().await;
        let Some(handle) = guard.as_ref() else {
            return Vec::new();
        };
        let mut list = handle.closed().await;
        list.reverse();
        list
    }
}
