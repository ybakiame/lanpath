//! Windows 平台进程原语实现。
//!
//! 句柄类型说明：windows-sys 0.61 中 `HANDLE` 为 `*mut c_void`，空句柄以
//! `.is_null()` 判断。

use std::io;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::AF_INET;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, QueryFullProcessImageNameW,
    TerminateProcess,
};

/// 持有 Job Object 句柄的 guard；drop 时关闭句柄。
///
/// Job 配置了 `KILL_ON_JOB_CLOSE`：最后一个句柄关闭（含父进程死亡时内核
/// 自动关闭）即终止 Job 内所有进程，因此 guard 必须与管理的子进程同生命周期。
pub struct JobGuard(HANDLE);

impl Drop for JobGuard {
    fn drop(&mut self) {
        // SAFETY: self.0 为 CreateJobObjectW 返回的有效句柄，仅在此关闭一次。
        unsafe {
            CloseHandle(self.0);
        }
    }
}

// SAFETY: Job 句柄是进程内资源，guard 仅持有句柄供 drop 关闭，跨线程移动/共享安全。
unsafe impl Send for JobGuard {}
unsafe impl Sync for JobGuard {}

/// Start suspended, contain before the executable can run, then resume its only thread.
pub fn spawn_bound(
    cmd: &mut std::process::Command,
) -> io::Result<(std::process::Child, super::LifecycleGuard)> {
    spawn_bound_flags(cmd, windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
}

pub fn spawn_bound_flags(
    cmd: &mut std::process::Command,
    flags: u32,
) -> io::Result<(std::process::Child, super::LifecycleGuard)> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::{Diagnostics::ToolHelp::*, Threading::*};
    cmd.creation_flags(CREATE_SUSPENDED | flags);
    let mut child = cmd.spawn()?;
    let result = (|| {
        let job = bind_to_job(&child)?;
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        let mut threads = Vec::new();
        let mut more = unsafe { Thread32First(snapshot, &mut entry) };
        while more != 0 {
            if entry.th32OwnerProcessID == child.id() {
                threads.push(entry.th32ThreadID);
            }
            more = unsafe { Thread32Next(snapshot, &mut entry) };
        }
        unsafe {
            CloseHandle(snapshot);
        }
        if threads.len() != 1 {
            return Err(io::Error::other(
                "suspended process has ambiguous primary thread",
            ));
        }
        let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, threads[0]) };
        if thread.is_null() {
            return Err(io::Error::last_os_error());
        }
        let previous = unsafe { ResumeThread(thread) };
        unsafe {
            CloseHandle(thread);
        }
        if previous != 1 {
            return Err(io::Error::other("failed to resume contained process"));
        }
        Ok(super::LifecycleGuard::new(true, Some(job)))
    })();
    match result {
        Ok(guard) => Ok((child, guard)),
        Err(error) => {
            let killed = child.kill();
            let waited = child.wait();
            if killed.is_err() || waited.is_err() {
                return Err(io::Error::other(format!(
                    "containment failed: {error}; termination unresolved"
                )));
            }
            Err(error)
        }
    }
}

fn bind_to_job(child: &std::process::Child) -> io::Result<JobGuard> {
    use std::os::windows::io::AsRawHandle;
    unsafe {
        // SAFETY: 全空默认属性的匿名 Job；错误路径均关闭句柄。
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &raw const info as *const core::ffi::c_void,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if ok == 0 {
            let e = io::Error::last_os_error();
            CloseHandle(job);
            return Err(e);
        }
        if AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) == 0 {
            let e = io::Error::last_os_error();
            CloseHandle(job);
            return Err(e);
        }
        Ok(JobGuard(job))
    }
}

/// `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` 探测进程是否存在。
pub fn is_process_alive(pid: u32) -> bool {
    unsafe {
        // SAFETY: 仅查询打开随即关闭。
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | 0x0010_0000, 0, pid);
        if h.is_null() {
            return io::Error::last_os_error().raw_os_error() != Some(87);
        }
        let state = windows_sys::Win32::System::Threading::WaitForSingleObject(h, 0);
        CloseHandle(h);
        state != windows_sys::Win32::Foundation::WAIT_OBJECT_0
    }
}

/// `QueryFullProcessImageNameW` 取进程可执行文件完整路径。
pub fn process_exe_path(pid: u32) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    unsafe {
        // SAFETY: 查询打开；缓冲区定长，len 为 in/out 参数。
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        buf.truncate(len as usize);
        Some(PathBuf::from(std::ffi::OsString::from_wide(&buf)))
    }
}

/// `TerminateProcess` 强杀。调用方须先经 [`super::process_exe_path`] 校验身份。
pub fn kill_process(pid: u32) -> io::Result<()> {
    unsafe {
        // SAFETY: 终止后关闭句柄；失败透传 OS 错误。
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if h.is_null() {
            return Err(io::Error::last_os_error());
        }
        let ok = TerminateProcess(h, 1);
        CloseHandle(h);
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// `GetExtendedTcpTable`（仅 IPv4，127.0.0.1 绑定的 Clash API / mixed 入站
/// 均为 IPv4）反查监听端口 PID，进程名取 exe 文件名。
pub fn port_owner(port: u16) -> Option<super::PortOwner> {
    let pid = tcp_listener_pid(port)?;
    let name = process_exe_path(pid)
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "<unknown>".into());
    Some(super::PortOwner { pid, name })
}

fn tcp_listener_pid(port: u16) -> Option<u32> {
    unsafe {
        // SAFETY: 首次调用取所需缓冲区大小（ERROR_INSUFFICIENT_BUFFER），
        // 二次调用写入堆缓冲区后按表结构读取。
        let mut size: u32 = 0;
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let ret = GetExtendedTcpTable(
            buf.as_mut_ptr().cast(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
        if ret != 0 {
            return None;
        }
        let table = buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID;
        let num = (*table).dwNumEntries as isize;
        let rows = (*table).table.as_ptr();
        for i in 0..num {
            let row = &*rows.offset(i);
            // dwLocalPort 低 16 位为网络字节序端口。
            if u16::from_be((row.dwLocalPort & 0xFFFF) as u16) == port {
                return Some(row.dwOwningPid);
            }
        }
        None
    }
}
