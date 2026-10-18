//! One-time metadata upgrade. Keep the original bytes before replacing configuration.
use lanpath_common::{LanpathError, LanpathResult};
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path};

static MIGRATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn upgrade(dir: &Path, text: String) -> LanpathResult<String> {
    let _guard = MIGRATION_LOCK
        .lock()
        .map_err(|_| LanpathError::Client("规则集升级锁不可用".into()))?;
    let text = std::fs::read_to_string(dir.join("local_override.json")).unwrap_or(text);
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(text);
    };
    if serde_json::from_value::<super::LocalOverride>(value.clone()).is_err() {
        return Ok(text);
    }
    match value
        .get("ruleset_metadata_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
    {
        1 => return Ok(text),
        0 => {}
        _ => {
            return Err(LanpathError::Client(
                "规则集配置版本较新，请升级客户端".into(),
            ));
        }
    }
    preserve_original(dir, text.as_bytes())?;
    value["ruleset_metadata_version"] = 1.into();
    if let Some(sets) = value
        .get_mut("custom_rule_sets")
        .and_then(serde_json::Value::as_array_mut)
    {
        for rule_set in sets {
            if rule_set.get("category").is_none() {
                rule_set["category"] = "".into();
            }
        }
    }
    let next = serde_json::to_string_pretty(&value)?;
    let mut temporary = tempfile::NamedTempFile::new_in(dir)?;
    temporary.write_all(next.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(dir.join("local_override.json"))
        .map_err(|e| e.error)?;
    Ok(next)
}

fn preserve_original(dir: &Path, bytes: &[u8]) -> LanpathResult<()> {
    let path = dir.join("local_override.ruleset-metadata-v1.backup.json");
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    match file.persist_noclobber(&path) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(&path)? == bytes {
                return Ok(());
            }
            // A restored old snapshot is a different upgrade, not a retry of the first one.
            let distinct = dir.join(format!(
                "local_override.ruleset-metadata-v1-{:x}.backup.json",
                Sha256::digest(bytes)
            ));
            match error.file.persist_noclobber(&distinct) {
                Ok(_) => Ok(()),
                Err(error)
                    if error.error.kind() == std::io::ErrorKind::AlreadyExists
                        && std::fs::read(&distinct)? == bytes =>
                {
                    Ok(())
                }
                Err(error) => Err(error.error.into()),
            }
        }
        Err(error) => Err(error.error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upgrade_is_atomic_versioned_and_preserves_exact_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let original = "{\n \"custom_rule_sets\": [], \"builtins_seeded\": true\n}";
        std::fs::write(dir.path().join("local_override.json"), original).unwrap();
        let upgraded = upgrade(dir.path(), original.into()).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&upgraded).unwrap()["ruleset_metadata_version"],
            1
        );
        assert_eq!(
            std::fs::read_to_string(
                dir.path()
                    .join("local_override.ruleset-metadata-v1.backup.json")
            )
            .unwrap(),
            original
        );
        assert_eq!(upgrade(dir.path(), upgraded.clone()).unwrap(), upgraded);
    }
    #[test]
    fn restoring_another_old_snapshot_keeps_both_originals_and_backup_errors_preserve_config() {
        let dir = tempfile::tempdir().unwrap();
        let first = "{\"builtins_seeded\":true}";
        let second = "{\"builtins_seeded\":true,\"custom_rule_sets\":[]}";
        std::fs::write(dir.path().join("local_override.json"), first).unwrap();
        upgrade(dir.path(), first.into()).unwrap();
        std::fs::write(dir.path().join("local_override.json"), second).unwrap();
        upgrade(dir.path(), second.into()).unwrap();
        let copies: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().ends_with(".backup.json"))
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect();
        assert!(copies.contains(&first.to_owned()) && copies.contains(&second.to_owned()));
        let broken = tempfile::tempdir().unwrap();
        std::fs::write(broken.path().join("local_override.json"), first).unwrap();
        std::fs::create_dir(
            broken
                .path()
                .join("local_override.ruleset-metadata-v1.backup.json"),
        )
        .unwrap();
        assert!(upgrade(broken.path(), first.into()).is_err());
        assert_eq!(
            std::fs::read_to_string(broken.path().join("local_override.json")).unwrap(),
            first
        );
    }
}
