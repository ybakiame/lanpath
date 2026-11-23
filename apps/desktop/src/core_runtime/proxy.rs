//! CoreRuntime 的系统代理守护编排：arm/acquire/restore/recover/status。
//!
//! 语义从 apps/gpui 的 `runtime/system_control.rs` 迁移（不简化）：
//! - 接管只经独立同映像守护（GUI 侧绝不直写注册表）；owner 为本数据根
//!   UUID，守护持有共享 `SystemProxySession` 的全部变更/检查点。
//! - 接管失败保留核心与守护证据（不 rollback）；停止先守护 restore +
//!   确认退出再停核心；守护死亡/通信失败不是恢复证明，只按本应用 owner
//!   精确恢复（`has_recovery_for_owner` 为真才 arm 继任守护 recover）。
//! - 锁顺序：调用方持有 lifecycle（启停）时本模块只取 proxy_guardian
//!   短临界区；守护 RPC 绝不持 business 锁（配置读取在 arm 之前完成）。

use std::sync::atomic::Ordering;

use lanpath_client::sysproxy::SystemProxyOwnership;
use lanpath_common::{LanpathError, LanpathResult};

use super::CoreRuntime;
use crate::system_proxy::{ArmedGuardian, GuardianSpawner};

/// 探测结果：ownership 查询错误也要能上屏（未知错误态）。
pub(crate) type ProxyOwnershipProbe = Result<SystemProxyOwnership, String>;

impl CoreRuntime {
    /// 当前 spawner（Mutex 只保护 Arc 交换，绝不跨守护 RPC 持有）。
    pub(super) fn proxy_spawner(&self) -> std::sync::Arc<dyn GuardianSpawner> {
        self.proxy_spawner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// 测试注入守护装配 fixture（共享 session 语义，不是 Windows 真实 WinINet）。
    #[cfg(test)]
    pub(crate) fn replace_proxy_spawner(&self, spawner: std::sync::Arc<dyn GuardianSpawner>) {
        *self
            .proxy_spawner
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = spawner;
    }

    /// 测试断言用：守护日志 owner（本数据根 UUID）。
    #[cfg(test)]
    pub(crate) fn proxy_owner_for_test(&self) -> String {
        self.proxy_owner.clone()
    }

    /// 平台是否支持系统代理守护（Windows 真实守护；测试 fixture 可声明支持）。
    pub(crate) fn proxy_supported(&self) -> bool {
        self.proxy_spawner().supported()
    }

    pub(super) fn proxy_guardian_lock(
        &self,
    ) -> std::sync::MutexGuard<'_, Option<Box<dyn ArmedGuardian>>> {
        self.proxy_guardian
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn client_error(message: impl Into<String>) -> LanpathError {
        LanpathError::Client(message.into())
    }

    /// 核心校验/mixed/API 全部就绪后的接管：arm 独立守护并 acquire
    /// `127.0.0.1:port`。未启用或平台不支持时不动作；失败时守护（若已
    /// arm）与核心全部保留，由停止/显式恢复完成精确恢复。
    ///
    /// 普通 acquire 恒为共享 RequireDirect（外来活动代理拒绝）。仅当
    /// 用户在本进程确认过一次（`grant_takeover_consent`）时，本次
    /// acquire 以 ExplicitTakeover 发起；授权在发起前读取并消费，成功
    /// 或失败都只影响这一次。
    pub(super) fn acquire_proxy(&self, enabled: bool, port: u16) -> LanpathResult<()> {
        let spawner = self.proxy_spawner();
        if !spawner.supported() {
            if enabled {
                tracing::info!(target: "lanpath::core_runtime",
                    "当前平台不支持系统代理守护；已保存偏好本次不生效（不接管）");
            }
            return Ok(());
        }
        if !enabled {
            return Ok(());
        }
        let mut guardian = self.proxy_guardian_lock();
        if guardian.is_some() {
            return Err(Self::client_error(
                "上次系统代理守护会话尚未结束；请先停止并恢复",
            ));
        }
        let armed = spawner.arm(&self.proxy_owner).map_err(|error| {
            Self::client_error(format!("系统代理守护启动失败；未接管系统代理：{error}"))
        })?;
        let armed = guardian.insert(armed);
        // 一次性接管授权：读取并消费。普通 Acquire 绝不因此升级——未确认
        // 时外来活动代理仍由共享层 RequireDirect 拒绝。
        let takeover = self.proxy_takeover_consent.swap(false, Ordering::AcqRel);
        // 意图先于 RPC：超时/丢响应可能意味着独立守护已变更 WinINet。
        // 任何失败都保留核心与守护，绝不在此 rollback 清证据。
        let result = if takeover {
            armed.acquire_takeover(port)
        } else {
            armed.acquire(port)
        };
        result.map_err(|error| {
            Self::client_error(format!(
                "系统代理守护接管失败；已保留核心与恢复资源：{error}"
            ))
        })
    }

    /// UI 确认框确认：授予进程内一次性接管授权（不持久化，重启即失效）。
    pub(crate) fn grant_takeover_consent(&self) {
        self.proxy_takeover_consent.store(true, Ordering::Release);
        tracing::info!(target: "lanpath::core_runtime",
            "已收到一次性显式接管授权（仅本进程，不持久化）");
    }

    /// 停用系统代理偏好时清除未消费的一次性授权。
    pub(crate) fn clear_takeover_consent(&self) {
        self.proxy_takeover_consent.store(false, Ordering::Release);
    }

    /// 停止路径的守护恢复：已装配守护 shutdown（restore + 退出确认）；
    /// 守护死亡/通信失败不是恢复证明——仅当本应用 owner 的遗留日志存在
    /// 时，arm 继任守护按 owner 精确恢复（外来/继任记录拒绝覆盖）。
    pub(super) fn restore_proxy(&self) -> LanpathResult<()> {
        let spawner = self.proxy_spawner();
        if !spawner.supported() {
            return Ok(());
        }
        let mut guardian = self.proxy_guardian_lock();
        if let Some(armed) = guardian.as_mut() {
            armed.shutdown().map_err(|error| {
                Self::client_error(format!(
                    "系统代理守护恢复/退出未确认；保留核心及恢复资源：{error}"
                ))
            })?;
            *guardian = None;
        }
        // 无装配守护时仍可能有遗留日志（如 GUI 退出后守护留下的恢复记录）：
        // 只有本应用 owner 的精确记录才授权继任守护恢复。
        if spawner
            .has_recovery_for_owner(&self.proxy_owner)
            .map_err(|error| Self::client_error(format!("系统代理恢复记录查询失败：{error}")))?
        {
            let armed = spawner.arm(&self.proxy_owner).map_err(|error| {
                Self::client_error(format!("系统代理恢复守护启动失败：{error}"))
            })?;
            let armed = guardian.insert(armed);
            armed.recover_watched().map_err(|error| {
                Self::client_error(format!("系统代理守护遗留恢复失败：{error}"))
            })?;
            armed.shutdown().map_err(|error| {
                Self::client_error(format!("系统代理恢复守护退出未确认：{error}"))
            })?;
            *guardian = None;
        }
        Ok(())
    }

    /// 启动前恢复：本实例已装配守护仍持有时拒绝启动（Owned）；遗留
    /// abandoned journal 由独立守护完成恢复（GUI 死亡不能中断检查点）。
    pub(super) fn recover_proxy_before_start(&self) -> LanpathResult<()> {
        let spawner = self.proxy_spawner();
        if !spawner.supported() {
            return Ok(());
        }
        let mut guardian = self.proxy_guardian_lock();
        if let Some(armed) = guardian.as_mut() {
            if armed
                .status()
                .map_err(|error| Self::client_error(format!("系统代理守护状态未知：{error}")))?
                == SystemProxyOwnership::Owned
            {
                return Err(Self::client_error(
                    "系统代理会话仍被本实例持有；请先停止并恢复",
                ));
            }
            armed.shutdown().map_err(|error| {
                Self::client_error(format!("遗留守护会话恢复/退出未确认：{error}"))
            })?;
            *guardian = None;
        }
        match spawner
            .ownership(&self.proxy_owner)
            .map_err(|error| Self::client_error(error.to_string()))?
        {
            SystemProxyOwnership::Unowned => Ok(()),
            SystemProxyOwnership::Owned => Err(Self::client_error(
                "系统代理会话仍被本实例持有；请先停止并恢复",
            )),
            SystemProxyOwnership::RecoveryRequired => {
                // 显式启动恢复授权移交给同一独立进程；GUI 死亡不能中断
                // 其检查点。外来活动 owner 由共享会话 lease 拒绝。
                let armed = spawner.arm(&self.proxy_owner).map_err(|error| {
                    Self::client_error(format!("遗留代理恢复守护启动失败：{error}"))
                })?;
                let armed = guardian.insert(armed);
                armed.recover_abandoned().map_err(|error| {
                    Self::client_error(format!("遗留系统代理恢复失败：{error}"))
                })?;
                armed.shutdown().map_err(|error| {
                    Self::client_error(format!("遗留代理恢复守护退出未确认：{error}"))
                })?;
                *guardian = None;
                Ok(())
            }
        }
    }

    /// 状态查询使用守护真实 ownership：已装配守护优先（守护已退出返回
    /// RecoveryRequired，不是恢复证明）；无守护按 owner 查共享会话。
    /// 无记录且当前直连按共享语义释放；外来代理/未知变化拒绝覆盖。
    pub(crate) fn proxy_ownership(&self) -> ProxyOwnershipProbe {
        let spawner = self.proxy_spawner();
        if !spawner.supported() {
            return Err("系统代理守护仅支持 Windows".to_owned());
        }
        if let Some(armed) = self.proxy_guardian_lock().as_mut() {
            return armed
                .status()
                .map_err(|error| format!("系统代理守护状态未知：{error}"));
        }
        spawner
            .ownership(&self.proxy_owner)
            .map_err(|error| error.to_string())
    }

    /// 显式“恢复系统代理”操作（UI 入口，独立任务中执行）：只允许本应用
    /// owner，绝无条件清空；未运行时也允许（idle 恢复）。
    pub(crate) async fn restore_system_proxy(self: &std::sync::Arc<Self>) -> LanpathResult<()> {
        let this = self.clone();
        tokio::spawn(async move { this.restore_proxy() })
            .await
            .map_err(|error| LanpathError::Core(format!("系统代理恢复任务异常中断:{error}")))?
    }

    /// 读已保存的系统代理偏好（UI 入口；同一业务锁内读取）。
    pub(crate) fn proxy_preference(&self) -> LanpathResult<bool> {
        crate::system_proxy::load_preference(&self.root)
    }

    /// 保存系统代理偏好（配置合并，只改一个字段）；运行中生效必须走
    /// 安全启停（`stop` 含守护恢复 → `start` 按新偏好接管），本函数绝不
    /// 绕过守护直写注册表。
    pub(crate) fn save_proxy_preference(&self, enabled: bool) -> LanpathResult<()> {
        crate::system_proxy::save_preference(&self.root, enabled)
    }
}

#[cfg(test)]
#[path = "proxy_tests.rs"]
mod tests;
