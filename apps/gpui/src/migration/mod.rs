//! Versioned, splash-screen friendly settings migrations.
//!
//! A migration only transforms files. The runner owns backup, validation and
//! artifact publication, so later format migrations can reuse the same flow.

mod json_to_toml;
mod resource_layout;
pub(crate) mod session;

use std::path::{Path, PathBuf};

use lanpath_common::{LanpathError, LanpathResult};
use serde::Serialize;

use json_to_toml::JsonToToml;

pub(crate) trait SettingsMigration: Send + Sync {
    fn id(&self) -> &'static str;
    fn is_required(&self, root: &Path) -> LanpathResult<bool>;
    fn rollback_files(&self) -> &'static [&'static str] {
        &[]
    }
    fn legacy_files(&self) -> &'static [&'static str];
    fn backup_files(&self) -> &'static [&'static str] {
        self.legacy_files()
    }
    fn migrate(&self, root: &Path) -> LanpathResult<()>;
    fn validate(&self, root: &Path) -> LanpathResult<()>;
    fn output_files(&self) -> &'static [&'static str];
}

static JSON_TO_TOML: JsonToToml = JsonToToml;
static RESOURCE_LAYOUT: resource_layout::ResourceLayout = resource_layout::ResourceLayout;
const MIGRATIONS: &[&dyn SettingsMigration] = &[&JSON_TO_TOML, &RESOURCE_LAYOUT];

#[derive(Serialize)]
struct MigrationManifest<'a> {
    id: &'a str,
    created_at: String,
    legacy: &'a [&'static str],
    converted: &'a [&'static str],
}

pub(crate) fn is_required(root: &Path) -> LanpathResult<bool> {
    for migration in MIGRATIONS {
        if migration.is_required(root)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn run(root: &Path) -> LanpathResult<Vec<PathBuf>> {
    crate::data_root::validate_tree(root)?;
    let mut backups = Vec::new();
    for migration in MIGRATIONS {
        if migration.is_required(root)? {
            backups.push(run_one(root, *migration)?);
        }
    }
    Ok(backups)
}

fn run_one(root: &Path, migration: &'static dyn SettingsMigration) -> LanpathResult<PathBuf> {
    let backup =
        root.join("backup/migrations")
            .join(format!("{}-{}", migration.id(), uuid::Uuid::new_v4()));
    let legacy = backup.join("legacy");
    let converted = backup.join("converted");
    std::fs::create_dir_all(&legacy)?;
    for name in migration.backup_files() {
        copy_entry(&root.join(name), &legacy.join(name))?;
    }

    let new_outputs: Vec<&str> = migration
        .output_files()
        .iter()
        .filter(|name| !root.join(name).exists())
        .copied()
        .collect();
    let result = (|| {
        migration.migrate(root)?;
        migration.validate(root)?;
        std::fs::create_dir(&converted)?;
        for name in migration.output_files() {
            copy_entry(&root.join(name), &converted.join(name))?;
        }
        let manifest = MigrationManifest {
            id: migration.id(),
            created_at: chrono::Utc::now().to_rfc3339(),
            legacy: migration.legacy_files(),
            converted: migration.output_files(),
        };
        std::fs::write(
            backup.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(())
    })();
    if let Err(error) = result {
        for name in migration.rollback_files() {
            let original = legacy.join(name);
            if original.is_file() {
                use std::io::Write as _;
                let mut file = tempfile::NamedTempFile::new_in(root)?;
                file.write_all(&std::fs::read(original)?)?;
                file.as_file().sync_all()?;
                file.persist(root.join(name)).map_err(|error| error.error)?;
            }
        }
        // Only files created by this attempt are removed. A pre-existing valid
        // unified document must survive a later cleanup or validation failure.
        for name in new_outputs {
            match remove_entry(&root.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(LanpathError::Config(format!(
                        "迁移失败后清理新配置失败：{error}"
                    )));
                }
            }
        }
        return Err(error);
    }
    // Removal is the final step: legacy copies and converted output already
    // exist. A removal failure keeps the valid app.toml for the next retry.
    for name in migration.legacy_files() {
        match remove_entry(&root.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(backup)
}

fn remove_entry(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn copy_entry(source: &Path, destination: &Path) -> LanpathResult<()> {
    let metadata = match std::fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
        return Err(LanpathError::Config("迁移拒绝链接或特殊文件".into()));
    }
    if metadata.is_dir() {
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_entry(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }
    let bytes = std::fs::read(source)?;
    if destination.exists() {
        if std::fs::read(destination)? == bytes {
            return Ok(());
        }
        return Err(LanpathError::Config(
            "迁移目标已有不同内容，原数据保留，请检查迁移备份".into(),
        ));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, destination)?;
    if std::fs::read(destination)? != bytes {
        return Err(LanpathError::Config("迁移文件校验失败，原数据保留".into()));
    }
    Ok(())
}
