#![cfg_attr(not(windows), allow(dead_code))]

use lanpath_client::sysproxy::{AcquisitionPolicy, SystemProxyOwnership};
use lanpath_common::LanpathResult;

/// 守护持有的系统代理会话操作面（守护进程内唯一持有变更/检查点的 owner）。
/// Windows 下由共享 `lanpath_client::sysproxy::SystemProxySession` 实现；
/// 测试用 fake 实现按同一契约建模启动/停止/异常路径——它是 fixture，
/// 不是 Windows 真实 WinINet。
pub(super) trait GuardianSession {
    /// 共享 `acquire_with_policy` 直通：普通请求恒为 RequireDirect，
    /// 显式接管授权（用户一次性确认）才使用 ExplicitTakeover。
    fn acquire(&self, port: u16, policy: AcquisitionPolicy) -> LanpathResult<()>;
    fn status(&self) -> LanpathResult<SystemProxyOwnership>;
    fn restore(&self) -> LanpathResult<()>;
    fn recover_abandoned(&self) -> LanpathResult<bool>;
    /// 只恢复指定 owner 的已认证日志；缺失或继任日志不是写入授权。
    fn recover_watched(&self, owner: &str) -> LanpathResult<bool>;
    /// 守护被授权操作的精确日志身份（GUI 侧为本数据根 UUID）。
    fn owner_id(&self) -> &str;
}

#[cfg(windows)]
impl GuardianSession for lanpath_client::sysproxy::SystemProxySession {
    fn acquire(&self, port: u16, policy: AcquisitionPolicy) -> LanpathResult<()> {
        lanpath_client::sysproxy::SystemProxySession::acquire_with_policy(
            self,
            (std::net::Ipv4Addr::LOCALHOST, port).into(),
            policy,
        )
    }
    fn status(&self) -> LanpathResult<SystemProxyOwnership> {
        lanpath_client::sysproxy::SystemProxySession::status(self)
    }
    fn restore(&self) -> LanpathResult<()> {
        lanpath_client::sysproxy::SystemProxySession::restore(self)
    }
    fn recover_abandoned(&self) -> LanpathResult<bool> {
        lanpath_client::sysproxy::SystemProxySession::recover_abandoned(self)
    }
    fn recover_watched(&self, owner: &str) -> LanpathResult<bool> {
        lanpath_client::sysproxy::SystemProxySession::recover_abandoned_owner(self, owner)
    }
    fn owner_id(&self) -> &str {
        lanpath_client::sysproxy::SystemProxySession::owner_id(self)
    }
}
