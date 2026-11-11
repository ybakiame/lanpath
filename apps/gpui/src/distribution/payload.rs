//! Authenticated payload updates for Standard and Portable data locations. This
//! early command path performs no startup, OS integration or process enumeration.
use super::{
    invalid, payload_files as files,
    signature::{self, Manifest},
};
use files::Files;
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

#[cfg(any(windows, test))]
#[path = "payload_cli.rs"]
mod cli;
#[path = "payload_root.rs"]
mod roots;
#[cfg(windows)]
pub(crate) use cli::{admit_application, dispatch_from_env};
use roots::Root;

const JOURNAL: &str = ".gpui-payload-journal.json";
const BACKUP: &str = ".gpui-payload-backup";
const LOCK: &str = ".gpui-payload-lock";
const LEGACY_STATE: &[&str] = &[
    ".gpui-installed-journal.json",
    ".gpui-installed-backup",
    ".gpui-installed.lock",
    ".gpui-portable-journal.json",
    ".gpui-portable-backup",
    ".gpui-portable.lock",
    ".gpui-update-journal.json",
    ".gpui-update-backup",
    ".gpui-update.lock",
];

fn refuse_legacy_state(root: &Path) -> LanpathResult<()> {
    for name in LEGACY_STATE {
        let path = root.join(name);
        files::check_path(&path)?;
        if path.try_exists()? {
            return Err(invalid(
                "legacy payload transaction state preserved; authenticated manual reconciliation required",
            ));
        }
    }
    Ok(())
}

fn admit_application_at(root: &Path) -> LanpathResult<()> {
    refuse_legacy_state(root)?;
    for name in [JOURNAL, BACKUP] {
        let path = root.join(name);
        files::check_path(&path)?;
        if path.try_exists()? {
            return Err(invalid(
                "this payload has an unfinished transaction; use its authenticated replacement/recovery helper before launching GPUI",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SignedBundle {
    payload: String,
    signature: String,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Staging,
    Prepared,
    Committed,
    RolledBack,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    root: PathBuf,
    mode: roots::DataMode,
    identity: roots::Identity,
    phase: Phase,
    previous: SignedBundle,
    next: SignedBundle,
}

impl SignedBundle {
    fn read(files: &Files) -> LanpathResult<Self> {
        let get = |name: &str, limit| -> LanpathResult<String> {
            String::from_utf8(files::bytes(
                files
                    .get(name)
                    .ok_or_else(|| invalid("incomplete authenticated payload bundle"))?,
                limit,
            )?)
            .map_err(|_| invalid("payload bundle metadata is not UTF-8"))
        };
        Ok(Self {
            payload: get("bundle.json", 128 * 1024)?,
            signature: get("bundle.json.sig", 1024)?,
        })
    }
    fn manifest(&self, key: &str) -> LanpathResult<Manifest> {
        let manifest = signature::verify_manifest(self.payload.as_bytes(), &self.signature, key)?;
        let app = manifest
            .files
            .iter()
            .find(|file| file.path == crate::product::BINARY_NAME)
            .ok_or_else(|| invalid("bundle executable missing"))?;
        if manifest.artifact.url.rsplit('/').next() != Some(crate::product::BINARY_NAME)
            || manifest.artifact.size != app.size
            || manifest.artifact.sha256 != app.sha256
        {
            return Err(invalid(
                "payload recovery requires an executable bundle descriptor",
            ));
        }
        Ok(manifest)
    }
    fn stamps(
        &self,
        key: &str,
    ) -> LanpathResult<std::collections::BTreeMap<String, (u64, String)>> {
        let mut stamps = self
            .manifest(key)?
            .files
            .into_iter()
            .map(|file| (file.path, (file.size, file.sha256)))
            .collect::<std::collections::BTreeMap<_, _>>();
        for (name, mut bytes) in [
            ("bundle.json", self.payload.as_bytes()),
            ("bundle.json.sig", self.signature.as_bytes()),
        ] {
            stamps.insert(name.into(), signature::digest_reader(&mut bytes)?);
        }
        Ok(stamps)
    }
    fn verify(&self, files: &Files, key: &str) -> LanpathResult<()> {
        for (name, expected) in self.stamps(key)? {
            if files::stamp(
                files
                    .get(&name)
                    .ok_or_else(|| invalid("incomplete authenticated payload bundle"))?,
            )? != expected
            {
                return Err(invalid(format!("payload bundle digest mismatch: {name}")));
            }
        }
        Ok(())
    }
}

fn lock(root: &Path) -> LanpathResult<File> {
    let path = root.join(LOCK);
    files::check_path(&path)?;
    let file = files::options(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()
        .map_err(|error| invalid(format!("another operation owns this payload root: {error}")))?;
    Ok(file)
}
fn save(root: &Path, journal: &Journal) -> LanpathResult<()> {
    files::check_path(&root.join(JOURNAL))?;
    super::migration::write_transaction_json(&root.join(JOURNAL), journal)
}
fn cleanup(root: &Root) -> LanpathResult<()> {
    roots::validate(root)?;
    files::clean_backup(&root.path.join(BACKUP))?;
    roots::validate(root)?;
    std::fs::remove_file(root.path.join(JOURNAL))?;
    files::sync_directory(&root.path)
}
fn version(bundle: &SignedBundle, key: &str) -> LanpathResult<semver::Version> {
    semver::Version::parse(&bundle.manifest(key)?.version)
        .map_err(|_| invalid("invalid payload version"))
}

// Every target must still be an authenticated old or incoming file. This is
// possible even after a crash because replacement is atomic per-file, never an
// in-place partial write. Unexpected edits cause refusal before ANY restoration.
fn validate_recovery_targets(target: &Files, journal: &Journal, key: &str) -> LanpathResult<()> {
    let previous = journal.previous.stamps(key)?;
    let next = journal.next.stamps(key)?;
    for name in files::paths() {
        let file = target
            .get(name)
            .ok_or_else(|| invalid("recovery refuses a missing previous payload file"))?;
        let stamp = files::stamp(file)?;
        if previous.get(name) != Some(&stamp) && next.get(name) != Some(&stamp) {
            return Err(invalid(format!(
                "recovery refuses an unexpected payload edit: {name}"
            )));
        }
    }
    Ok(())
}

fn recover_locked(root: &Root, key: &str) -> LanpathResult<Option<SignedBundle>> {
    roots::validate(root)?;
    let path = root.path.join(JOURNAL);
    files::check_path(&path)?;
    files::check_path(&root.path.join(BACKUP))?;
    let file = files::options(false).open(&path)?;
    let mut journal: Journal = serde_json::from_slice(&files::bytes(&file, 512 * 1024)?)?;
    drop(file);
    if journal.schema != 1
        || journal.root != root.path
        || journal.mode != root.mode
        || journal.identity != root.identity
    {
        return Err(invalid(
            "payload journal belongs to another root, data mode, identity or schema",
        ));
    }
    if !version(&journal.previous, key)?
        .cmp_precedence(&version(&journal.next, key)?)
        .is_lt()
    {
        return Err(invalid("invalid payload recovery version transition"));
    }
    let target = files::open_files(&root.path, true)?;
    match journal.phase {
        Phase::Staging | Phase::RolledBack => journal.previous.verify(&target, key)?,
        Phase::Committed => journal.next.verify(&target, key)?,
        Phase::Prepared => {
            let backup = files::open_files(&root.path.join(BACKUP), false)?;
            journal.previous.verify(&backup, key)?;
            validate_recovery_targets(&target, &journal, key)?;
            let mut restored = Files::new();
            for name in files::paths() {
                roots::validate(root)?;
                restored.insert(
                    name.into(),
                    files::replace(&root.path, name, &backup[name])?,
                );
            }
            journal.previous.verify(&restored, key)?;
            roots::validate(root)?;
            journal.phase = Phase::RolledBack;
            save(&root.path, &journal)?;
        }
    }
    drop(target);
    cleanup(root)?;
    Ok((journal.phase == Phase::Committed).then_some(journal.next))
}

fn update_with(
    root: &Path,
    source: &Path,
    key: &str,
    mut before_replace: impl FnMut(usize) -> LanpathResult<()>,
) -> LanpathResult<()> {
    let admitted = roots::admit(root)?;
    let root = &admitted.path;
    let _lock = lock(root)?;
    files::check_path(&root.join(JOURNAL))?;
    files::check_path(&root.join(BACKUP))?;
    files::check_path(source)?;
    let source_root = source.canonicalize()?;
    if source_root == *root || source_root.starts_with(root) || root.starts_with(&source_root) {
        return Err(invalid("payload helper must be outside the selected root"));
    }
    let source = files::open_files(&source_root, false)?;
    let next = SignedBundle::read(&source)?;
    next.verify(&source, key)?;
    if root.join(JOURNAL).try_exists()? && recover_locked(&admitted, key)?.as_ref() == Some(&next) {
        return Ok(());
    }
    if root.join(BACKUP).try_exists()? {
        return Err(invalid(
            "unrecognized payload recovery directory; no files changed",
        ));
    }
    let target = files::open_files(root, true)?;
    let previous = SignedBundle::read(&target)?;
    previous.verify(&target, key)?;
    if !version(&next, key)?
        .cmp_precedence(&version(&previous, key)?)
        .is_gt()
    {
        return Err(invalid(
            "payload update must advance the authenticated previous version",
        ));
    }
    roots::validate(&admitted)?;
    let mut journal = Journal {
        schema: 1,
        root: root.clone(),
        mode: admitted.mode,
        identity: admitted.identity,
        phase: Phase::Staging,
        previous,
        next,
    };
    save(root, &journal)?;
    let result: LanpathResult<()> = (|| {
        std::fs::create_dir(root.join(BACKUP))?;
        files::snapshot(&target, &root.join(BACKUP))?;
        let backup = files::open_files(&root.join(BACKUP), false)?;
        journal.previous.verify(&backup, key)?;
        roots::validate(&admitted)?;
        journal.phase = Phase::Prepared;
        save(root, &journal)?;
        let mut replacement = Files::new();
        for (index, name) in files::paths().enumerate() {
            before_replace(index)?;
            roots::validate(&admitted)?;
            replacement.insert(name.into(), files::replace(root, name, &source[name])?);
        }
        journal.next.verify(&replacement, key)?;
        roots::validate(&admitted)?;
        journal.phase = Phase::Committed;
        save(root, &journal)?;
        Ok(())
    })();
    drop(target);
    if let Err(error) = result {
        if let Err(recovery) = recover_locked(&admitted, key) {
            return Err(invalid(format!(
                "payload replacement failed: {error}; recovery retained and requires retry: {recovery}"
            )));
        }
        return Err(invalid(format!(
            "payload replacement failed; previous payload restored: {error}"
        )));
    }
    cleanup(&admitted)
}

#[cfg(test)]
#[path = "payload_tests.rs"]
mod tests;
