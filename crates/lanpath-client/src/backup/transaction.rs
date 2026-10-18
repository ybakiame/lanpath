//! Startup-only transaction: no running core or background writers can race it.
use super::*;
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

const PENDING: &str = ".backup-pending";
const WORK: &str = ".backup-transaction";
const JOURNAL: &str = ".backup-journal.json";

#[derive(Serialize, Deserialize)]
struct Journal {
    originals: Vec<String>,
    #[serde(default = "legacy_roots")]
    roots: Vec<String>,
    committed: bool,
}

fn legacy_roots() -> Vec<String> {
    ROOTS
        .iter()
        .filter(|root| **root != "rulesets")
        .map(|root| (*root).into())
        .collect()
}

pub fn prepare_restore(
    dir: &Path,
    mut snapshot: Snapshot,
    password: &str,
    appearance: Option<Appearance>,
) -> LanpathResult<String> {
    if dir.join(PENDING).exists() || dir.join(JOURNAL).exists() {
        return Err(invalid("已有待恢复的备份，请先重启客户端"));
    }
    snapshot::relocate(&mut snapshot, dir)?;
    let previous = capture(dir, appearance)?;
    let encrypted = encode(&previous, password)?;
    let rollback_dir = dir.join("backup-history");
    std::fs::create_dir_all(&rollback_dir)?;
    let rollback = rollback_dir.join(format!("Lanpath-rollback-{}.zip.age", uuid::Uuid::new_v4()));
    atomic_write(&rollback, &encrypted)?;
    let stage = tempfile::Builder::new()
        .prefix(".backup-prepare-")
        .tempdir_in(dir)?;
    for (name, bytes) in &snapshot.files {
        let path = stage.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&path, bytes)?;
    }
    atomic_write(
        &stage.path().join("manifest.json"),
        &serde_json::to_vec(&snapshot.summary)?,
    )?;
    std::fs::rename(stage.path(), dir.join(PENDING))?;
    // The rollback is already encrypted using the same password as the imported backup.
    Ok(rollback.to_string_lossy().into_owned())
}

pub fn apply_pending(dir: &Path) -> LanpathResult<()> {
    if dir.join(JOURNAL).exists() {
        recover(dir)?;
    }
    let pending = dir.join(PENDING);
    if !pending.exists() {
        return Ok(());
    }
    let summary: BackupSummary =
        serde_json::from_slice(&std::fs::read(pending.join("manifest.json"))?)?;
    // Validate staged files again before touching the current installation.
    let mut files = BTreeMap::new();
    for root in ROOTS {
        collect_pending(&pending, root, &mut files)?;
    }
    snapshot::validate(&mut Snapshot {
        summary: summary.clone(),
        files,
    })?;
    let work = dir.join(WORK);
    if work.exists() {
        std::fs::remove_dir_all(&work)?;
    }
    std::fs::create_dir_all(work.join("original"))?;
    let incoming = work.join("incoming");
    std::fs::create_dir_all(&incoming)?;
    for root in ROOTS {
        copy_tree(&pending.join(root), &incoming.join(root))?;
    }
    let mut journal = Journal {
        originals: Vec::new(),
        roots: ROOTS.iter().map(|root| (*root).into()).collect(),
        committed: false,
    };
    for root in ROOTS {
        if let Ok(meta) = std::fs::symlink_metadata(dir.join(root)) {
            if meta.file_type().is_symlink() {
                return Err(invalid("目标配置包含符号链接，恢复已停止"));
            }
            journal.originals.push((*root).into());
        }
    }
    atomic_write(&dir.join(JOURNAL), &serde_json::to_vec(&journal)?)?;
    let result = (|| -> LanpathResult<()> {
        for root in ROOTS {
            let target = dir.join(root);
            if target.exists() {
                std::fs::rename(&target, work.join("original").join(root))?;
            }
            let source = incoming.join(root);
            if source.exists() {
                std::fs::rename(source, target)?;
            }
        }
        // Metadata is part of the transaction, written before the commit marker.
        atomic_write(
            &dir.join(".backup-applied.json"),
            &serde_json::to_vec(&summary)?,
        )?;
        journal.committed = true;
        atomic_write(&dir.join(JOURNAL), &serde_json::to_vec(&journal)?)?;
        Ok(())
    })();
    if let Err(error) = result {
        recover(dir)?;
        return Err(error);
    }
    recover(dir)
}

fn recover(dir: &Path) -> LanpathResult<()> {
    let journal: Journal = serde_json::from_slice(&std::fs::read(dir.join(JOURNAL))?)?;
    if journal.roots.iter().any(|s| !ROOTS.contains(&s.as_str()))
        || journal.originals.iter().any(|s| !journal.roots.contains(s))
    {
        return Err(invalid("恢复日志无效"));
    }
    let work = dir.join(WORK);
    if !journal.committed {
        for root in &journal.roots {
            let original = work.join("original").join(root);
            let target = dir.join(root);
            if original.exists() {
                remove(&target)?;
                std::fs::rename(original, target)?;
            } else if !journal.originals.iter().any(|s| s == root)
                && !work.join("incoming").join(root).exists()
            {
                remove(&target)?;
            }
        }
        remove(&dir.join(".backup-applied.json"))?;
    } else {
        remove(&dir.join(PENDING))?;
    }
    remove(&work)?;
    remove(&dir.join(JOURNAL))?;
    Ok(())
}

fn collect_pending(
    dir: &Path,
    name: &str,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> LanpathResult<()> {
    let path = dir.join(name);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() {
        return Err(invalid("恢复文件包含链接"));
    }
    if meta.is_dir() {
        if name.split('/').count() >= 8 {
            return Err(invalid("恢复目录过深"));
        }
        for entry in std::fs::read_dir(path)? {
            let next = format!("{name}/{}", entry?.file_name().to_string_lossy());
            if !allowed(&next) {
                return Err(invalid("恢复路径无效"));
            }
            collect_pending(dir, &next, files)?;
        }
    } else if meta.is_file() {
        if meta.len() > MAX_EXPANDED as u64 {
            return Err(invalid("恢复文件过大"));
        }
        files.insert(name.into(), std::fs::read(path)?);
        check_size(files)?;
    } else {
        return Err(invalid("恢复条目无效"));
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> LanpathResult<()> {
    if !from.exists() {
        return Ok(());
    }
    if std::fs::symlink_metadata(from)?.file_type().is_symlink() {
        return Err(invalid("恢复文件包含链接"));
    }
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        atomic_write(to, &std::fs::read(from)?)?;
    }
    Ok(())
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> LanpathResult<()> {
    let parent = path.parent().ok_or_else(|| invalid("备份目标无效"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn remove(path: &Path) -> LanpathResult<()> {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.is_dir() && !meta.file_type().is_symlink() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// A failed pre-commit restore must leave the old app usable. Ambiguous journal
/// recovery still stops startup rather than exposing a partially restored state.
pub fn apply_pending_at_startup(dir: &Path) -> LanpathResult<()> {
    match apply_pending(dir) {
        Ok(()) => Ok(()),
        Err(error) if dir.join(JOURNAL).exists() => Err(error),
        Err(_) => {
            let pending = dir.join(PENDING);
            if pending.exists() {
                std::fs::rename(
                    pending,
                    dir.join(format!(".backup-rejected-{}", uuid::Uuid::new_v4())),
                )?;
            }
            atomic_write(
                &dir.join(".backup-failed"),
                b"restore failed; original config preserved",
            )?;
            Ok(())
        }
    }
}
