//! 核心进程生命周期辅助（ADR-0012 D1/D2）。
//!
//! - [`spawn_core`]：经 `lanpath_common::spawn_guarded` spawn 核心子进程并完成
//!   std → tokio 转换（OS 级父子绑定：父死子亡）；
//! - PID 文件（`<config_dir>/<core>.pid`）：`start` 成功写入、`stop` 删除、
//!   `start` 前 [`reap_stale`] 收割上次实例遗留的孤儿进程。

use std::ffi::OsStr;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use lanpath_common::{LifecycleGuard, ProcessStopKind};
use tokio::process::{ChildStderr, ChildStdout};

/// Persist the original OS identity; changing the selected binary cannot redefine ownership.
#[derive(serde::Serialize, serde::Deserialize)]
struct PidFile {
    identity: lanpath_common::ProcessIdentity,
}

/// 核心 spawn 结果：std 子进程 + 父子绑定 guard + tokio 化的 stdio。
///
/// 子进程保持 std 形态：tokio 不提供 `Child::from_std`（无法接管 std 子进程
/// 的 reap 集成）；std Child 的 kill/try_wait/id 均为同步 API，管理器直接
/// 使用即可，stdio 则经 `ChildStdout::from_std` / `ChildStderr::from_std`
/// 转为 tokio 句柄供异步日志读取。
pub struct CoreSpawn {
    pub child: std::process::Child,
    pub guard: LifecycleGuard,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

/// Spawn 核心子进程并绑定其生命周期到本进程（ADR-0012 D1），完成
/// std → tokio 的 Child / stdio 转换。`core` 为错误信息中的核心名。
pub fn spawn_core<I, S>(core: &str, binary: &Path, args: I) -> Result<CoreSpawn, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = std::process::Command::new(binary);
    cmd.args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    lanpath_common::no_window(&mut cmd);
    let (mut std_child, guard) = lanpath_common::spawn_guarded(&mut cmd)
        .map_err(|e| format!("failed to spawn {core}: {e}"))?;
    if !guard.is_bound() {
        tracing::warn!("核心进程父子绑定未生效；依赖自有 Child 停止，遗留进程恢复受平台支持限制");
    }
    // 先从 std Child 取出 stdio，再分别 tokio 化。
    let stdout = std_child.stdout.take();
    let stderr = std_child.stderr.take();
    let stdout = stdout
        .map(ChildStdout::from_std)
        .transpose()
        .map_err(|e| format!("failed to convert stdout: {e}"))?;
    let stderr = stderr
        .map(ChildStderr::from_std)
        .transpose()
        .map_err(|e| format!("failed to convert stderr: {e}"))?;
    Ok(CoreSpawn {
        child: std_child,
        guard,
        stdout,
        stderr,
    })
}

/// Recover only the recorded original process and report the operation after record cleanup.
/// Unknown/mismatched identities and failed operations retain the record.
pub fn reap_stale(pid_path: &Path) -> Result<ProcessStopKind, String> {
    let content = match std::fs::read(pid_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProcessStopKind::AlreadyStopped);
        }
        Err(e) => return Err(format!("cannot read recovery record: {e}")),
    };
    let record: PidFile = serde_json::from_slice(&content)
        .map_err(|e| format!("invalid recovery identity; record retained: {e}"))?;
    #[cfg(windows)]
    let kind = match record.identity.terminate_verified() {
        Ok(kind) => kind,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(87) => {
            ProcessStopKind::AlreadyStopped
        }
        Err(e) => return Err(format!("core recovery unresolved; record retained: {e}")),
    };
    // Unix capture supplies the platform's absent-process classification.
    #[cfg(not(windows))]
    let kind = match lanpath_common::ProcessIdentity::capture(record.identity.pid) {
        Ok(actual) if actual == record.identity => record
            .identity
            .terminate_verified()
            .map_err(|e| format!("core recovery unresolved; record retained: {e}"))?,
        Ok(_) => return Err("process identity mismatch; recovery record retained".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(87) => {
            ProcessStopKind::AlreadyStopped
        }
        Err(e) => {
            return Err(format!(
                "cannot inspect original process; record retained: {e}"
            ));
        }
    };
    std::fs::remove_file(pid_path)
        .map_err(|e| format!("cannot remove resolved recovery record: {e}"))?;
    Ok(kind)
}

pub fn write_pid_file(pid_path: &Path, pid: u32, binary: &Path) -> Result<(), String> {
    let identity = lanpath_common::ProcessIdentity::capture(pid).map_err(|e| e.to_string())?;
    let expected = binary.canonicalize().map_err(|e| e.to_string())?;
    let actual = identity.exe.canonicalize().map_err(|e| e.to_string())?;
    if !paths_match(&expected, &actual) {
        return Err("spawned process image mismatch".into());
    }
    let bytes = serde_json::to_vec(&PidFile { identity }).map_err(|e| e.to_string())?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(pid_path)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}

fn paths_match(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

/// Remove an ownership record only after the original process is known to have exited.
pub fn remove_pid_file(pid_path: &Path) -> Result<(), String> {
    match std::fs::remove_file(pid_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove resolved process record: {e}")),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Re-execute this test binary so the fixture has a stable native identity,
    /// independent of PATH, scripts, multicall coreutils or host wrappers.
    pub(crate) fn fixture_child() -> (std::process::Child, u32) {
        let child = std::process::Command::new(fixture_binary())
            .args([
                "--exact",
                "lifecycle::tests::orphan_child_fixture",
                "--nocapture",
            ])
            .env("LANPATH_LIFECYCLE_CHILD_FIXTURE", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn lifecycle fixture");
        let pid = child.id();
        (child, pid)
    }

    pub(crate) fn fixture_binary() -> PathBuf {
        std::env::current_exe().expect("current test binary")
    }

    #[test]
    fn orphan_child_fixture() {
        if std::env::var_os("LANPATH_LIFECYCLE_CHILD_FIXTURE").is_some() {
            use std::io::Read;
            let _ = std::io::stdin().read_exact(&mut [0]);
        }
    }

    #[test]
    fn reap_stale_no_pidfile_is_noop() {
        let dir = std::env::temp_dir().join(format!("pp-test-{}", std::process::id()));
        let pid_path = dir.join("sing-box.pid");
        assert_eq!(
            reap_stale(&pid_path).unwrap(),
            ProcessStopKind::AlreadyStopped
        );
        assert!(!pid_path.exists());
    }

    #[cfg(any(windows, target_os = "linux", target_os = "android"))]
    #[test]
    fn write_and_reap_kills_matching_orphan() {
        let dir = std::env::temp_dir().join(format!("pp-test-reap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_path = dir.join("sing-box.pid");
        let binary = fixture_binary();

        let (mut child, pid) = fixture_child();
        assert_eq!(
            lanpath_common::ProcessIdentity::capture(pid).unwrap().exe,
            binary
        );
        write_pid_file(&pid_path, pid, &binary).unwrap();
        assert!(pid_path.exists());
        assert!(lanpath_common::is_process_alive(pid));

        assert_eq!(reap_stale(&pid_path).unwrap(), ProcessStopKind::Forced);

        assert!(!pid_path.exists(), "收割后 pidfile 应删除");
        let status = child.wait().unwrap();
        assert!(!status.success(), "遗留进程应被强杀");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn live_orphan_recovery_is_unsupported_but_owned_child_can_stop() {
        let dir = std::env::temp_dir().join(format!("pp-test-macos-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_path = dir.join("sing-box.pid");
        let binary = fixture_binary();
        let (mut child, pid) = fixture_child();
        let identity = lanpath_common::ProcessIdentity::capture(pid).unwrap();
        assert_eq!(identity.pid, pid);
        assert_eq!(identity.exe, binary);
        write_pid_file(&pid_path, pid, &binary).unwrap();
        let record = std::fs::read(&pid_path).unwrap();

        assert_eq!(
            identity.terminate_verified().unwrap_err().kind(),
            std::io::ErrorKind::Unsupported
        );
        let error = reap_stale(&pid_path).unwrap_err();
        assert!(error.contains("core recovery unresolved; record retained"));
        assert_eq!(std::fs::read(&pid_path).unwrap(), record);
        assert!(child.try_wait().unwrap().is_none());

        // The original Child remains authority for ordinary owned-child stop.
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(
            reap_stale(&pid_path).unwrap(),
            ProcessStopKind::AlreadyStopped
        );
        assert!(!pid_path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn reap_stale_skips_pid_reuse() {
        // A PID reused by the exact same executable is still a different process.
        let dir = std::env::temp_dir().join(format!("pp-test-reuse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_path = dir.join("sing-box.pid");

        let (mut child, pid) = fixture_child();
        write_pid_file(&pid_path, pid, &fixture_binary()).unwrap();
        let mut record: PidFile =
            serde_json::from_slice(&std::fs::read(&pid_path).unwrap()).unwrap();
        record.identity.created += 1;
        let mismatched_record = serde_json::to_vec(&record).unwrap();
        std::fs::write(&pid_path, &mismatched_record).unwrap();
        assert!(reap_stale(&pid_path).is_err());
        assert!(pid_path.exists(), "mismatched record must be retained");
        assert_eq!(std::fs::read(&pid_path).unwrap(), mismatched_record);
        assert!(
            lanpath_common::is_process_alive(pid),
            "创建身份不匹配时不得误杀"
        );
        child.kill().unwrap();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn reap_stale_removes_dead_pidfile() {
        let dir = std::env::temp_dir().join(format!("pp-test-dead-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_path = dir.join("sing-box.pid");
        let binary = fixture_binary();

        let (mut child, pid) = fixture_child();
        write_pid_file(&pid_path, pid, &binary).unwrap();
        child.kill().unwrap();
        let _ = child.wait();

        assert_eq!(
            reap_stale(&pid_path).unwrap(),
            ProcessStopKind::AlreadyStopped
        );
        assert!(!pid_path.exists(), "进程已死时 pidfile 应清理");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reap_stale_cleanup_failure_retains_record_and_returns_error() {
        use std::os::fd::AsRawFd;
        let dir = std::env::temp_dir().join(format!("pp-test-cleanup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_path = dir.join("sing-box.pid");
        let (mut child, pid) = fixture_child();
        write_pid_file(&pid_path, pid, &fixture_binary()).unwrap();
        let record = std::fs::read(&pid_path).unwrap();
        let file = std::fs::File::open(&pid_path).unwrap();
        // procfs permits reading the real record through this descriptor but forbids
        // unlinking its entry, even for root; no permission or timing races are needed.
        let unremovable_path = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
        let result = reap_stale(&unremovable_path);
        if result.is_err() && child.try_wait().unwrap().is_none() {
            let _ = child.kill();
        }
        let status = child.wait().unwrap();
        let error = result.unwrap_err();

        assert!(error.contains("cannot remove resolved recovery record"));
        assert!(!status.success());
        assert_eq!(std::fs::read(&pid_path).unwrap(), record);
        assert_eq!(std::fs::read(&unremovable_path).unwrap(), record);
        drop(file);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
