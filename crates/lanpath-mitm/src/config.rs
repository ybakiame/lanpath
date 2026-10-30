//! MITM 代理配置与主机名匹配。

use std::net::SocketAddr;
use std::path::PathBuf;

use crate::upstream::UpstreamProxy;

/// 主机名匹配器：精确匹配、域名后缀匹配或通配符正则匹配。
#[derive(Debug, Clone)]
pub enum HostnameMatcher {
    /// 精确相等（如 `example.com`）。
    Exact(String),
    /// A host pattern constrained to one destination port.
    Port {
        host: Box<HostnameMatcher>,
        port: u16,
    },
    /// 匹配裸后缀本身及其所有子域（如 `*.example.com`）。
    Suffix(String),
    /// 含任意位置 `*` 的通配模式（如 `api-*.example.com` / `*xmind.*`），
    /// 转正则后全串锚定匹配；`pattern` 保留原始文本用于比较/展示。
    Wildcard {
        pattern: String,
        regex: regex::Regex,
    },
}

impl PartialEq for HostnameMatcher {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HostnameMatcher::Exact(a), HostnameMatcher::Exact(b)) => a == b,
            (HostnameMatcher::Suffix(a), HostnameMatcher::Suffix(b)) => a == b,
            (
                HostnameMatcher::Port { host: a, port: ap },
                HostnameMatcher::Port { host: b, port: bp },
            ) => a == b && ap == bp,
            (
                HostnameMatcher::Wildcard { pattern: a, .. },
                HostnameMatcher::Wildcard { pattern: b, .. },
            ) => a == b,
            _ => false,
        }
    }
}
impl Eq for HostnameMatcher {}

impl HostnameMatcher {
    /// Match an authority (or a bare TLS hostname, whose default port is 443).
    pub fn matches(&self, authority: &str) -> bool {
        let (host, port) = split_authority(authority);
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        match self {
            HostnameMatcher::Exact(exact) => {
                host.eq_ignore_ascii_case(exact.trim_matches(['[', ']']).trim_end_matches('.'))
            }
            HostnameMatcher::Suffix(suffix) => {
                host.eq_ignore_ascii_case(suffix)
                    || host
                        .strip_suffix(&suffix.to_ascii_lowercase())
                        .is_some_and(|prefix| prefix.ends_with('.'))
            }
            HostnameMatcher::Wildcard { regex, .. } => regex.is_match(&host),
            HostnameMatcher::Port {
                host: matcher,
                port: expected,
            } => port.unwrap_or(443) == *expected && matcher.matches(&host),
        }
    }

    /// Parse host wildcards and optional destination ports without losing their association.
    pub fn from_pattern(pattern: &str) -> Self {
        let pattern = pattern.trim().to_ascii_lowercase();
        let (host, port) = split_authority(&pattern);
        if let Some(port) = port {
            return Self::Port {
                host: Box::new(Self::from_pattern(host)),
                port,
            };
        }
        let host = pattern
            .strip_suffix(":*")
            .unwrap_or(host)
            .trim_end_matches('.');
        if let Some(suffix) = host.strip_prefix("*.")
            && !suffix.contains('*')
        {
            return Self::Suffix(suffix.to_string());
        }
        if host.contains('*') {
            let regex_src = format!("^{}$", regex::escape(host).replace(r"\*", ".*"));
            if let Ok(regex) = regex::Regex::new(&regex_src) {
                return Self::Wildcard {
                    pattern: host.to_string(),
                    regex,
                };
            }
        }
        Self::Exact(host.to_string())
    }
}

fn split_authority(authority: &str) -> (&str, Option<u16>) {
    if let Some(rest) = authority.strip_prefix('[')
        && let Some((host, suffix)) = rest.split_once(']')
    {
        return (host, suffix.strip_prefix(':').and_then(|p| p.parse().ok()));
    }
    if authority.bytes().filter(|b| *b == b':').count() == 1
        && let Some((host, port)) = authority.rsplit_once(':')
        && let Ok(port) = port.parse()
    {
        return (host, Some(port));
    }
    (authority, None)
}

/// MITM 代理配置。
#[derive(Debug, Clone)]
pub struct MitmConfig {
    /// 监听地址。`127.0.0.1:0` 表示随机空闲端口。
    pub listen_addr: SocketAddr,
    /// 存放 MITM CA（`ca.crt` / `ca.key`）的目录。
    pub ca_dir: PathBuf,
    /// 需要代理拦截的主机名列表。
    pub hostnames: Vec<HostnameMatcher>,
    /// 主机名排除列表：命中排除的主机不拦截（优先级高于 `hostnames` 白名单）。
    pub excluded_hostnames: Vec<HostnameMatcher>,
    /// Static rewrite and recorder body cap; selected Surge scripts use their own
    /// declared limits within a separate 32 MiB inspection envelope.
    pub max_body_size: usize,
    /// 是否启用流量记录。
    pub record_enabled: bool,
    /// 脚本钩子使用的脚本方言。
    pub script_dialect: lanpath_script::ScriptDialect,
    /// 上游去向：直连或经父代理（HTTP CONNECT / SOCKS5）转发。
    pub upstream: UpstreamProxy,
}

impl Default for MitmConfig {
    fn default() -> Self {
        Self {
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            ca_dir: PathBuf::new(),
            hostnames: Vec::new(),
            excluded_hostnames: Vec::new(),
            max_body_size: 131_072,
            record_enabled: true,
            script_dialect: lanpath_script::ScriptDialect::Surge,
            upstream: UpstreamProxy::Direct,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_matcher_matches_only_identical_host() {
        let m = HostnameMatcher::Exact("example.com".to_string());
        assert!(m.matches("example.com"));
        assert!(!m.matches("www.example.com"));
        assert!(!m.matches("example.com.evil.com"));
        assert!(!m.matches("notexample.com"));
        assert!(m.matches("Example.com:443"));
    }

    #[test]
    fn suffix_matcher_matches_host_and_subdomains() {
        let m = HostnameMatcher::Suffix("example.com".to_string());
        assert!(m.matches("example.com"));
        assert!(m.matches("www.example.com"));
        assert!(m.matches("a.b.example.com"));
        assert!(!m.matches("notexample.com"));
        assert!(!m.matches("example.com.evil.com"));
    }

    #[test]
    fn from_pattern_maps_wildcard_and_exact() {
        let wildcard = HostnameMatcher::from_pattern("*.example.com");
        assert_eq!(wildcard, HostnameMatcher::Suffix("example.com".to_string()));
        assert!(wildcard.matches("www.example.com"));
        assert!(wildcard.matches("example.com"));
        assert!(!wildcard.matches("example.org"));

        assert_eq!(
            HostnameMatcher::from_pattern("example.com"),
            HostnameMatcher::Exact("example.com".to_string())
        );
    }

    #[test]
    fn mid_string_wildcard_matches_via_regex() {
        let m = HostnameMatcher::from_pattern("api-*.facereplacerext.com");
        assert!(m.matches("api-cn.facereplacerext.com"));
        assert!(m.matches("api-.facereplacerext.com"));
        assert!(!m.matches("apicn.facereplacerext.com"));
        assert!(!m.matches("api-cn.facereplacerext.com.evil.com"));

        let m = HostnameMatcher::from_pattern("*xmind.*");
        assert!(m.matches("www.xmind.com"));
        assert!(m.matches("xmind.app"));
        assert!(!m.matches("xmind2.com"));

        // 尾部通配与 IP 形态。
        let m = HostnameMatcher::from_pattern("api.meiease.*");
        assert!(m.matches("api.meiease.com"));
        assert!(m.matches("api.meiease.cn"));
        // 贪婪通配语义（与 QX/Loon 一致）：`.` 后任意串均命中。
        assert!(m.matches("api.meiease.com.evil.org"));
        assert!(!m.matches("api.meiease2.com"));
        let m = HostnameMatcher::from_pattern("39.156.12*.*");
        assert!(m.matches("39.156.123.46"));
        assert!(!m.matches("39.155.13.46"));
    }

    #[test]
    fn port_constraints_stay_attached_to_their_host() {
        let matcher = HostnameMatcher::from_pattern("*.skyjos.com:58080");
        assert!(matcher.matches("api.skyjos.com:58080"));
        assert!(!matcher.matches("api.skyjos.com:443"));
        assert!(!matcher.matches("other.test:58080"));
        let default_tls = HostnameMatcher::from_pattern("Example.COM:443");
        assert!(default_tls.matches("example.com"));
        assert!(default_tls.matches("EXAMPLE.COM.:443"));
        assert!(!default_tls.matches("example.com:8443"));
        assert!(HostnameMatcher::from_pattern("example.com:*").matches("example.com:8443"));
        assert!(HostnameMatcher::from_pattern("[::1]:8443").matches("[::1]:8443"));
        assert!(!HostnameMatcher::from_pattern("[::1]:8443").matches("[::1]:443"));
        assert_eq!(
            HostnameMatcher::from_pattern("example.com:abc"),
            HostnameMatcher::Exact("example.com:abc".to_string())
        );
    }

    #[test]
    fn default_config_uses_sane_defaults() {
        let cfg = MitmConfig::default();
        assert_eq!(cfg.listen_addr.port(), 0);
        assert!(cfg.hostnames.is_empty());
        assert!(cfg.excluded_hostnames.is_empty());
        assert_eq!(cfg.max_body_size, 131_072);
        assert!(cfg.record_enabled);
        assert!(matches!(
            cfg.script_dialect,
            lanpath_script::ScriptDialect::Surge
        ));
    }
}
