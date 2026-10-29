//! Process spawning and inspection helpers.
//!
//! 除窗口抑制（[`no_window`] / [`no_window_tokio`]）外，本模块承载核心进程
//! 生命周期原语（ADR-0012）：
//!
//! - [`spawn_guarded`]：父子绑定 spawn——父进程以任何方式死亡（崩溃 /
//!   结束任务 / SIGKILL），子进程由 OS 立即回收（D1）；
//! - [`is_process_alive`] / [`process_exe_path`] / [`kill_process`]：PID 级
//!   进程查询与强杀，供 PID 文件收割使用（D2）；
//! - [`port_owner`]：监听端口占用者反查，供端口冲突诊断使用（D4）。

use std::io;
use std::path::PathBuf;

#[cfg(unix)]
#[path = "unix.rs"]
mod imp;
#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

mod identity;
pub use identity::*;

/// Do not create a console window for the spawned child on Windows
/// (`CREATE_NO_WINDOW`, `0x08000000`); no-op on other platforms.
///
/// The desktop client is a GUI-subsystem app: every child process spawned
/// without this flag (core `run`, `version` probes, `reg` for system proxy, …)
/// creates a visible cmd window — at startup this reads as a console window
/// flashing repeatedly. Apply to **every** child process spawned on behalf of
/// the GUI session.
#[cfg(windows)]
pub fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

/// See [`no_window`] (no-op on non-Windows platforms).
#[cfg(not(windows))]
pub fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd
}

/// Tokio variant of [`no_window`], using its Windows-only inherent method.
#[cfg(windows)]
pub fn no_window_tokio(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

/// See [`no_window_tokio`] (no-op on non-Windows platforms).
#[cfg(not(windows))]
pub fn no_window_tokio(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    cmd
}

/// 父子绑定句柄（ADR-0012 D1）。
///
/// Windows 上持有 Job Object 句柄（`KILL_ON_JOB_CLOSE`），随 guard drop 关闭；
/// Unix 上绑定经 `PR_SET_PDEATHSIG` 在子进程侧生效，guard 为零大小标记。
/// 持有方应与被管理的子进程同生命周期存活。
pub struct LifecycleGuard {
    bound: bool,
    #[cfg(windows)]
    #[allow(dead_code)] // 仅用于持有 Job 句柄（drop 语义），不直接读取
    job: Option<imp::JobGuard>,
}

impl LifecycleGuard {
    /// OS 级父子绑定是否已生效。非 Windows 平台缺少等价机制时可能返回 `false`；
    /// Windows 绑定失败会直接拒绝启动，不会返回一个继续运行的无绑定子进程。
    pub fn is_bound(&self) -> bool {
        self.bound
    }

    pub(crate) fn new(bound: bool, #[cfg(windows)] job: Option<imp::JobGuard>) -> Self {
        Self {
            bound,
            #[cfg(windows)]
            job,
        }
    }
}

/// Spawn 子进程并将其生命周期绑定到本进程：父进程以任何方式死亡
/// （崩溃 / 结束任务 / SIGKILL），子进程由 OS 立即回收。
///
/// - Windows：挂入带 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 的 Job 后才恢复
///   suspended 主线程；任一步失败均关闭 Job 并终止子进程，不会无绑定继续运行。
/// - Linux（含 WSL）/ Android：`pre_exec` 设置 `PR_SET_PDEATHSIG = SIGKILL`，
///   并复核 `getppid()` 兜底 fork/prctl 竞态。
/// - macOS：无等价轻量机制，绑定跳过（`is_bound() == false`）。
///
/// tokio 使用者：对返回的 std Child 取走 stdio 句柄后
/// `tokio::process::Child::from_std` 转换即可。
pub fn spawn_guarded(
    cmd: &mut std::process::Command,
) -> io::Result<(std::process::Child, LifecycleGuard)> {
    #[cfg(windows)]
    {
        imp::spawn_bound(cmd)
    }
    #[cfg(not(windows))]
    {
        imp::configure_pre_spawn(cmd);
        let child = cmd.spawn()?;
        let guard = imp::bind_post_spawn(&child);
        Ok((child, guard))
    }
}

/// Spawn into the caller's private console, with a separately targetable process group.
/// The caller must own that console; unlike `no_window`, this preserves Ctrl-Break delivery.
#[cfg(windows)]
pub fn spawn_guarded_console(
    cmd: &mut std::process::Command,
) -> io::Result<(std::process::Child, LifecycleGuard)> {
    imp::spawn_bound_flags(
        cmd,
        windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP,
    )
}

/// 指定 PID 的进程是否存在（权限不足视为存在，避免误收割）。
pub fn is_process_alive(pid: u32) -> bool {
    imp::is_process_alive(pid)
}

/// 指定 PID 进程的可执行文件完整路径（进程退出或无权限时 `None`）。
pub fn process_exe_path(pid: u32) -> Option<PathBuf> {
    imp::process_exe_path(pid)
}

/// 强制终止指定 PID 的进程（SIGKILL / TerminateProcess）。
///
/// 调用方必须先经 [`process_exe_path`] 校验目标进程身份，防 PID 复用误杀。
pub fn kill_process(pid: u32) -> io::Result<()> {
    imp::kill_process(pid)
}

/// 监听端口占用者（[`port_owner`] 的反查结果）。
#[derive(Debug, Clone)]
pub struct PortOwner {
    pub pid: u32,
    /// 进程名（Windows 为 exe 文件名，Linux 为 comm）。
    pub name: String,
}

/// 反查正在监听指定本地 TCP 端口的进程（best-effort）。
///
/// Linux 解析 `/proc/net/tcp{,6}` + fd 反查；Windows 经
/// `GetExtendedTcpTable`（仅 IPv4）；macOS 不支持（返回 `None`）。
/// 权限不足或查询失败均返回 `None`，调用方应退化为无占用者的通用提示。
pub fn port_owner(port: u16) -> Option<PortOwner> {
    imp::port_owner(port)
}

#[cfg(test)]
mod tests {

    #[test]
    fn no_window_is_applicable_to_tokio_command() {
        let mut cmd = tokio::process::Command::new("sing-box");
        let _ = super::no_window_tokio(&mut cmd);
    }

    #[test]
    fn is_process_alive_self() {
        assert!(super::is_process_alive(std::process::id()));
    }

    #[test]
    fn process_exe_path_self_exists() {
        let path = super::process_exe_path(std::process::id()).expect("self exe path");
        assert!(path.exists(), "self exe path 应存在: {}", path.display());
    }

    #[cfg(unix)]
    #[test]
    fn spawn_guarded_sleep_is_alive_and_bound() {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("30");
        let (mut child, guard) = super::spawn_guarded(&mut cmd).expect("spawn sleep");
        // macOS 无 pdeathsig，绑定跳过；其余 unix 应已绑定。
        assert_eq!(guard.is_bound(), cfg!(not(target_os = "macos")));
        assert!(child.try_wait().unwrap().is_none(), "sleep 应存活");
        child.kill().unwrap();
        let _ = child.wait();
    }

    #[cfg(unix)]
    #[test]
    fn kill_process_terminates_sleep() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();
        super::kill_process(pid).expect("kill sleep");
        let status = child.wait().unwrap();
        assert!(!status.success(), "被 SIGKILL 的进程不应正常退出");
    }

    // 端口反查依赖 /proc（Linux）或 GetExtendedTcpTable（Windows，CI 不跑
    // Windows 测试）；macOS 不支持，跳过。
    #[cfg(target_os = "linux")]
    #[test]
    fn port_owner_finds_own_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let owner = super::port_owner(port).expect("应反查到自身监听");
        assert_eq!(owner.pid, std::process::id());
        assert!(!owner.name.is_empty());
    }
}
