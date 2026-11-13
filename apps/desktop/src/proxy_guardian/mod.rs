//! 独立普通权限系统代理守护（仅 Windows）：GUI 退出后仍能完成 WinINet
//! 精确恢复的同映像 worker。从 apps/gpui 的 `proxy_guardian` 迁移语义，
//! 不简化安全校验：有界版本化参数协议、命名管道身份核验（SID/session/
//! logon/elevation/映像/进程创建身份）、独立于 Job 与核心进程。
//! 守护不安装服务/启动项，不经 TUN helper，TUN 仍拒绝。
#[cfg(windows)]
mod launch;
#[cfg(windows)]
mod pipe;
#[cfg(any(windows, test))]
mod protocol;
#[cfg(any(windows, test))]
mod rpc;
#[cfg(windows)]
mod security;
#[cfg(any(windows, test))]
mod service;
#[cfg(any(windows, test))]
mod session;
#[cfg(windows)]
mod windows;
#[cfg(any(windows, test))]
mod wire;

#[cfg(windows)]
pub(crate) use launch::Guardian;

/// 管道名前缀：NSH 桌面独立身份（区别于 GPUI 的 `LanpathGPUI-Proxy-`）。
#[cfg(any(windows, test))]
pub(super) const PIPE_PREFIX: &str = r"\\.\pipe\LanpathNSH-Proxy-";

/// Called by the binary before entering any NSH/GUI/storage/logger startup path.
/// A failed/refused recovery leaves the protected journal for explicit recovery.
pub fn dispatch_from_env() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let command = args.next()?;
    if command != "--proxy-guardian" {
        return None;
    }
    #[cfg(windows)]
    {
        let args: Vec<_> = std::iter::once(command).chain(args).collect();
        Some(
            match protocol::Arguments::parse(&args).and_then(windows::watch) {
                Ok(()) => 0,
                Err(_) => 6,
            },
        )
    }
    #[cfg(not(windows))]
    {
        // 非 Windows 明确不支持：不留新 GUI，非 0 退出。
        Some(6)
    }
}
