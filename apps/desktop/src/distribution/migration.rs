use super::{invalid, signature::safe_relative};
#[path = "migration_files.rs"]
mod files;
#[path = "migration_validation.rs"]
mod validation;
use crate::data_root::DataRoot;
use files::*;
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum SourceKind {
    Experimental,
    Tauri,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum CaScope {
    Exclude,
    IdentityAndPrivateKey,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct MigrationOptions {
    pub configuration: bool,
    pub history: bool,
    pub appearance: bool,
    pub ca: CaScope,
}
impl Default for MigrationOptions {
    fn default() -> Self {
        Self {
            configuration: true,
            history: false,
            appearance: true,
            ca: CaScope::Exclude,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct MigrationPreview {
    pub source: PathBuf,
    pub source_kind: SourceKind,
    pub options: MigrationOptions,
    pub files: BTreeMap<String, FileStamp>,
    pub conflicts: Vec<String>,
    pub omitted: Vec<String>,
    pub bytes: u64,
    target: PathBuf,
    target_files: BTreeMap<String, FileStamp>,
    roots: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PreparedMigration {
    pub id: String,
    pub files: usize,
    pub bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MigrationRecovery {
    None,
    Pending,
    RollbackPending,
    Applied,
    RolledBack,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum Phase {
    Prepared,
    Applying,
    Applied,
    RollbackRequested,
    RolledBack,
}
#[derive(Debug, Serialize, Deserialize)]
struct Journal {
    id: String,
    phase: Phase,
    roots: Vec<String>,
    originals: Vec<String>,
    staged: BTreeMap<String, FileStamp>,
    backup: BTreeMap<String, FileStamp>,
}
/// Must be used before opening any target business store, under the root lease.
pub(crate) struct MigrationService<'a> {
    root: &'a DataRoot,
    work_name: &'static str,
}
impl<'a> MigrationService<'a> {
    pub(crate) fn new(root: &'a DataRoot) -> Self {
        Self {
            root,
            work_name: ".gpui-import",
        }
    }
    fn work(&self) -> PathBuf {
        self.root.path().join(self.work_name)
    }
    fn journal(&self) -> LanpathResult<Option<Journal>> {
        crate::data_root::validate_owned_root(self.root.path())?;
        match std::fs::read(self.work().join("journal.json")) {
            Ok(bytes) => {
                let journal: Journal = serde_json::from_slice(&bytes)?;
                let mut unique = std::collections::HashSet::new();
                if journal.roots.is_empty() {
                    return Err(invalid("migration journal has no categories"));
                }
                for root in &journal.roots {
                    safe_relative(root)?;
                    if !allowed_root(root) {
                        return Err(invalid("unknown migration journal root"));
                    }
                    if !unique.insert(root) {
                        return Err(invalid("duplicate migration journal root"));
                    }
                }
                if journal.originals.iter().any(|s| !journal.roots.contains(s)) {
                    return Err(invalid("invalid migration backup journal"));
                }
                Ok(Some(journal))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub(crate) fn preview(
        &self,
        source: &Path,
        source_kind: SourceKind,
        options: MigrationOptions,
    ) -> LanpathResult<MigrationPreview> {
        crate::data_root::validate_tree(source)?;
        let source = source.canonicalize()?;
        if source.starts_with(self.root.path()) || self.root.path().starts_with(&source) {
            return Err(invalid("source and target migration roots overlap"));
        }
        if source_kind == SourceKind::Experimental {
            let marker = std::fs::read(source.join(".lanpath-experimental"))?;
            if marker != b"lanpath-experimental-data-root-v1\n" {
                return Err(invalid("source is not an owned experimental data root"));
            }
        }
        if !source.join("client.json").is_file() {
            return Err(invalid("selected source does not contain client.json"));
        }
        let mut roots = Vec::new();
        if options.configuration {
            roots.extend(CONFIG.iter().map(|s| (*s).into()));
        }
        if options.history {
            roots.extend(HISTORY.iter().map(|s| (*s).into()));
        }
        if options.appearance {
            roots.push("appearance.json".into());
        }
        if options.ca == CaScope::IdentityAndPrivateKey {
            roots.extend(["certs/ca.crt".into(), "certs/ca.key".into()]);
        }
        if roots.is_empty() {
            return Err(invalid("select at least one migration category"));
        }
        let files = inventory(&source, &roots)?;
        let target_files = inventory(self.root.path(), &roots)?;
        // Validate a stable private copy, never open or migrate source stores.
        let validation = tempfile::tempdir_in(self.root.path())?;
        copy_inventory(&source, validation.path(), &files)?;
        if inventory(&source, &roots)? != files {
            return Err(invalid(
                "source changed during preview; stop its writers and retry",
            ));
        }
        validation::validate_stage(validation.path(), &source, self.root.path(), options)?;
        let conflicts = roots
            .iter()
            .filter(|name| self.root.path().join(name).exists())
            .cloned()
            .collect();
        let mut omitted = vec![
            "core binaries (the target's selected core is preserved)",
            "system proxy/TUN/MITM activation",
            "OS certificate trust",
            "logs, transient sessions, backups, updater state and unrecognized files",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        if !options.configuration {
            omitted.push("configuration and resources".into());
        }
        if !options.history {
            omitted.push("traffic history (stats.db)".into());
        }
        if !options.appearance {
            omitted.push("appearance".into());
        }
        if options.ca == CaScope::Exclude {
            omitted.push("CA identity and private key".into());
        }
        let bytes = files.values().map(|f| f.size).sum();
        Ok(MigrationPreview {
            source,
            source_kind,
            options,
            files,
            conflicts,
            omitted,
            bytes,
            target: self.root.path().into(),
            target_files,
            roots,
        })
    }
    pub(crate) fn prepare(&self, preview: MigrationPreview) -> LanpathResult<PreparedMigration> {
        if preview.target != self.root.path() || self.journal()?.is_some() {
            return Err(invalid(
                "acknowledge or rollback the existing migration before preparing another",
            ));
        }
        if inventory(&preview.source, &preview.roots)? != preview.files
            || inventory(self.root.path(), &preview.roots)? != preview.target_files
        {
            return Err(invalid("migration preview is stale; preview again"));
        }
        let work = self.work();
        if work.try_exists()? {
            remove(&work)?;
        }
        std::fs::create_dir(&work)?;
        let stage = work.join("incoming");
        let backup = work.join("original");
        std::fs::create_dir(&stage)?;
        std::fs::create_dir(&backup)?;
        copy_inventory(&preview.source, &stage, &preview.files)?;
        if inventory(&preview.source, &preview.roots)? != preview.files {
            remove(&work)?;
            return Err(invalid(
                "source changed during import; original target unchanged",
            ));
        }
        validation::validate_stage(&stage, &preview.source, self.root.path(), preview.options)?;
        for name in &preview.roots {
            copy_tree(&self.root.path().join(name), &backup.join(name))?;
        }
        if inventory(self.root.path(), &preview.roots)? != preview.target_files {
            remove(&work)?;
            return Err(invalid("target changed during backup"));
        }
        let journal = Journal {
            id: uuid::Uuid::new_v4().to_string(),
            phase: Phase::Prepared,
            originals: preview
                .roots
                .iter()
                .filter(|name| self.root.path().join(name).exists())
                .cloned()
                .collect(),
            staged: inventory(&stage, &preview.roots)?,
            backup: inventory(&backup, &preview.roots)?,
            roots: preview.roots,
        };
        atomic_json(&work.join("journal.json"), &journal)?;
        Ok(PreparedMigration {
            id: journal.id,
            files: journal.staged.len(),
            bytes: journal.staged.values().map(|f| f.size).sum(),
        })
    }
    pub(crate) fn status(&self) -> LanpathResult<MigrationRecovery> {
        Ok(match self.journal()?.map(|j| j.phase) {
            None => MigrationRecovery::None,
            Some(Phase::Prepared | Phase::Applying) => MigrationRecovery::Pending,
            Some(Phase::RollbackRequested) => MigrationRecovery::RollbackPending,
            Some(Phase::Applied) => MigrationRecovery::Applied,
            Some(Phase::RolledBack) => MigrationRecovery::RolledBack,
        })
    }
    /// Startup barrier: call under the DataRoot lease BEFORE any business store.
    /// Imports recover first; owned Portable moves use a separate repeatable
    /// journal so an unacknowledged import remains available for rollback.
    pub(crate) fn startup(&self) -> LanpathResult<MigrationRecovery> {
        let recovery = self.apply_pending()?;
        let relocation = Self {
            root: self.root,
            work_name: ".gpui-relocation",
        };
        relocation.apply_pending()?;
        relocation.acknowledge()?;
        let path = self.root.path().join("client.json");
        let config: lanpath_client::ClientConfig = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(recovery),
            Err(e) => return Err(e.into()),
        };
        if config.data_dir == self.root.path() {
            return Ok(recovery);
        }
        relocation.prepare_relocation(&config.data_dir)?;
        relocation.apply_pending()?;
        relocation.acknowledge()?;
        Ok(recovery)
    }
    fn prepare_relocation(&self, previous: &Path) -> LanpathResult<()> {
        if self.journal()?.is_some() {
            return Err(invalid("unfinished owned-root relocation"));
        }
        let work = self.work();
        if work.try_exists()? {
            remove(&work)?;
        }
        std::fs::create_dir(&work)?;
        let stage = work.join("incoming");
        let backup = work.join("original");
        std::fs::create_dir(&stage)?;
        std::fs::create_dir(&backup)?;
        let roots: Vec<String> = CONFIG.iter().map(|s| (*s).into()).collect();
        let before = inventory(self.root.path(), &roots)?;
        copy_inventory(self.root.path(), &stage, &before)?;
        copy_inventory(self.root.path(), &backup, &before)?;
        if inventory(self.root.path(), &roots)? != before {
            return Err(invalid("owned configuration changed during relocation"));
        }
        relocate(&stage, previous, self.root.path(), false)?;
        let journal = Journal {
            id: uuid::Uuid::new_v4().to_string(),
            phase: Phase::Prepared,
            originals: roots
                .iter()
                .filter(|name| self.root.path().join(name).exists())
                .cloned()
                .collect(),
            staged: inventory(&stage, &roots)?,
            backup: before,
            roots,
        };
        atomic_json(&work.join("journal.json"), &journal)
    }
    pub(crate) fn apply_pending(&self) -> LanpathResult<MigrationRecovery> {
        let Some(mut journal) = self.journal()? else {
            return Ok(MigrationRecovery::None);
        };
        match journal.phase {
            Phase::Applied => return Ok(MigrationRecovery::Applied),
            Phase::RolledBack => return Ok(MigrationRecovery::RolledBack),
            Phase::Applying | Phase::RollbackRequested => {
                self.restore(&mut journal)?;
                return Ok(MigrationRecovery::RolledBack);
            }
            Phase::Prepared => {}
        }
        let stage = self.work().join("incoming");
        if inventory(&stage, &journal.roots)? != journal.staged {
            return Err(invalid("staged migration was modified; target unchanged"));
        }
        self.validate_backup(&journal)?;
        if inventory(self.root.path(), &journal.roots)? != journal.backup {
            return Err(invalid(
                "target changed after migration preparation; rollback or preview again",
            ));
        }
        journal.phase = Phase::Applying;
        atomic_json(&self.work().join("journal.json"), &journal)?;
        let result = (|| -> LanpathResult<()> {
            for name in &journal.roots {
                let target = self.root.path().join(name);
                remove(&target)?;
                let incoming = stage.join(name);
                if incoming.try_exists()? {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::rename(incoming, target)?;
                }
                if let Some(parent) = self.root.path().join(name).parent() {
                    sync_directory(parent)?;
                }
            }
            journal.phase = Phase::Applied;
            atomic_json(&self.work().join("journal.json"), &journal)
        })();
        if let Err(error) = result {
            self.restore(&mut journal).map_err(|recovery| invalid(format!("migration failed ({error}); rollback also failed ({recovery}); keep this root closed")))?;
            return Err(error);
        }
        Ok(MigrationRecovery::Applied)
    }
    fn validate_backup(&self, journal: &Journal) -> LanpathResult<()> {
        if inventory(&self.work().join("original"), &journal.roots)? != journal.backup {
            return Err(invalid(
                "migration rollback backup integrity failed; keep root closed",
            ));
        }
        Ok(())
    }
    fn restore(&self, journal: &mut Journal) -> LanpathResult<()> {
        self.validate_backup(journal)?;
        journal.phase = Phase::Applying;
        atomic_json(&self.work().join("journal.json"), journal)?;
        // Copies, not moves: interrupted rollback is repeatable on the next startup.
        for name in &journal.roots {
            let target = self.root.path().join(name);
            remove(&target)?;
            if journal.originals.contains(name) {
                copy_tree(&self.work().join("original").join(name), &target)?;
            }
            if let Some(parent) = target.parent() {
                sync_directory(parent)?;
            }
        }
        journal.phase = Phase::RolledBack;
        atomic_json(&self.work().join("journal.json"), journal)
    }
    /// Persist intent only. Store replacement belongs to the next startup barrier,
    /// never to a live GUI with open SQLite/configuration stores.
    pub(crate) fn request_rollback(&self) -> LanpathResult<()> {
        let mut journal = self
            .journal()?
            .ok_or_else(|| invalid("no migration available to roll back"))?;
        match journal.phase {
            Phase::RolledBack | Phase::RollbackRequested => return Ok(()),
            Phase::Applying => {
                return Err(invalid(
                    "restart to finish interrupted migration recovery first",
                ));
            }
            Phase::Prepared | Phase::Applied => {}
        }
        self.validate_backup(&journal)?;
        journal.phase = Phase::RollbackRequested;
        atomic_json(&self.work().join("journal.json"), &journal)
    }
    /// Removes the sensitive rollback only after explicit user acknowledgement.
    pub(crate) fn acknowledge(&self) -> LanpathResult<()> {
        if self.journal()?.is_some_and(|j| {
            matches!(
                j.phase,
                Phase::Prepared | Phase::Applying | Phase::RollbackRequested
            )
        }) {
            return Err(invalid("cannot acknowledge an unfinished migration"));
        }
        remove(&self.work())
    }
}
fn allowed_root(name: &str) -> bool {
    CONFIG.contains(&name)
        || HISTORY.contains(&name)
        || matches!(name, "appearance.json" | "certs/ca.crt" | "certs/ca.key")
}
fn copy_inventory(
    source: &Path,
    stage: &Path,
    files: &BTreeMap<String, FileStamp>,
) -> LanpathResult<()> {
    for (name, stamp) in files {
        safe_relative(name)?;
        copy_tree(&source.join(name), &stage.join(name))?;
        if super::signature::digest(&stage.join(name))? != (stamp.size, stamp.sha256.clone()) {
            return Err(invalid("migration source changed during copy"));
        }
    }
    Ok(())
}

pub(super) fn write_transaction_json<T: Serialize>(path: &Path, value: &T) -> LanpathResult<()> {
    atomic_json(path, value)
}
#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
