//! Versioned portable snapshots. Only allowlisted configuration is included.
use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod archive;
pub mod path_relocation;
mod preview;
mod resource_layout;
mod service;
#[cfg(test)]
mod service_tests;
mod snapshot;
#[cfg(test)]
mod tests;
mod transaction;

pub use archive::{decode, encode, generate_password};
pub use preview::{BackupPreview, PreviewCache, RestoreInput};
pub use service::BackupService;
pub use snapshot::capture;
pub use transaction::{
    apply_pending, apply_pending_at_startup, apply_validated_pending, prepare_restore,
    recover_interrupted, validate_pending,
};

pub const MAX_ARCHIVE: usize = 16 * 1024 * 1024;
const MAX_EXPANDED: usize = 64 * 1024 * 1024;
const MAX_FILES: usize = 1024;
pub(super) const ROOTS: &[&str] = &[
    "app.toml",
    "subscriptions.toml",
    "rules.toml",
    ".cache",
    "resources",
    "client.json",
    "subscriptions.json",
    "profiles.json",
    "remotes.json",
    "subscription_cache",
    "profile_cache",
    "scripts",
    "remote_cache",
    "script_store",
    "rulesets",
    "overwrite",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appearance {
    pub theme: String,
    pub accent: String,
    pub pure_black: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSummary {
    pub version: u32,
    pub app_version: String,
    pub platform: String,
    pub created_at: String,
    pub subscriptions: usize,
    pub profiles: usize,
    pub files: usize,
    pub bytes: usize,
    pub appearance: Option<Appearance>,
}

pub struct Snapshot {
    pub summary: BackupSummary,
    pub(super) files: BTreeMap<String, Vec<u8>>,
}

pub(super) fn invalid(message: &str) -> LanpathError {
    LanpathError::Client(message.to_string())
}

pub(super) fn allowed(name: &str) -> bool {
    if name.len() > 1024
        || name.contains(['\\', ':', '<', '>', '"', '|', '?', '*'])
        || name.starts_with('/')
    {
        return false;
    }
    let parts: Vec<_> = name.split('/').collect();
    if parts[0] == ".cache"
        && parts.len() > 1
        && !matches!(
            parts[1],
            "remotes" | "subscriptions" | "profiles" | "scripts"
        )
    {
        return false;
    }
    if parts[0] == "resources" && parts.len() > 1 && parts[1] != "imports" {
        return false;
    }
    if parts[0] == "overwrite"
        && ((parts.len() == 2 && parts[1] != "sing-box")
            || (parts.len() == 3
                && (parts[1] != "sing-box"
                    || crate::singbox_overwrite::SingboxSliceKind::from_file_name(parts[2])
                        .is_none()))
            || parts.len() > 3)
    {
        return false;
    }

    parts.len() <= 8
        && !parts.iter().any(|p| {
            p.is_empty()
                || p.len() > 255
                || p.ends_with([' ', '.'])
                || p.chars().any(char::is_control)
                || reserved(p)
        })
        && ROOTS.contains(&parts[0])
        && ((parts.len() == 1
            && (parts[0].ends_with(".json")
                || matches!(parts[0], "app.toml" | "subscriptions.toml" | "rules.toml")))
            || (parts.len() > 1 && !parts[0].ends_with(".json") && !parts[0].ends_with(".toml")))
}

fn reserved(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

pub(super) fn check_size(files: &BTreeMap<String, Vec<u8>>) -> LanpathResult<()> {
    if files.len() > MAX_FILES || files.values().map(Vec::len).sum::<usize>() > MAX_EXPANDED {
        return Err(invalid("备份内容过大：最多 1024 个文件、64 MiB 解压数据"));
    }
    Ok(())
}
