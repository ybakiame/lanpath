//! Client runtime state types.

use std::net::SocketAddr;

/// Current client running status.
#[derive(Debug, Clone)]
pub struct ClientStatus {
    /// Whether the core is running.
    pub core_running: bool,
    /// MITM proxy listen address (`None` when not enabled).
    pub mitm_addr: Option<SocketAddr>,
    /// Effective enabled state; `None` means the platform read failed (not disabled).
    pub system_proxy: Option<bool>,
    pub system_proxy_ownership: Option<crate::sysproxy::SystemProxyOwnership>,
    pub system_proxy_error: Option<String>,
    /// Current effective rule mode (= persisted value in client.json, illegal values normalized to `rule`).
    pub rule_mode: String,
    /// Number of rules in the composed config (sing-box takes `route.rules` array length;
    /// 0 when not running).
    pub rule_count: u64,
    /// Clash dashboard API address (when core is running and `clash_api_enabled`,
    /// `http://127.0.0.1:{clash_api_port}`, otherwise `None`).
    pub clash_api_url: Option<String>,
    /// 本次启动未能本地化的内置 CN 规则集 tag（降级启动；空 = 完整分流）。
    pub missing_rule_sets: Vec<String>,
}
