//! Owned refresh attempt ledger and drop-safe sequential fetch orchestration.
use std::{
    collections::BTreeMap,
    io::Write,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{FetchReport, RemoteFetchContext, RemoteManager, RemoteResource};

#[derive(Default, Serialize, Deserialize)]
struct Schedule {
    attempts: BTreeMap<String, u64>,
}

fn identity(resource: &RemoteResource) -> String {
    // Length framing avoids ambiguous names/URLs, and kind is deliberately included
    // independently of the activation cache's module identity.
    let mut digest = Sha256::new();
    for part in [
        resource.name.as_str(),
        resource.url.as_str(),
        match resource.kind {
            super::RemoteKind::Script => "script",
            super::RemoteKind::Snippet => "snippet",
        },
        match resource.dialect {
            lanpath_script::ScriptDialect::Surge => "surge",
            lanpath_script::ScriptDialect::QuantumultX => "quantumult-x",
            lanpath_script::ScriptDialect::Loon => "loon",
            lanpath_script::ScriptDialect::Stash => "stash",
        },
    ] {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

impl Schedule {
    fn load(path: &Path) -> LanpathResult<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| schedule_error()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err(schedule_error()),
        }
    }

    fn due(&self, resource: &RemoteResource, key: &str, now: u64) -> bool {
        let interval = if resource.update_interval_secs == 0 {
            86_400
        } else {
            resource.update_interval_secs
        };
        resource.enabled
            && self.attempts.get(key).is_none_or(|last| {
                // A wall-clock rollback must not freeze automatic refresh until time catches up.
                now < *last || now - *last >= interval
            })
    }
}

fn schedule_error() -> LanpathError {
    LanpathError::Client(
        "remote refresh schedule could not be read or saved; previous schedule retained".into(),
    )
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> LanpathResult<()> {
    atomic_write_with_context(path, bytes, None)
}

pub(crate) fn atomic_write_with_context(
    path: &Path,
    bytes: &[u8],
    context: Option<&RemoteFetchContext>,
) -> LanpathResult<()> {
    let check = || match context {
        Some(context) => context.check_write(path),
        None => Ok(()),
    };
    check()?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    check()?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    check()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) async fn fetch(
    manager: &RemoteManager,
    resources: &[RemoteResource],
    context: Option<&RemoteFetchContext>,
) -> LanpathResult<Option<FetchReport>> {
    fetch_at(manager, resources, context, now).await
}

async fn fetch_at(
    manager: &RemoteManager,
    resources: &[RemoteResource],
    context: Option<&RemoteFetchContext>,
    clock: impl Fn() -> u64,
) -> LanpathResult<Option<FetchReport>> {
    let automatic = context.is_some();
    let path = crate::resource_storage::cache_path(&manager.data_dir, "remote_refresh.json");
    // Never silently reset an unreadable ledger (including on manual refresh).
    let mut schedule = Schedule::load(&path)?;
    // Freeze eligibility before admission: downloads may cross another resource's
    // interval, but that resource belongs to the next batch, not this lease.
    let admitted_at = clock();
    let batch: Vec<_> = resources
        .iter()
        .filter(|resource| resource.enabled)
        .map(|resource| (resource, identity(resource)))
        .filter(|(resource, key)| !automatic || schedule.due(resource, key, admitted_at))
        .collect();
    // Admit the entire selected batch before the first request or mutation.
    for (resource, _) in &batch {
        manager.check_publication(resource, context)?;
    }
    let mut report = FetchReport::default();
    let attempted = !batch.is_empty();
    for (resource, key) in batch {
        let fetched = if automatic {
            match tokio::time::timeout(
                Duration::from_secs(30),
                manager.fetch_one(resource, context),
            )
            .await
            {
                Ok(report) => report?,
                Err(_) => FetchReport {
                    warnings: vec![
                        "automatic resource download timed out; previous complete cache retained"
                            .into(),
                    ],
                    ..Default::default()
                },
            }
        } else {
            manager.fetch_one(resource, None).await?
        };
        if let Some(context) = context {
            manager.check_publication(resource, Some(context))?;
        }
        report.fetched += fetched.fetched;
        report.scripts += fetched.scripts;
        report.rewrites += fetched.rewrites;
        report.tasks += fetched.tasks;
        report.hostnames += fetched.hostnames;
        report.warnings.extend(fetched.warnings);
        // There is no await between publication, recording this completed attempt,
        // and starting the next resource. Dropping the future never marks unstarted work.
        schedule.attempts.insert(key, clock());
        let bytes = serde_json::to_vec_pretty(&schedule).map_err(|_| schedule_error())?;
        atomic_write_with_context(&path, &bytes, context).map_err(|_| schedule_error())?;
    }
    Ok((attempted || !automatic).then_some(report))
}

#[cfg(all(test, feature = "mitm"))]
mod tests;
