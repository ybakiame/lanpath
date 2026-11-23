//! 系统代理接管的产品 glue：owner 身份、守护装配抽象与偏好持久化。
//!
//! - `APP_ID` 是本应用身份 `com.ybakiame.lanpath`；守护日志的
//!   owner 为本数据根 UUID（`DataRoot::id`），跨进程复用同一份精确身份。
//! - `GuardianSpawner`/`ArmedGuardian` 把“启动同映像独立守护进程 + RPC”
//!   抽象成可注入接口：Windows 走真实 `proxy_guardian::Guardian`，测试用
//!   共享 session 语义的 fixture（不是 Windows 真实 WinINet）覆盖
//!   启动/停止/异常路径；非 Windows 恒不支持，运行时不因偏好产生失败。
//! - 偏好读写只改 `ClientConfig.system_proxy_enabled` 一个字段（同一业务锁
//!   内读最新配置后保存），绝不覆盖 active 选择、端口、请求代理等无关字段。

use std::sync::{Arc, Mutex};

use lanpath_client::sysproxy::SystemProxyOwnership;
#[cfg(not(windows))]
use lanpath_common::LanpathError;
use lanpath_common::LanpathResult;

use crate::data_root::DataRoot;

/// 本应用发行身份（与窗口壳 app_id 一致）；守护日志的 owner_label。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const APP_ID: &str = "com.ybakiame.lanpath";

#[cfg(not(windows))]
fn unsupported() -> LanpathError {
    LanpathError::Client("系统代理守护仅支持 Windows；本应用未接管系统代理".into())
}

/// 已装配守护句柄：Guardian 侧 RPC 客户端（守护进程内才是会话 owner，
/// GUI 侧绝不直接写注册表）。
pub(crate) trait ArmedGuardian: Send {
    /// 普通接管：共享 RequireDirect 策略，外来活动代理拒绝。
    fn acquire(&mut self, port: u16) -> LanpathResult<()>;
    /// 显式一次性接管（用户已确认）：共享 ExplicitTakeover 策略。
    fn acquire_takeover(&mut self, port: u16) -> LanpathResult<()>;
    fn recover_abandoned(&mut self) -> LanpathResult<()>;
    /// 只恢复本应用 owner 的已认证日志；外来/继任记录拒绝覆盖。
    fn recover_watched(&mut self) -> LanpathResult<()>;
    /// 守护真实 ownership；守护已退出返回 RecoveryRequired（不是恢复证明）。
    fn status(&mut self) -> LanpathResult<SystemProxyOwnership>;
    /// 恢复 + 确认守护退出；超时/断连不是恢复证明。
    fn shutdown(&mut self) -> LanpathResult<()>;
}

/// 守护装配与只读 ownership 查询。`supported()==false`（非 Windows）时
/// 运行时按“未接管”处理，不因已保存偏好失败。
pub(crate) trait GuardianSpawner: Send + Sync {
    fn supported(&self) -> bool;
    fn arm(&self, owner: &str) -> LanpathResult<Box<dyn ArmedGuardian>>;
    fn ownership(&self, owner: &str) -> LanpathResult<SystemProxyOwnership>;
    fn has_recovery_for_owner(&self, owner: &str) -> LanpathResult<bool>;
}

#[cfg(windows)]
pub(crate) struct WindowsGuardianSpawner {
    /// GUI 侧只读会话（status/存在性查询）；变更全部经独立守护进程。
    session: lanpath_client::sysproxy::SystemProxySession,
}

#[cfg(windows)]
impl WindowsGuardianSpawner {
    pub(crate) fn new(owner: &str) -> LanpathResult<Self> {
        Ok(Self {
            session: lanpath_client::sysproxy::SystemProxySession::windows_with_owner(
                APP_ID, owner,
            )?,
        })
    }
}

#[cfg(windows)]
impl GuardianSpawner for WindowsGuardianSpawner {
    fn supported(&self) -> bool {
        true
    }
    fn arm(&self, owner: &str) -> LanpathResult<Box<dyn ArmedGuardian>> {
        Ok(Box::new(crate::proxy_guardian::Guardian::arm(owner)?))
    }
    fn ownership(&self, _owner: &str) -> LanpathResult<SystemProxyOwnership> {
        lanpath_client::sysproxy::SystemProxySession::status(&self.session)
    }
    fn has_recovery_for_owner(&self, owner: &str) -> LanpathResult<bool> {
        lanpath_client::sysproxy::SystemProxySession::has_recovery_for_owner(&self.session, owner)
    }
}

#[cfg(windows)]
impl ArmedGuardian for crate::proxy_guardian::Guardian {
    fn acquire(&mut self, port: u16) -> LanpathResult<()> {
        crate::proxy_guardian::Guardian::acquire(self, port)
    }
    fn acquire_takeover(&mut self, port: u16) -> LanpathResult<()> {
        crate::proxy_guardian::Guardian::acquire_takeover(self, port)
    }
    fn recover_abandoned(&mut self) -> LanpathResult<()> {
        crate::proxy_guardian::Guardian::recover_abandoned(self)
    }
    fn recover_watched(&mut self) -> LanpathResult<()> {
        crate::proxy_guardian::Guardian::recover_watched(self)
    }
    fn status(&mut self) -> LanpathResult<SystemProxyOwnership> {
        crate::proxy_guardian::Guardian::status(self)
    }
    fn shutdown(&mut self) -> LanpathResult<()> {
        crate::proxy_guardian::Guardian::shutdown(self)
    }
}

/// 非 Windows：显式不支持的装配器（`supported()==false`）。
#[cfg(not(windows))]
pub(crate) struct UnsupportedGuardianSpawner;

#[cfg(not(windows))]
impl GuardianSpawner for UnsupportedGuardianSpawner {
    fn supported(&self) -> bool {
        false
    }
    fn arm(&self, _owner: &str) -> LanpathResult<Box<dyn ArmedGuardian>> {
        Err(unsupported())
    }
    fn ownership(&self, _owner: &str) -> LanpathResult<SystemProxyOwnership> {
        Err(unsupported())
    }
    fn has_recovery_for_owner(&self, _owner: &str) -> LanpathResult<bool> {
        Err(unsupported())
    }
}

/// 装配默认 spawner：Windows 为真实守护装配（owner=本数据根 UUID）；
/// 其他平台为显式不支持。返回 `Arc<Mutex<_>>` 以便测试注入 fixture。
#[cfg(windows)]
pub(crate) fn default_spawner(owner: &str) -> LanpathResult<Arc<Mutex<Arc<dyn GuardianSpawner>>>> {
    Ok(Arc::new(Mutex::new(Arc::new(WindowsGuardianSpawner::new(
        owner,
    )?))))
}

#[cfg(not(windows))]
pub(crate) fn default_spawner(_owner: &str) -> LanpathResult<Arc<Mutex<Arc<dyn GuardianSpawner>>>> {
    Ok(Arc::new(Mutex::new(Arc::new(UnsupportedGuardianSpawner))))
}

/// 读已保存的系统代理偏好（`ClientConfig.system_proxy_enabled`）。
pub(crate) fn load_preference(root: &DataRoot) -> LanpathResult<bool> {
    let _guard = root.business_guard();
    Ok(crate::request_settings::load_request_config(root.path())?.system_proxy_enabled)
}

/// 保存系统代理偏好：同一业务锁内读最新 `ClientConfig`，只改
/// `system_proxy_enabled` 一个字段（配置合并），再落盘；active 订阅、
/// 核心选择、端口、请求代理、MITM 等无关字段原样保留。
pub(crate) fn save_preference(root: &DataRoot, enabled: bool) -> LanpathResult<()> {
    let _guard = root.business_guard();
    root.ensure_writable()?;
    let mut config = crate::request_settings::load_request_config(root.path())?;
    config.system_proxy_enabled = enabled;
    config.save()?;
    tracing::info!(target: "lanpath::system_proxy",
        "系统代理偏好已保存：{enabled}");
    Ok(())
}

#[cfg(test)]
#[path = "system_proxy_tests.rs"]
mod tests;
