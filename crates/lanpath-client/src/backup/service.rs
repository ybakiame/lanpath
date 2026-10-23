//! Framework-independent backup orchestration; callers schedule blocking work.
use super::{
    Appearance, BackupPreview, BackupSummary, MAX_ARCHIVE, PreviewCache, RestoreInput, capture,
    encode, invalid,
};
use lanpath_common::LanpathResult;
use std::{
    fs::File,
    io::{ErrorKind, Read},
    path::PathBuf,
};

pub struct BackupService {
    root: PathBuf,
    previews: PreviewCache,
}

impl BackupService {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            previews: PreviewCache::default(),
        }
    }

    pub fn export(&self, password: &str, appearance: Option<Appearance>) -> LanpathResult<Vec<u8>> {
        encode(&capture(&self.root, appearance)?, password)
    }

    pub fn inspect(&self, encrypted: &[u8], password: String) -> LanpathResult<BackupPreview> {
        // Serde validation errors can contain decrypted field values. This
        // user-facing boundary exposes only validation failure, never payload.
        self.previews
            .inspect(encrypted, password)
            .map_err(|_| invalid("备份无法解密或校验：请检查密码、文件完整性和备份版本"))
    }

    /// Consume authorization before stopping writers; prepare only after they settle.
    pub fn take_preview(&self, id: &str) -> LanpathResult<RestoreInput> {
        self.previews.take(id)
    }

    /// Callers also schedule this after 300 seconds to release unused secrets promptly.
    pub fn discard(&self, id: &str) {
        self.previews.discard(id);
    }

    /// Read-only startup result, acknowledged after the shell applies appearance.
    pub fn restored(&self) -> LanpathResult<Option<BackupSummary>> {
        if self.root.join(".backup-failed").try_exists()? {
            return Err(invalid("恢复未完成，原配置已保留。请重新选择备份并导入"));
        }
        match std::fs::read(self.root.join(".backup-applied.json")) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn acknowledge(&self) -> LanpathResult<()> {
        for name in [".backup-failed", ".backup-applied.json"] {
            match std::fs::remove_file(self.root.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub fn is_pending(&self) -> bool {
        self.root.join(".backup-pending").exists()
    }

    /// Return the newest encrypted rollback without interpreting or decrypting it.
    pub fn previous(&self) -> LanpathResult<Option<Vec<u8>>> {
        let entries = match std::fs::read_dir(self.root.join("backup-history")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut newest = None;
        for entry in entries {
            let entry = entry?;
            if !entry.file_name().to_string_lossy().ends_with(".zip.age")
                || !entry.file_type()?.is_file()
            {
                continue;
            }
            let modified = entry.metadata()?.modified()?;
            if newest.as_ref().is_none_or(|(time, _)| modified >= *time) {
                newest = Some((modified, entry.path()));
            }
        }
        let Some((_, path)) = newest else {
            return Ok(None);
        };
        let file = File::open(path)?;
        if file.metadata()?.len() > MAX_ARCHIVE as u64 {
            return Err(invalid("回滚文件过大"));
        }
        let mut bytes = Vec::new();
        file.take((MAX_ARCHIVE + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_ARCHIVE {
            return Err(invalid("回滚文件过大"));
        }
        Ok(Some(bytes))
    }
}
