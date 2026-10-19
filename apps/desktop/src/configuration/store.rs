use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use lanpath_common::{LanpathError, LanpathResult};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use super::{Bindings, policy, resources};

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
const DIAGNOSTIC_LIMIT: usize = 32 * 1024;

pub(crate) struct ConfigStore {
    root: PathBuf,
    binary: Option<PathBuf>,
    bindings: Bindings,
}

impl ConfigStore {
    /// The caller retains DataRoot's directory lock and supplies the same
    /// canonical root and selected executable to the engine manager.
    pub(crate) fn new(root: &Path, binary: Option<PathBuf>, bindings: Bindings) -> Self {
        Self {
            root: root.to_owned(),
            binary,
            bindings,
        }
    }

    pub(crate) fn starter_text(&self) -> LanpathResult<String> {
        Ok(serde_json::to_string_pretty(&json!({
            "log": { "level": "info", "timestamp": true },
            "inbounds": [{
                "type": "mixed", "tag": "mixed-in",
                "listen": "127.0.0.1", "listen_port": self.bindings.mixed_port
            }],
            "outbounds": [
                { "type": "selector", "tag": "proxy", "outbounds": ["direct"], "default": "direct" },
                { "type": "direct", "tag": "direct" }
            ],
            "route": { "final": "proxy" },
            "experimental": { "clash_api": {
                "external_controller": format!("127.0.0.1:{}", self.bindings.api_port),
                "default_mode": "rule"
            }}
        }))?)
    }

    /// Load even invalid JSON exactly as saved, so the editor can repair it.
    pub(crate) async fn load(&self) -> LanpathResult<Option<String>> {
        let path = resources::source_path(&self.root)?;
        match tokio::fs::read_to_string(path).await {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) async fn validate(&self, text: &str) -> LanpathResult<()> {
        self.checked_value(text).await?;
        Ok(())
    }

    pub(crate) async fn save(&self, text: &str) -> LanpathResult<()> {
        self.checked_value(text).await?;
        let path = resources::source_path(&self.root)?;
        // Same private transaction pattern as lanpath-client/backup/transaction:
        // a unique sibling file, write_all, sync_all, then atomic persist.
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        file.write_all(text.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    pub(crate) async fn for_start(&self) -> LanpathResult<Value> {
        let text = self.load().await?.ok_or_else(|| {
            resources::denied(
                "configuration.json",
                "no saved configuration; save and validate the document before starting",
            )
        })?;
        self.checked_value(&text).await
    }

    async fn checked_value(&self, text: &str) -> LanpathResult<Value> {
        // Go's JSON decoder may merge duplicate objects, whereas serde_json
        // keeps the last value. Reject duplicates before either parser can
        // interpret a different authorization boundary.
        let value = super::unique_json::parse(text)?;
        policy::check(&self.root, &value, self.bindings)?;
        let binary = self.binary.as_deref().ok_or_else(|| {
            LanpathError::Core("no sing-box binary selected; install/select the real core before checking or saving".into())
        })?;
        if !binary.is_absolute() || !self.root.is_absolute() {
            return Err(resources::denied(
                "core",
                "checker and engine require an absolute binary path and data root",
            ));
        }
        let canonical_binary = std::fs::canonicalize(binary).map_err(|error| {
            LanpathError::Core(format!(
                "cannot access selected sing-box binary {}: {error}",
                binary.display()
            ))
        })?;
        for (pointer, location) in [
            ("/log/output", "log.output"),
            (
                "/experimental/cache_file/path",
                "experimental.cache_file.path",
            ),
        ] {
            if let Some(path) = value
                .pointer(pointer)
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                && resources::confined(&self.root, path, location)? == canonical_binary
            {
                return Err(resources::denied(
                    location,
                    "output must not overwrite the selected core executable",
                ));
            }
        }
        self.check_core(binary, text).await?;
        Ok(value)
    }

    async fn check_core(&self, binary: &Path, text: &str) -> LanpathResult<()> {
        let mut candidate = tempfile::Builder::new()
            .prefix(".configuration-check-")
            .suffix(".json")
            .tempfile_in(&self.root)?;
        candidate.write_all(text.as_bytes())?;
        candidate.as_file().sync_all()?;
        let mut command = Command::new(binary);
        command
            .arg("check")
            .arg("-c")
            .arg(candidate.path())
            .arg("-D")
            .arg(&self.root)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = lanpath_common::no_window_tokio(&mut command)
            .spawn()
            .map_err(|error| {
                LanpathError::Core(format!("cannot start selected sing-box checker: {error}"))
            })?;
        let (Some(output), Some(errors)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(LanpathError::Core(
                "checker diagnostic pipes are unavailable".into(),
            ));
        };
        let mut stdout = tokio::spawn(drain(output));
        let mut stderr = tokio::spawn(drain(errors));
        let result = tokio::time::timeout(CHECK_TIMEOUT, async {
            let status = child.wait().await?;
            let out = (&mut stdout).await.map_err(std::io::Error::other)??;
            let err = (&mut stderr).await.map_err(std::io::Error::other)??;
            Ok::<_, std::io::Error>((status, out, err))
        })
        .await;
        match result {
            Ok(Ok((status, _, _))) if status.success() => Ok(()),
            Ok(Ok((status, out, err))) => Err(LanpathError::Core(format!(
                "sing-box check failed ({status}):\n{}\n{}",
                String::from_utf8_lossy(&err),
                String::from_utf8_lossy(&out)
            ))),
            Ok(Err(error)) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                stdout.abort();
                stderr.abort();
                Err(LanpathError::Core(format!(
                    "sing-box check could not complete: {error}"
                )))
            }
            Err(_) => {
                // kill() waits for exit; wait() also reaps when a race meant
                // the checker had already exited. Keep the candidate alive
                // until the checker can no longer read it.
                let killed = child.kill().await;
                let reaped = child.wait().await;
                stdout.abort();
                stderr.abort();
                Err(LanpathError::Core(format!(
                    "sing-box check timed out after {} seconds (kill: {killed:?}, reap: {reaped:?})",
                    CHECK_TIMEOUT.as_secs()
                )))
            }
        }
    }
}

/// Drain both pipes fully to avoid deadlocks, but retain bounded diagnostics.
async fn drain(mut stream: impl AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut diagnostic = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Ok(diagnostic);
        }
        let retained = count.min(DIAGNOSTIC_LIMIT.saturating_sub(diagnostic.len()));
        diagnostic.extend_from_slice(&buffer[..retained]);
    }
}
