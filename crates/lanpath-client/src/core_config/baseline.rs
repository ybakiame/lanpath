//! sing-box 基础默认基线：内置出站标签、Clash 模式 DNS 规则与远程规则集直连 HTTP 客户端。

use serde_json::{Value, json};

use crate::config_slices::{
    DnsMatchType, DnsRule, DnsRuleAction, DnsServer, DnsServerType, DnsSlice, DnsStrategy,
};

/// 默认 DNS 切片视图（与运行态默认 DNS 结构一致：`local` + `proxy` 上游、
/// 屏蔽 HTTPS/SVCB(/AAAA) 与 Clash `direct`/`global` 模式分流规则）。
#[must_use]
pub fn builtin_dns_slice(ipv6_enabled: bool) -> DnsSlice {
    let mut drop_target = "HTTPS,SVCB".to_string();
    if !ipv6_enabled {
        drop_target.push_str(",AAAA");
    }
    let rule = |id: &str, match_type: DnsMatchType, target: &str, server_tag: &str| DnsRule {
        id: id.to_string(),
        enabled: true,
        match_type,
        target: target.to_string(),
        server_tag: server_tag.to_string(),
        action: DnsRuleAction::Route,
        rcode: String::new(),
    };
    DnsSlice {
        mode: Default::default(),
        servers: vec![
            DnsServer {
                tag: "local".to_string(),
                server: "223.5.5.5".to_string(),
                server_type: DnsServerType::Https,
                server_port: Some(443),
                ..Default::default()
            },
            DnsServer {
                // 境外解析器：Google 公共 DNS 的 UDP（经代理 UDP 转发）；Google 的 DoH 端点
                // 默认不可通（tls/https 型 8.8.8.8 在国内直连与部分节点落地均被拦），
                // UDP:53 走代理 UDP 中继反而最稳。tag = proxy（2026-09 由 remote 更名，
                // 表达"经代理的解析器"语义）。
                tag: "proxy".to_string(),
                server: "8.8.8.8".to_string(),
                server_type: DnsServerType::Udp,
                server_port: Some(53),
                detour: OUTBOUND_TAG_PROXY.to_string(),
                ..Default::default()
            },
        ],
        // 仅保留模式切换与查询类型过滤，不预置任何规则集引用；规则集分流由用户自行添加。
        rules: vec![
            DnsRule {
                id: "builtin-drop".to_string(),
                enabled: true,
                match_type: DnsMatchType::QueryType,
                target: drop_target,
                server_tag: String::new(),
                action: DnsRuleAction::Predefined,
                rcode: "NOERROR".to_string(),
            },
            rule(
                "builtin-mode-direct",
                DnsMatchType::ClashMode,
                "direct",
                "local",
            ),
            rule(
                "builtin-mode-global",
                DnsMatchType::ClashMode,
                "global",
                "proxy",
            ),
        ],
        final_tag: "proxy".to_string(),
        strategy: if ipv6_enabled {
            DnsStrategy::PreferIpv4
        } else {
            DnsStrategy::Ipv4Only
        },
        reverse_mapping: true,
    }
}
/// 远程规则集下载用直连 HTTP 客户端 tag（由 [`ensure_rule_set_http_client`] 注入）。
pub const RULE_SET_HTTP_CLIENT_TAG: &str = "rule-set-direct";

/// Built-in outbound tags (single source of truth shared by [`crate::profile::singbox_template`]
/// and the read-only baseline view [`crate::core_config::BaselineView`]).
pub const OUTBOUND_TAG_PROXY: &str = "proxy";
/// See [`OUTBOUND_TAG_PROXY`].
pub const OUTBOUND_TAG_AUTO: &str = "auto";
/// See [`OUTBOUND_TAG_PROXY`].
pub const OUTBOUND_TAG_DIRECT: &str = "direct";
/// See [`OUTBOUND_TAG_PROXY`].
pub const OUTBOUND_TAG_BLOCK: &str = "block";

/// Built-in outbound `type` values (single source of truth shared by
/// [`crate::profile::singbox_template`] and [`crate::core_config::BaselineView`]).
pub const OUTBOUND_KIND_SELECTOR: &str = "selector";
/// See [`OUTBOUND_KIND_SELECTOR`].
pub const OUTBOUND_KIND_URLTEST: &str = "urltest";
/// See [`OUTBOUND_KIND_SELECTOR`].
pub const OUTBOUND_KIND_DIRECT: &str = "direct";
/// See [`OUTBOUND_KIND_SELECTOR`].
pub const OUTBOUND_KIND_BLOCK: &str = "block";

/// Clash 模式切换基础 DNS 规则：`clash_mode: direct → local`、`clash_mode: global → proxy`。
///
/// 仅负责让 DNS 解析随 Clash 模式（直连 / 全局）切换，不预置任何 `rule_set` 引用；
/// 所有规则集分流均由用户在「规则集」页自行添加并引用。
pub fn clash_mode_dns_rules() -> Vec<Value> {
    vec![
        json!({ "clash_mode": "direct", "action": "route", "server": "local" }),
        json!({ "clash_mode": "global", "action": "route", "server": "proxy" }),
    ]
}

/// 内置分组规格（物化进出站切片的真值源，2026-09）。
///
/// 内置分组**可修改不可删除**：物化后用户可调 selector 的 `default` / urltest 的
/// `url` / `interval` / `tolerance` / 两者的 `interrupt_exist_connections`，成员列表
/// 由模板动态计算（订阅节点变化跟随），不可在切片中编辑。id / name 稳定，勿改。
pub struct BuiltinOutboundGroupSpec {
    /// 稳定条目 ID（物化进切片的 `CustomOutbound.id`）。
    pub id: &'static str,
    /// 名称 = 模板分组 tag（apply 时按此匹配模板出站做字段覆写）。
    pub name: &'static str,
    /// 是否 selector（false = urltest）。
    pub selector: bool,
    /// 成员是否模板动态计算（proxy/auto = 跟随订阅，不可编辑；global/final = 静态内置
    /// 成员集合，可在切片中编辑）。
    pub dynamic_members: bool,
    /// 静态成员（`dynamic_members = false` 时的模板与播种默认值）。
    pub members: &'static [&'static str],
    /// 默认成员 tag。
    pub default: &'static str,
}

/// See [`OUTBOUND_TAG_PROXY`].
pub const OUTBOUND_TAG_GLOBAL: &str = "global";
/// See [`OUTBOUND_TAG_PROXY`].
pub const OUTBOUND_TAG_FINAL: &str = "final";

/// 内置分组（顺序 = 列表展示顺序）。
///
/// `proxy` / `auto` 成员跟随订阅动态计算；`global`（Clash GLOBAL 语义：全局模式出站，
/// 含其它内置出站）/ `final`（兜底：节点选择 + 直连，`route.final` 指向它）成员为静态
/// 内置集合，可在切片中编辑。
pub const BUILTIN_OUTBOUND_GROUPS: [BuiltinOutboundGroupSpec; 4] = [
    BuiltinOutboundGroupSpec {
        id: "builtin-group-proxy",
        name: OUTBOUND_TAG_PROXY,
        selector: true,
        dynamic_members: true,
        members: &[],
        default: OUTBOUND_TAG_AUTO,
    },
    BuiltinOutboundGroupSpec {
        id: "builtin-group-auto",
        name: OUTBOUND_TAG_AUTO,
        selector: false,
        dynamic_members: true,
        members: &[],
        default: "",
    },
    BuiltinOutboundGroupSpec {
        id: "builtin-group-global",
        name: OUTBOUND_TAG_GLOBAL,
        selector: true,
        dynamic_members: false,
        members: &[
            OUTBOUND_TAG_PROXY,
            OUTBOUND_TAG_AUTO,
            OUTBOUND_TAG_FINAL,
            OUTBOUND_TAG_DIRECT,
            OUTBOUND_TAG_BLOCK,
        ],
        default: OUTBOUND_TAG_PROXY,
    },
    BuiltinOutboundGroupSpec {
        id: "builtin-group-final",
        name: OUTBOUND_TAG_FINAL,
        selector: true,
        dynamic_members: false,
        members: &[OUTBOUND_TAG_PROXY, OUTBOUND_TAG_DIRECT],
        default: OUTBOUND_TAG_PROXY,
    },
];

/// 确保顶层 `http_clients` 携带直连规则集下载客户端 [`RULE_SET_HTTP_CLIENT_TAG`]。
///
/// 若配置中没有 `local` DNS 上游则跳过，避免写入悬空的 resolver 引用；已有同名客户端则保留。
pub fn ensure_rule_set_http_client(obj: &mut serde_json::Map<String, Value>) {
    let has_local_dns = obj
        .get("dns")
        .and_then(|d| d.get("servers"))
        .and_then(Value::as_array)
        .is_some_and(|servers| {
            servers
                .iter()
                .any(|s| s.get("tag").and_then(Value::as_str) == Some("local"))
        });
    if !has_local_dns {
        tracing::debug!("无 local DNS server，跳过规则集直连 http_client 注入");
        return;
    }
    let clients = obj
        .entry("http_clients")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(arr) = clients.as_array_mut() else {
        return;
    };
    if arr
        .iter()
        .any(|c| c.get("tag").and_then(Value::as_str) == Some(RULE_SET_HTTP_CLIENT_TAG))
    {
        return;
    }
    arr.push(json!({
        "tag": RULE_SET_HTTP_CLIENT_TAG,
        "domain_resolver": { "server": "local" }
    }));
}
