//! HTTP validators describe content versions, not publication timestamps.
use super::{CustomRuleSet, CustomRuleSetSource, RuleSetRemoteState, RuleSetUpdateStatus};
use lanpath_common::{LanpathError, LanpathResult};
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(super) async fn update(
    data_dir: &Path,
    path: &Path,
    rs: &mut CustomRuleSet,
) -> LanpathResult<RuleSetUpdateStatus> {
    let CustomRuleSetSource::Remote { url, .. } = &rs.source else {
        return Ok(RuleSetUpdateStatus::Skipped);
    };
    let cfg = crate::ClientConfig::load_for_requests(data_dir)?;
    let url = crate::apply_github_proxy_prefix(
        &crate::normalize_resource_url(url),
        &cfg.github_proxy_prefix,
    );
    let client = crate::request_network::build_client(Duration::from_secs(60), &cfg)?;
    let old_bytes = std::fs::read(path).ok().filter(|bytes| !bytes.is_empty());
    let validators = old_bytes
        .as_deref()
        .is_some_and(|bytes| content_hash(bytes) == rs.remote_state.content_sha256)
        && rs.remote_state.validator_url == url;
    let mut request = client.get(&url);
    if validators {
        if let Some(etag) = &rs.remote_state.etag {
            request = request.header(IF_NONE_MATCH, etag);
        } else if let Some(modified) = &rs.remote_state.last_modified {
            request = request.header(IF_MODIFIED_SINCE, modified);
        }
    }
    let retry = request
        .try_clone()
        .ok_or_else(|| LanpathError::Client("无法创建规则集请求".into()))?;
    let response = match request.send().await {
        Ok(resp) => resp,
        Err(_) => retry.send().await.map_err(network_error)?,
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        if !validators
            || (rs.remote_state.etag.is_none() && rs.remote_state.last_modified.is_none())
        {
            return Err(LanpathError::Client(
                "规则集源返回了无法验证的 304 响应".into(),
            ));
        }
        rs.remote_state.checked_at = now;
        return Ok(RuleSetUpdateStatus::Skipped);
    }
    if !response.status().is_success() {
        return Err(LanpathError::Client(format!(
            "规则集更新返回 HTTP {}",
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
    let remote_time = last_modified
        .as_deref()
        .and_then(|text| crate::parse_http_date(text).ok())
        .unwrap_or(0);
    let bytes = response.bytes().await.map_err(network_error)?;
    if bytes.is_empty() {
        return Err(LanpathError::Client(
            "规则集源返回空内容，已保留旧缓存".into(),
        ));
    }
    let changed = old_bytes.as_deref() != Some(bytes.as_ref());
    if changed {
        let parent = path
            .parent()
            .ok_or_else(|| LanpathError::Client("规则集路径无效".into()))?;
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| e.error)?;
        rs.last_updated = now;
    }
    rs.remote_updated_at = remote_time;
    rs.remote_state = RuleSetRemoteState {
        content_sha256: content_hash(&bytes),
        checked_at: now,
        validator_url: url,
        etag,
        last_modified,
    };
    Ok(if changed {
        RuleSetUpdateStatus::Updated
    } else {
        RuleSetUpdateStatus::Skipped
    })
}

fn content_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn network_error(error: reqwest::Error) -> LanpathError {
    LanpathError::Client(format!(
        "规则集请求失败，请检查网络请求和 GitHub 访问设置：{}",
        error.without_url()
    ))
}

#[cfg(test)]
#[path = "tests/http_update_tests.rs"]
mod tests;
