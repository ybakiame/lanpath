//! Mihomo process manager.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use lanpath_common::{CoreType, LanpathError, LanpathResult, LifecycleGuard, ProcessStopKind};
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::RwLock;

use super::path::absolutize;
use super::{CoreManager, first_output_line, record_exit, spawn_output_reader};
use crate::lifecycle;

/// Mihomo process manager.
///
/// The child stays a `std::process::Child` so stop and status checks can reap it
/// deterministically; `lifecycle` supplies OS-level parent/child lifetime binding.
pub struct MihomoProcessManager {
    binary: PathBuf,
    config_dir: PathBuf,
    process: RwLock<Option<std::process::Child>>,
    // Drop the Child before the guard so Windows Job Object cleanup kills any
    // residual child after the owned handle has been released.
    guard: RwLock<Option<LifecycleGuard>>,
    start_time: RwLock<Option<Instant>>,
    last_error: Arc<RwLock<String>>,
}

impl MihomoProcessManager {
    pub fn new(binary: impl AsRef<Path>, config_dir: impl AsRef<Path>) -> LanpathResult<Self> {
        Ok(Self {
            binary: absolutize(binary.as_ref()),
            config_dir: absolutize(config_dir.as_ref()),
            process: RwLock::new(None),
            guard: RwLock::new(None),
            start_time: RwLock::new(None),
            last_error: Arc::new(RwLock::new(String::new())),
        })
    }

    fn config_path(&self) -> PathBuf {
        self.config_dir.join("mihomo.yaml")
    }

    fn pid_path(&self) -> PathBuf {
        self.config_dir.join("mihomo.pid")
    }

    fn spawn_args(&self) -> [OsString; 4] {
        [
            OsStr::new("-d").to_owned(),
            self.config_dir.as_os_str().to_owned(),
            OsStr::new("-f").to_owned(),
            self.config_path().as_os_str().to_owned(),
        ]
    }

    async fn write_config(&self, config: &Value) -> LanpathResult<PathBuf> {
        let config_path = self.config_path();
        let yaml = serde_yaml::to_string(config).map_err(|error| {
            LanpathError::Core(format!("failed to serialize mihomo config: {error}"))
        })?;
        if let Err(e) = tokio::fs::write(&config_path, yaml).await {
            let msg = format!("failed to write mihomo config: {e}");
            *self.last_error.write().await = msg.clone();
            return Err(LanpathError::Core(msg));
        }
        Ok(config_path)
    }

    /// Stop the owned child or recover its exact recorded identity.
    ///
    /// Cleanup failures retain ownership and return errors, matching sing-box
    /// lifecycle semantics on Unix and Windows.
    pub async fn stop_process(&self) -> LanpathResult<ProcessStopKind> {
        let mut proc = self.process.write().await;
        let kind = if let Some(child) = proc.as_mut() {
            #[cfg(windows)]
            let kind = {
                use std::os::windows::io::AsHandle;
                lanpath_common::WindowsProcess::duplicate(child.as_handle())?
                    .terminate_and_wait(10_000)?
            };
            #[cfg(not(windows))]
            let kind = match child.try_wait()? {
                Some(_) => ProcessStopKind::AlreadyStopped,
                None => {
                    child.kill()?;
                    ProcessStopKind::Forced
                }
            };
            child.wait()?;
            lifecycle::remove_pid_file(&self.pid_path()).map_err(LanpathError::Core)?;
            *proc = None;
            tracing::info!(?kind, "mihomo stopped");
            kind
        } else {
            lifecycle::reap_stale(&self.pid_path()).map_err(LanpathError::Core)?
        };
        *self.start_time.write().await = None;
        *self.guard.write().await = None;
        Ok(kind)
    }
}

#[async_trait::async_trait]
impl CoreManager for MihomoProcessManager {
    fn core_type(&self) -> CoreType {
        CoreType::Mihomo
    }

    async fn start(&self, config: &Value) -> LanpathResult<()> {
        if !tokio::fs::try_exists(&self.binary).await.unwrap_or(false) {
            let msg = format!("mihomo binary not found at {}", self.binary.display());
            *self.last_error.write().await = msg.clone();
            return Err(LanpathError::Core(msg));
        }

        let mut proc = self.process.write().await;
        if proc.is_some() {
            return Err(LanpathError::Core("mihomo already running".into()));
        }

        let config_path = self.write_config(config).await?;

        // Recover only the exact process identity recorded by the previous run.
        lifecycle::reap_stale(&self.pid_path()).map_err(LanpathError::Core)?;

        let spawn = match lifecycle::spawn_core("mihomo", &self.binary, self.spawn_args()) {
            Ok(s) => s,
            Err(msg) => {
                *self.last_error.write().await = msg.clone();
                return Err(LanpathError::Core(msg));
            }
        };

        let mut child = spawn.child;
        let last_error = self.last_error.clone();
        *self.last_error.write().await = String::new();
        if let Some(stderr) = spawn.stderr {
            spawn_output_reader(stderr, last_error.clone());
        }
        if let Some(stdout) = spawn.stdout {
            spawn_output_reader(stdout, last_error);
        }

        // Capture configuration errors reported immediately after spawn.
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(-1);
                record_exit(self.last_error.clone(), code).await;
                let err = self.last_error.read().await.clone();
                tracing::error!("mihomo exited immediately: {err}");
                Err(LanpathError::Core(err))
            }
            Ok(None) => {
                if let Err(error) =
                    lifecycle::write_pid_file(&self.pid_path(), child.id(), &self.binary)
                {
                    let stopped = child.kill().and_then(|()| child.wait().map(|_| ()));
                    if let Err(stop_error) = stopped {
                        *self.guard.write().await = Some(spawn.guard);
                        *proc = Some(child);
                        return Err(LanpathError::Core(format!(
                            "identity record failed: {error}; stop unresolved: {stop_error}"
                        )));
                    }
                    return Err(LanpathError::Core(format!(
                        "identity record failed: {error}"
                    )));
                }
                *self.guard.write().await = Some(spawn.guard);
                *proc = Some(child);
                *self.start_time.write().await = Some(Instant::now());
                tracing::info!("mihomo started with config {}", config_path.display());
                Ok(())
            }
            Err(e) => {
                let msg = format!("failed to check mihomo status after start: {e}");
                *self.last_error.write().await = msg.clone();
                Err(LanpathError::Core(msg))
            }
        }
    }

    async fn stop(&self) -> LanpathResult<()> {
        self.stop_process().await.map(|_| ())
    }

    async fn restart(&self, config: &Value) -> LanpathResult<()> {
        self.stop().await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        self.start(config).await
    }

    async fn is_running(&self) -> bool {
        let mut proc = self.process.write().await;
        if let Some(child) = proc.as_mut() {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(status)) => {
                    let code = status.code().unwrap_or(-1);
                    record_exit(self.last_error.clone(), code).await;
                    *proc = None;
                    *self.guard.write().await = None;
                    if let Err(error) = lifecycle::remove_pid_file(&self.pid_path()) {
                        *self.last_error.write().await = error;
                    }
                    false
                }
                Err(e) => {
                    *self.last_error.write().await = format!("failed to check process status: {e}");
                    false
                }
            }
        } else {
            false
        }
    }

    async fn reload(&self, config: &Value) -> LanpathResult<()> {
        // Mihomo has no portable CLI reload command. Restarting writes and applies
        // the complete configuration without requiring an external-controller API.
        self.restart(config).await
    }

    async fn version(&self) -> LanpathResult<String> {
        let output = lanpath_common::no_window_tokio(Command::new(&self.binary).arg("-v"))
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(first_output_line(&stdout))
    }

    async fn uptime_secs(&self) -> LanpathResult<u64> {
        Ok(self
            .start_time
            .read()
            .await
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0))
    }

    async fn active_inbounds(&self) -> LanpathResult<Vec<String>> {
        Ok(Vec::new())
    }

    async fn last_error(&self) -> LanpathResult<String> {
        Ok(self.last_error.read().await.clone())
    }

    fn reap_stale(&self) {
        if let Err(error) = lifecycle::reap_stale(&self.pid_path()) {
            tracing::error!(%error, "core recovery remains unresolved");
        }
    }
}

#[cfg(test)]
#[path = "mihomo_tests.rs"]
mod tests;
