//! Framework-neutral access to existing plaintext disk logs.
//!
//! Callers own the data-root policy. This service never deletes source logs;
//! exports concatenate their actual bytes, in filename order, without encryption.

use lanpath_common::{LanpathError, LanpathResult};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub const LOG_TAIL_MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const LOG_TAIL_MAX_LINES: u32 = 1000;

#[derive(Clone, Debug)]
pub struct DiskLogService {
    logs_dir: PathBuf,
    core_log: bool,
}

impl DiskLogService {
    /// Uses the historical Tauri viewable-file set by default.
    pub fn new(logs_dir: impl Into<PathBuf>) -> Self {
        Self {
            logs_dir: logs_dir.into(),
            core_log: false,
        }
    }

    /// Includes the experimental desktop's persisted core stream in the viewer.
    pub fn with_core_log(mut self) -> Self {
        self.core_log = true;
        self
    }

    pub fn list_files(&self) -> LanpathResult<Vec<String>> {
        let mut names = self.names(false)?;
        names.reverse();
        Ok(names)
    }

    pub fn read_tail(&self, name: &str, max_lines: Option<u32>) -> LanpathResult<String> {
        self.validate_name(name)?;
        let bytes = read_tail_bytes(&self.logs_dir.join(name), LOG_TAIL_MAX_BYTES)?;
        let content = String::from_utf8_lossy(&bytes);
        let max_lines = max_lines
            .unwrap_or(LOG_TAIL_MAX_LINES)
            .clamp(1, LOG_TAIL_MAX_LINES) as usize;
        let mut lines: Vec<&str> = content.lines().rev().take(max_lines).collect();
        lines.reverse();
        Ok(lines.join("\n"))
    }

    /// Concatenates all app.log* and *.log sources (except export-*), as Tauri did.
    pub fn export_to(&self, export_dir: &Path) -> LanpathResult<PathBuf> {
        ensure_directory(&self.logs_dir)?;
        let files = self.export_files()?;
        if !export_dir.exists() {
            let parent = export_dir
                .parent()
                .ok_or_else(|| LanpathError::Config("导出目录没有父目录".into()))?;
            ensure_directory(parent)?;
            fs::create_dir(export_dir)?;
        }
        ensure_directory(export_dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let (path, mut out) = (0..1000)
            .find_map(|ix| {
                let suffix = if ix == 0 {
                    String::new()
                } else {
                    format!("-{ix}")
                };
                let path = export_dir.join(format!("export-{stamp}{suffix}.log"));
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(file) => Some(Ok((path, file))),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(e) => Some(Err(LanpathError::Io(e))),
                }
            })
            .ok_or_else(|| LanpathError::Config("创建导出文件失败：同名导出过多".into()))??;
        let result = (|| {
            for file in files {
                let mut source = open_regular(&file)?;
                // Snapshot the length so a continuously growing log cannot keep exporting forever.
                let len = source.metadata()?.len();
                std::io::copy(&mut (&mut source).take(len), &mut out)?;
            }
            out.flush().map_err(LanpathError::Io)
        })();
        if let Err(error) = result {
            drop(out);
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        Ok(path)
    }

    pub fn export_files(&self) -> LanpathResult<Vec<PathBuf>> {
        Ok(self
            .names(true)?
            .into_iter()
            .map(|name| self.logs_dir.join(name))
            .collect())
    }

    fn validate_name(&self, name: &str) -> LanpathResult<()> {
        validate_log_file_name(name, self.core_log)
    }

    fn names(&self, exporting: bool) -> LanpathResult<Vec<String>> {
        match fs::symlink_metadata(&self.logs_dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            _ => ensure_directory(&self.logs_dir)?,
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.logs_dir)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let accepted = if exporting {
                safe_name(&name)
                    && !name.starts_with("export-")
                    && (name.starts_with("app.log") || name.ends_with(".log"))
            } else {
                self.validate_name(&name).is_ok()
            };
            if accepted {
                let metadata = fs::symlink_metadata(entry.path())?;
                if regular_metadata(&metadata) {
                    names.push(name);
                }
            }
        }
        names.sort();
        Ok(names)
    }
}

fn safe_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\', ':', '\0']) && !name.contains("..")
}

pub fn validate_log_file_name(name: &str, include_core: bool) -> LanpathResult<()> {
    if safe_name(name)
        && (name.starts_with("app.log")
            || matches!(name, "libbox.log" | "last_start_config.json")
            || (include_core && name == "core.log"))
    {
        Ok(())
    } else {
        Err(LanpathError::Config(format!("非法的日志文件名：{name}")))
    }
}

fn ensure_directory(path: &Path) -> LanpathResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(LanpathError::Config(format!(
            "日志目录不安全：{}",
            path.display()
        )));
    }
    Ok(())
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn regular_metadata(metadata: &fs::Metadata) -> bool {
    if !metadata.is_file() || is_link(metadata) {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return false;
        }
    }
    true
}

fn open_regular(path: &Path) -> LanpathResult<File> {
    ensure_directory(
        path.parent()
            .ok_or_else(|| LanpathError::Config("日志文件没有父目录".into()))?,
    )?;
    let before = fs::symlink_metadata(path)?;
    if !regular_metadata(&before) {
        return Err(LanpathError::Config("日志文件不是安全的普通文件".into()));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    // Reject a last-component symlink swap and avoid blocking on a swapped FIFO.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | 0x800); // O_NOFOLLOW | O_NONBLOCK
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100 | 0x4); // O_NOFOLLOW | O_NONBLOCK
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path)?;
    let after = file.metadata()?;
    if !regular_metadata(&after) {
        return Err(LanpathError::Config("日志文件不是安全的普通文件".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return Err(LanpathError::Config(
                "日志文件在读取前已改变，请刷新后重试".into(),
            ));
        }
    }
    Ok(file)
}

/// Reads a bounded byte snapshot, dropping the partial first line after seeking.
pub fn read_tail_bytes(path: &Path, max_bytes: u64) -> LanpathResult<Vec<u8>> {
    let mut file = open_regular(path)?;
    let size = file.metadata()?.len();
    let start = size.saturating_sub(max_bytes);
    let partial = if start > 0 {
        file.seek(SeekFrom::Start(start - 1))?;
        let mut previous = [0];
        file.read_exact(&mut previous)?;
        previous[0] != b'\n'
    } else {
        false
    };
    let mut bytes = Vec::with_capacity(size.min(max_bytes) as usize);
    file.take(size.min(max_bytes)).read_to_end(&mut bytes)?;
    if partial {
        if let Some(ix) = bytes.iter().position(|&byte| byte == b'\n') {
            bytes.drain(..=ix);
        } else {
            bytes.clear();
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
