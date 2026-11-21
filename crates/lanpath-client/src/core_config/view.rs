//! 内置基线视图（read-only）：把 [`super::baseline`] 的基础 DNS 模式规则与内置出站
//! 派生成只读展示结构（不再预置任何规则集或默认分流规则，所有规则集均由用户开放添加）。

use serde::Serialize;
use serde_json::Value;

use super::baseline::{
    OUTBOUND_KIND_BLOCK, OUTBOUND_KIND_DIRECT, OUTBOUND_KIND_SELECTOR, OUTBOUND_KIND_URLTEST,
    OUTBOUND_TAG_AUTO, OUTBOUND_TAG_BLOCK, OUTBOUND_TAG_DIRECT, OUTBOUND_TAG_FINAL,
    OUTBOUND_TAG_GLOBAL, OUTBOUND_TAG_PROXY, clash_mode_dns_rules,
};

/// 内置 DNS 规则视图项（只读）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BaselineDnsRuleView {
    /// 中文描述（如「直连模式：DNS 走本地解析器」）。
    pub description: String,
    /// 分流到的 DNS server tag（`local` / `proxy`）。
    pub server: String,
    /// clash_mode 条件（`direct` / `global`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clash_mode: Option<String>,
}

/// 内置出站视图项（只读）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BaselineOutboundView {
    pub tag: String,
    pub kind: String,
    pub description: String,
}

/// 基础基线只读视图（Clash 模式 DNS 规则 + 内置出站）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BaselineView {
    pub dns_rules: Vec<BaselineDnsRuleView>,
    pub outbounds: Vec<BaselineOutboundView>,
    pub route_final: String,
}

impl BaselineView {
    #[must_use]
    pub fn new() -> Self {
        let dns_rules = clash_mode_dns_rules()
            .iter()
            .map(|rule| {
                let clash_mode = rule
                    .get("clash_mode")
                    .and_then(Value::as_str)
                    .map(String::from);
                let server = rule
                    .get("server")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let subject = clash_mode
                    .as_deref()
                    .map(|mode| format!("{}模式", clash_mode_label(mode)))
                    .unwrap_or_else(|| "默认".to_owned());
                BaselineDnsRuleView {
                    description: format!("{subject}：DNS 走{}解析器", dns_server_label(&server)),
                    server,
                    clash_mode,
                }
            })
            .collect();

        let outbounds = vec![
            BaselineOutboundView {
                tag: OUTBOUND_TAG_PROXY.to_string(),
                kind: OUTBOUND_KIND_SELECTOR.to_string(),
                description: "手动选择".to_string(),
            },
            BaselineOutboundView {
                tag: OUTBOUND_TAG_FINAL.to_string(),
                kind: OUTBOUND_KIND_SELECTOR.to_string(),
                description: "兜底（节点选择 + 直连）".to_string(),
            },
            BaselineOutboundView {
                tag: OUTBOUND_TAG_GLOBAL.to_string(),
                kind: OUTBOUND_KIND_SELECTOR.to_string(),
                description: "全局模式（含全部内置出站）".to_string(),
            },
            BaselineOutboundView {
                tag: OUTBOUND_TAG_AUTO.to_string(),
                kind: OUTBOUND_KIND_URLTEST.to_string(),
                description: "自动测速".to_string(),
            },
            BaselineOutboundView {
                tag: OUTBOUND_TAG_DIRECT.to_string(),
                kind: OUTBOUND_KIND_DIRECT.to_string(),
                description: "直连".to_string(),
            },
            BaselineOutboundView {
                tag: OUTBOUND_TAG_BLOCK.to_string(),
                kind: OUTBOUND_KIND_BLOCK.to_string(),
                description: "拦截".to_string(),
            },
        ];

        Self {
            dns_rules,
            outbounds,
            route_final: OUTBOUND_TAG_FINAL.to_string(),
        }
    }
}

impl Default for BaselineView {
    fn default() -> Self {
        Self::new()
    }
}

fn clash_mode_label(mode: &str) -> &'static str {
    match mode {
        "direct" => "直连",
        "global" => "全局",
        _ => "规则",
    }
}

fn dns_server_label(server: &str) -> &'static str {
    match server {
        "local" => "本地",
        "proxy" => "代理",
        "fakeip" => "FakeIP",
        _ => "默认",
    }
}
