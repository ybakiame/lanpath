//! One-time v1 request-policy upgrade. Backup remains available for rollback.
use std::{io::Write, path::Path};

use lanpath_common::LanpathResult;

pub(super) fn migrate_request_proxy(
    path: &Path,
    original: &str,
    raw: &mut serde_json::Value,
) -> LanpathResult<()> {
    if raw.get("request_proxy_mode").is_some() {
        return Ok(());
    }
    let Some(object) = raw.as_object_mut() else {
        return Ok(());
    };
    // Every pre-v1 persisted config used false when the legacy flag was absent.
    let mode = if object
        .get("fetch_via_local_proxy")
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        "core"
    } else {
        "none"
    };
    object.insert("request_proxy_mode".into(), mode.into());
    object.insert("request_proxy_url".into(), "".into());
    object.remove("fetch_via_local_proxy");
    // Validate before touching the original. An existing backup is never overwritten.
    let _: super::ClientConfig = serde_json::from_value(raw.clone())?;
    let backup = if crate::settings::is_app_settings(path) {
        path.with_file_name("app.request-proxy-v1.backup.toml")
    } else {
        path.with_file_name("client.request-proxy-v1.backup.json")
    };
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut backup_file = tempfile::NamedTempFile::new_in(parent)?;
    backup_file.write_all(original.as_bytes())?;
    backup_file.as_file().sync_all()?;
    match backup_file.persist_noclobber(&backup) {
        Ok(_) => {}
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            // A directory or incomplete backup must not be treated as a rollback file.
            let backup_text = std::fs::read_to_string(&backup)?;
            if crate::settings::is_app_settings(path) {
                backup_text
                    .parse::<toml_edit::DocumentMut>()
                    .map_err(|error| {
                        lanpath_common::LanpathError::Config(format!(
                            "{}: {error}",
                            backup.display()
                        ))
                    })?;
            } else {
                let _: serde_json::Value = serde_json::from_str(&backup_text)?;
            }
        }
        Err(e) => return Err(e.error.into()),
    }
    if crate::settings::is_app_settings(path) {
        crate::settings::persist_client(path, raw)?;
    } else {
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(serde_json::to_string_pretty(raw)?.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|e| e.error)?;
    }
    Ok(())
}
