//! Native core lifecycle bridge for mobile hosts.
//!
//! Desktop hosts spawn core binaries through [`lanpath_engine::CoreManager`].
//! Android uses the Kotlin VPN service and the Go/libbox binding instead.
//! A future mobile host must install a bridge before starting [`CoreRunner`].
//! The registry permits one bridge per process and does not create a JNI adapter.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use lanpath_common::{CoreType, LanpathError, LanpathResult};
use lanpath_engine::CoreManager;
use serde_json::Value;

/// 桥接方法返回的盒化 future（手写对象安全异步签名，避免引入额外 crate）。
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Delegate core lifecycle operations to a native platform implementation.
pub trait CoreEngineBridge: Send + Sync {
    /// 启动核心。
    ///
    /// `core_type` 供 Android 桥向 Kotlin 插件分派核心类型（Android 仅支持 sing-box，
    /// 桌面支持 sing-box/Mihomo）；`config_json` 为所选核心的配置。
    fn start<'a>(
        &'a self,
        core_type: CoreType,
        config_json: &'a Value,
    ) -> BoxFuture<'a, LanpathResult<()>>;

    /// 停止核心。
    fn stop<'a>(&'a self) -> BoxFuture<'a, LanpathResult<()>>;

    /// 核心是否在运行。
    fn is_running<'a>(&'a self) -> BoxFuture<'a, bool>;
}

/// 全局核心引擎桥注册表（只允许安装一次）。
static CORE_ENGINE_BRIDGE: std::sync::OnceLock<Arc<dyn CoreEngineBridge>> =
    std::sync::OnceLock::new();

/// Install the global core bridge from the Android host; repeated installation fails.
pub fn install_core_engine_bridge(bridge: Arc<dyn CoreEngineBridge>) -> LanpathResult<()> {
    CORE_ENGINE_BRIDGE
        .set(bridge)
        .map_err(|_| LanpathError::Client("核心引擎桥已安装，不能重复安装".into()))
}

/// 读取已安装的核心引擎桥；未安装时返回 `None`。
pub fn core_engine_bridge() -> Option<Arc<dyn CoreEngineBridge>> {
    CORE_ENGINE_BRIDGE.get().cloned()
}

/// [`lanpath_engine::CoreManager`] → [`CoreEngineBridge`] 适配器：
/// 把 `CoreManager` 的调用转发给桥，供 Android 下 [`crate::runner::CoreRunner`]
/// 复用现有 `Box<dyn CoreManager>` 容器。
pub struct CoreEngineBridgeAdapter {
    core_type: CoreType,
    bridge: Arc<dyn CoreEngineBridge>,
}

impl CoreEngineBridgeAdapter {
    /// 构造适配器（记录核心类型以满足 [`CoreManager::core_type`]）。
    pub fn new(core_type: CoreType, bridge: Arc<dyn CoreEngineBridge>) -> Self {
        Self { core_type, bridge }
    }
}

#[async_trait::async_trait]
impl CoreManager for CoreEngineBridgeAdapter {
    fn core_type(&self) -> CoreType {
        self.core_type
    }

    async fn start(&self, config: &Value) -> LanpathResult<()> {
        self.bridge.start(self.core_type, config).await
    }

    async fn stop(&self) -> LanpathResult<()> {
        self.bridge.stop().await
    }

    async fn restart(&self, config: &Value) -> LanpathResult<()> {
        self.stop().await?;
        self.start(config).await
    }

    async fn is_running(&self) -> bool {
        self.bridge.is_running().await
    }

    async fn reload(&self, config: &Value) -> LanpathResult<()> {
        self.restart(config).await
    }

    async fn version(&self) -> LanpathResult<String> {
        Ok(String::new())
    }

    async fn uptime_secs(&self) -> LanpathResult<u64> {
        Ok(0)
    }

    async fn active_inbounds(&self) -> LanpathResult<Vec<String>> {
        Ok(vec![])
    }

    async fn last_error(&self) -> LanpathResult<String> {
        Ok(String::new())
    }
}
