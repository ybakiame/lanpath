//! 主机名拦截判定。
//!
//! CONNECT（按目标主机名）与 TLS ClientHello（按 SNI）两个 hudsucker 钩子
//! 共用的白名单 / 排除列表匹配逻辑。

use crate::config::MitmConfig;

/// 依据配置判断 `host` 是否应拦截。
///
/// 排除列表（`excluded_hostnames`）优先级高于白名单：命中排除的主机一律不拦截；
/// 白名单为空时除排除命中外全量拦截。
pub(crate) fn should_intercept_host(config: &MitmConfig, host: &str) -> bool {
    if config.excluded_hostnames.iter().any(|m| m.matches(host)) {
        return false;
    }
    if config.hostnames.is_empty() {
        return true;
    }
    config.hostnames.iter().any(|m| m.matches(host))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HostnameMatcher;

    /// 构造带指定 hostnames / excluded_hostnames 的配置。
    fn test_config(
        hostnames: Vec<HostnameMatcher>,
        excluded_hostnames: Vec<HostnameMatcher>,
    ) -> MitmConfig {
        MitmConfig {
            hostnames,
            excluded_hostnames,
            ..MitmConfig::default()
        }
    }

    /// CONNECT 请求：uri 为 `host:443` 形式，返回值与
    /// `should_intercept_connect` 内部提取的 host 一致（`vip.iqiyi.com:443` → `vip.iqiyi.com`）。
    fn connect_host(host: &str) -> String {
        host.split(':').next().unwrap_or_default().to_string()
    }

    #[test]
    fn should_intercept_connect_excluded_host_wins_over_whitelist() {
        // `*.iqiyi.com` 在白名单，`vip.iqiyi.com` 同时被排除 → 排除命中时不拦截。
        let config = test_config(
            vec![HostnameMatcher::Suffix("iqiyi.com".to_string())],
            vec![HostnameMatcher::Suffix("vip.iqiyi.com".to_string())],
        );
        assert!(
            !should_intercept_host(&config, &connect_host("vip.iqiyi.com")),
            "排除命中即使白名单命中也不应拦截"
        );
    }

    #[test]
    fn should_intercept_connect_whitelist_hit_intercepts() {
        let config = test_config(
            vec![HostnameMatcher::Suffix("iqiyi.com".to_string())],
            vec![HostnameMatcher::Exact("blocked.example.com".to_string())],
        );
        assert!(
            should_intercept_host(&config, &connect_host("api.iqiyi.com")),
            "未排除且白名单命中应拦截"
        );
        assert!(
            !should_intercept_host(&config, &connect_host("blocked.example.com")),
            "白名单未命中不应拦截"
        );
    }

    #[test]
    fn should_intercept_connect_empty_whitelist_intercepts_unless_excluded() {
        let config = test_config(
            Vec::new(),
            vec![HostnameMatcher::Suffix("vip.iqiyi.com".to_string())],
        );
        assert!(
            should_intercept_host(&config, &connect_host("example.com")),
            "白名单为空时未命中排除应全量拦截"
        );
        assert!(
            !should_intercept_host(&config, &connect_host("vip.iqiyi.com")),
            "白名单为空时命中排除仍不拦截"
        );
    }

    // These test only the shared host matcher. CONNECT binding and ClientHello
    // identity selection are exercised by the real TLS transport regressions.

    #[test]
    fn host_matcher_whitelist_hit_matches() {
        let config = test_config(
            vec![HostnameMatcher::Suffix("iqiyi.com".to_string())],
            vec![HostnameMatcher::Exact("blocked.example.com".to_string())],
        );
        assert!(
            should_intercept_host(&config, "api.iqiyi.com"),
            "an allowed, non-excluded hostname must match"
        );
        assert!(
            !should_intercept_host(&config, "other.example.com"),
            "a hostname outside the whitelist must not match"
        );
    }

    #[test]
    fn host_matcher_exclusion_wins_over_whitelist() {
        let config = test_config(
            vec![HostnameMatcher::Suffix("iqiyi.com".to_string())],
            vec![HostnameMatcher::Suffix("vip.iqiyi.com".to_string())],
        );
        assert!(
            !should_intercept_host(&config, "vip.iqiyi.com"),
            "an excluded hostname must not match even when whitelisted"
        );
    }

    #[test]
    fn host_matcher_empty_input_obeys_whitelist() {
        // Empty input is a matcher input, not the identity used for absent SNI.
        let whitelisted = test_config(
            vec![HostnameMatcher::Suffix("iqiyi.com".to_string())],
            Vec::new(),
        );
        assert!(
            !should_intercept_host(&whitelisted, ""),
            "empty input must not match this nonempty whitelist"
        );
        // An empty whitelist accepts any input not matched by an exclusion.
        let open = test_config(Vec::new(), Vec::new());
        assert!(
            should_intercept_host(&open, ""),
            "empty input matches an empty whitelist with no exclusions"
        );
    }
}
