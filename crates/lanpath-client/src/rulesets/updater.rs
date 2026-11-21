//! 规则集条件 HTTP 更新器（支持 ETag / If-Modified-Since / SHA-256 比对与请求策略）。

use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lanpath_common::{LanpathError, LanpathResult};
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    RuleSetEntry, RuleSetRemoteState, RuleSetSource, RuleSetStore, RuleSetUpdateOutcome,
    RuleSetUpdateStatus, collect_referenced_rule_set_tags,
};
use crate::{ClientConfig, RemoteFetchContext};

const UPDATE_TIMEOUT: Duration = Duration::from_secs(60);

/// 更新单条远程规则集（手动规则集返回 `Skipped`）。
pub async fn update_one(
    data_dir: &Path,
    entry: &mut RuleSetEntry,
    config_override: Option<&ClientConfig>,
) -> LanpathResult<RuleSetUpdateStatus> {
    let RuleSetSource::Remote { url, .. } = &entry.source else {
        return Ok(RuleSetUpdateStatus::Skipped);
    };
    let loaded_cfg;
    let cfg = match config_override {
        Some(cfg) => cfg,
        None => {
            loaded_cfg = ClientConfig::load_for_requests(data_dir)?;
            &loaded_cfg
        }
    };
    let effective_url = crate::apply_github_proxy_prefix(
        &crate::normalize_resource_url(url.trim()),
        &cfg.github_proxy_prefix,
    );
    let client = crate::request_network::build_client(UPDATE_TIMEOUT, cfg)?;
    let store = RuleSetStore::new(data_dir.to_path_buf());
    let path = store.backing_file_path(&entry.id, entry.file_format());

    let old_bytes = std::fs::read(&path).ok().filter(|bytes| !bytes.is_empty());
    let can_validate = old_bytes
        .as_deref()
        .is_some_and(|bytes| sha256_hex(bytes) == entry.remote_state.content_sha256)
        && entry.remote_state.validator_url == effective_url;

    let mut request = client.get(&effective_url);
    if can_validate {
        if let Some(etag) = &entry.remote_state.etag {
            request = request.header(IF_NONE_MATCH, etag);
        } else if let Some(modified) = &entry.remote_state.last_modified {
            request = request.header(IF_MODIFIED_SINCE, modified);
        }
    }

    let retry = request
        .try_clone()
        .ok_or_else(|| LanpathError::Client("无法构建规则集下载请求".into()))?;
    let response = match request.send().await {
        Ok(resp) => resp,
        Err(_) => retry.send().await.map_err(network_error)?,
    };

    let now = unix_now();
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        if !can_validate
            || (entry.remote_state.etag.is_none() && entry.remote_state.last_modified.is_none())
        {
            return Err(LanpathError::Client(
                "规则集源返回了无法验证的 304 响应".into(),
            ));
        }
        entry.remote_state.checked_at = now;
        return Ok(RuleSetUpdateStatus::Skipped);
    }

    if !response.status().is_success() {
        return Err(LanpathError::Client(format!(
            "规则集 `{}` 下载失败：HTTP {}",
            entry.tag,
            response.status()
        )));
    }

    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned)
    };
    let etag = header(ETAG);
    let last_modified = header(LAST_MODIFIED);
    let remote_updated_at = last_modified
        .as_deref()
        .and_then(|text| crate::parse_http_date(text).ok())
        .unwrap_or(0);

    let bytes = response.bytes().await.map_err(network_error)?;
    if bytes.is_empty() {
        return Err(LanpathError::Client(format!(
            "规则集 `{}` 返回空内容，已保留旧缓存",
            entry.tag
        )));
    }

    let changed = old_bytes.as_deref() != Some(bytes.as_ref());
    if changed {
        let parent = path
            .parent()
            .ok_or_else(|| LanpathError::Client("规则集缓存路径无效".into()))?;
        std::fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(&path).map_err(|error| error.error)?;
    }

    let updated_at = if changed {
        now
    } else {
        entry.remote_state.updated_at
    };
    entry.remote_state = RuleSetRemoteState {
        content_sha256: sha256_hex(&bytes),
        checked_at: now,
        updated_at,
        remote_updated_at,
        validator_url: effective_url,
        etag,
        last_modified,
    };

    Ok(if changed {
        RuleSetUpdateStatus::Updated
    } else {
        RuleSetUpdateStatus::Skipped
    })
}

/// 依次更新所有远程规则集（单条失败不中断批次）。
pub async fn update_all(
    data_dir: &Path,
    items: &mut [RuleSetEntry],
    config_override: Option<&ClientConfig>,
) -> RuleSetUpdateOutcome {
    let mut outcome = RuleSetUpdateOutcome::default();
    for entry in items.iter_mut() {
        if !matches!(entry.source, RuleSetSource::Remote { .. }) {
            continue;
        }
        match update_one(data_dir, entry, config_override).await {
            Ok(RuleSetUpdateStatus::Updated) => outcome.updated += 1,
            Ok(RuleSetUpdateStatus::Skipped) => outcome.skipped += 1,
            Ok(RuleSetUpdateStatus::Failed) | Err(_) => outcome.failed += 1,
        }
    }
    outcome
}

/// 启动/合成阶段补齐「被配置引用但本地文件缺失」的受管规则集（best-effort）。
pub async fn prepare_referenced_missing(
    data_dir: &Path,
    config: &Value,
    entries: &mut [RuleSetEntry],
    requests: &RemoteFetchContext,
) -> (bool, Vec<String>) {
    let referenced = collect_referenced_rule_set_tags(config);
    if referenced.is_empty() || entries.is_empty() {
        return (false, Vec::new());
    }

    let store = RuleSetStore::new(data_dir.to_path_buf());
    let mut changed = false;
    let mut warnings = Vec::new();

    for entry in entries.iter_mut() {
        if !referenced.contains(entry.tag.as_str()) || store.has_backing_file(entry) {
            continue;
        }
        match &entry.source {
            RuleSetSource::Manual { content } => {
                let path = store.backing_file_path(&entry.id, super::RuleSetFormat::Source);
                if let Err(error) = requests
                    .check_write(&path)
                    .and_then(|()| write_atomic(&path, content.as_bytes()))
                {
                    warnings.push(format!("恢复手动规则集 `{}` 失败：{error}", entry.tag));
                }
            }
            RuleSetSource::Remote { url, .. } => {
                if !requests.allows_network() {
                    warnings.push(format!(
                        "规则集 `{}` 缺少本地文件且当前仅使用缓存",
                        entry.tag
                    ));
                    continue;
                }
                let path = store.backing_file_path(&entry.id, entry.file_format());
                if let Err(error) = requests.check_write(&path) {
                    warnings.push(format!("写入规则集 `{}` 被策略拒绝：{error}", entry.tag));
                    continue;
                }
                match requests.fetch_bytes(url.trim(), UPDATE_TIMEOUT).await {
                    Ok(bytes) if !bytes.is_empty() => {
                        if requests.check_write(&path).is_ok()
                            && write_atomic(&path, &bytes).is_ok()
                        {
                            let now = unix_now();
                            entry.remote_state.content_sha256 = sha256_hex(&bytes);
                            entry.remote_state.checked_at = now;
                            entry.remote_state.updated_at = now;
                            changed = true;
                        } else {
                            warnings.push(format!("保存缺失规则集 `{}` 失败", entry.tag));
                        }
                    }
                    Ok(_) => {
                        warnings.push(format!("下载缺失规则集 `{}` 返回空内容", entry.tag));
                    }
                    Err(error) => {
                        warnings.push(format!("下载缺失规则集 `{}` 失败：{error}", entry.tag));
                    }
                }
            }
        }
    }

    (changed, warnings)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> LanpathResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| LanpathError::Client("规则集缓存路径无效".into()))?;
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn network_error(error: reqwest::Error) -> LanpathError {
    LanpathError::Client(format!(
        "规则集请求失败，请检查请求设置与网络连通性：{}",
        error.without_url()
    ))
}
