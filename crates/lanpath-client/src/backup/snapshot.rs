use super::*;
use lanpath_common::LanpathResult;
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

pub fn capture(dir: &Path, appearance: Option<Appearance>) -> LanpathResult<Snapshot> {
    let mut files = BTreeMap::new();
    for root in ROOTS {
        collect(dir, root, &mut files)?;
    }
    let cfg = if let Some(bytes) = files.get("client.json") {
        let mut raw: serde_json::Value = serde_json::from_slice(bytes)?;
        if raw.get("request_proxy_mode").is_none() {
            raw["request_proxy_mode"] = if raw["fetch_via_local_proxy"] == true {
                "core"
            } else {
                "none"
            }
            .into();
        }
        serde_json::from_value::<crate::ClientConfig>(raw)?
    } else {
        crate::ClientConfig::default()
    };
    let mut cfg = cfg;
    cfg.data_dir = Default::default();
    cfg.core_binary = Default::default();
    cfg.mitm.ca_dir = Default::default();
    files.insert("client.json".into(), serde_json::to_vec(&cfg)?);
    for (name, bytes) in &mut files {
        if name.ends_with(".json") && !name.starts_with("rulesets/") {
            let mut value: serde_json::Value = serde_json::from_slice(bytes)?;
            rewrite_paths(&mut value, &dir.to_string_lossy(), "$LANPATH_DATA");
            *bytes = serde_json::to_vec(&value)?;
        }
    }
    let mut snapshot = Snapshot {
        summary: BackupSummary {
            version: 2,
            app_version: env!("CARGO_PKG_VERSION").into(),
            platform: std::env::consts::OS.into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            subscriptions: 0,
            profiles: 0,
            files: 0,
            bytes: 0,
            appearance,
        },
        files,
    };
    validate(&mut snapshot)?;
    Ok(snapshot)
}

fn collect(dir: &Path, name: &str, files: &mut BTreeMap<String, Vec<u8>>) -> LanpathResult<()> {
    let path = dir.join(name);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() {
        return Err(invalid("备份目录包含符号链接，请先移除链接"));
    }
    if meta.is_dir() {
        if name.ends_with(".json") || name.split('/').count() >= 8 {
            return Err(invalid("备份目录结构无效"));
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let leaf = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("备份文件名无效"))?;
            let next = format!("{name}/{leaf}");
            if !allowed(&next) {
                return Err(invalid("备份文件名无效"));
            }
            collect(dir, &next, files)?;
        }
    } else if meta.is_file() {
        if files.len() >= MAX_FILES || meta.len() > MAX_EXPANDED as u64 {
            return Err(invalid("备份超出大小限制"));
        }
        files.insert(name.into(), std::fs::read(path)?);
        check_size(files)?;
    } else {
        return Err(invalid("备份包含特殊文件"));
    }
    Ok(())
}

pub(super) fn rewrite_paths(value: &mut serde_json::Value, from: &str, to: &str) {
    if from.is_empty() {
        return;
    }
    match value {
        serde_json::Value::String(s)
            if s == from
                || s.strip_prefix(from)
                    .is_some_and(|r| r.starts_with(['/', '\\'])) =>
        {
            *s = format!("{to}{}", s[from.len()..].replace('\\', "/"));
        }
        serde_json::Value::Array(items) => {
            for item in items {
                rewrite_paths(item, from, to);
            }
        }
        serde_json::Value::Object(items) => {
            for item in items.values_mut() {
                rewrite_paths(item, from, to);
            }
        }
        _ => {}
    }
}

pub(super) fn validate(snapshot: &mut Snapshot) -> LanpathResult<()> {
    if !matches!(snapshot.summary.version, 1 | 2) {
        return Err(invalid("备份版本不受支持，请更新客户端"));
    }
    check_size(&snapshot.files)?;
    let mut folded = HashSet::new();
    for name in snapshot.files.keys() {
        if !super::allowed(name) || !folded.insert(name.to_lowercase()) {
            return Err(invalid("备份路径无效或在目标系统上存在重名"));
        }
    }
    let cfg: crate::ClientConfig = serde_json::from_slice(
        snapshot
            .files
            .get("client.json")
            .ok_or_else(|| invalid("备份缺少客户端配置"))?,
    )?;
    cfg.validate_request_proxy()?;
    let read = |name: &str| -> LanpathResult<serde_json::Value> {
        Ok(match snapshot.files.get(name) {
            Some(b) => serde_json::from_slice(b)?,
            None => serde_json::json!([]),
        })
    };
    let subs: Vec<crate::Subscription> = serde_json::from_value(read("subscriptions.json")?)?;
    let profiles: Vec<crate::Profile> = serde_json::from_value(read("profiles.json")?)?;
    let sub_ids: HashSet<_> = subs.iter().map(|s| s.id).collect();
    let profile_ids: HashSet<_> = profiles.iter().map(|p| p.id).collect();
    if sub_ids.len() != subs.len()
        || profile_ids.len() != profiles.len()
        || cfg
            .active_subscription_id
            .is_some_and(|id| !sub_ids.contains(&id))
        || subs
            .iter()
            .any(|s| s.profile_id.is_some_and(|id| !profile_ids.contains(&id)))
    {
        return Err(invalid("备份中的订阅或模板关联无效"));
    }
    if let Some(bytes) = snapshot.files.get("config_slices.json") {
        serde_json::from_slice::<crate::config_slices::ConfigSlices>(bytes)?.validate()?;
    }
    if let Some(bytes) = snapshot.files.get("local_override.json") {
        let _: crate::LocalOverride = serde_json::from_slice(bytes)?;
    }
    if let Some(bytes) = snapshot.files.get("remotes.json") {
        let _: Vec<crate::RemoteResource> = serde_json::from_slice(bytes)?;
    }
    if let Some(a) = &snapshot.summary.appearance
        && (!matches!(a.theme.as_str(), "light" | "dark" | "system")
            || a.accent.len() > 32
            || !a.accent.chars().all(|c| c.is_ascii_lowercase()))
    {
        return Err(invalid("备份中的外观配置无效"));
    }
    snapshot.summary.subscriptions = subs.len();
    snapshot.summary.profiles = profiles.len();
    snapshot.summary.files = snapshot.files.len();
    snapshot.summary.bytes = snapshot.files.values().map(Vec::len).sum();
    Ok(())
}

pub(super) fn relocate(snapshot: &mut Snapshot, dir: &Path) -> LanpathResult<()> {
    for (name, bytes) in &mut snapshot.files {
        if name.ends_with(".json") && !name.starts_with("rulesets/") {
            let mut value: serde_json::Value = serde_json::from_slice(bytes)?;
            rewrite_paths(&mut value, "$LANPATH_DATA", &dir.to_string_lossy());
            *bytes = serde_json::to_vec(&value)?;
        }
    }
    let current = crate::ClientConfig::load_for_requests(dir)?;
    let mut cfg: crate::ClientConfig = serde_json::from_slice(&snapshot.files["client.json"])?;
    cfg.data_dir = dir.into();
    cfg.core_binary = current.core_binary;
    cfg.mitm.ca_dir = dir.join("certs");
    cfg.system_proxy_enabled = false;
    cfg.tun_enabled = false;
    cfg.mitm_enabled = false;
    if snapshot.summary.platform != std::env::consts::OS {
        cfg.tun_stack = crate::ClientConfig::default().tun_stack;
    }
    snapshot
        .files
        .insert("client.json".into(), serde_json::to_vec_pretty(&cfg)?);
    Ok(())
}
