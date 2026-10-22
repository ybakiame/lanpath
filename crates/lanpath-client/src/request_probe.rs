use crate::{ClientConfig, fetch_resource_bytes_with_config};

/// Test a captured saved policy through the real GitHub resource pipeline.
///
/// Matches the desktop connectivity command: UTF-8 `api.github.com/zen`,
/// ten-second request timeout, and elapsed milliseconds including retries.
pub async fn test_github_connectivity(cfg: &ClientConfig) -> lanpath_common::LanpathResult<u128> {
    let started = std::time::Instant::now();
    let bytes = fetch_resource_bytes_with_config(
        cfg,
        "https://api.github.com/zen",
        std::time::Duration::from_secs(10),
    )
    .await?;
    std::str::from_utf8(&bytes).map_err(|error| {
        lanpath_common::LanpathError::Client(format!(
            "remote fetch returned non-UTF-8 body (https://api.github.com/zen): {error}"
        ))
    })?;
    Ok(started.elapsed().as_millis())
}
