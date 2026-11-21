//! `overwrite/sing-box/*.json` 的默认基线切片。
//!
//! 保持与 Lanpath CN 分流默认基线一致（sing-box >= 1.14.0 语法），让用户一打开
//! 切片目录就能直接读懂并安全微调默认行为。

use serde_json::{Value, json};

use super::{NODES_PLACEHOLDER, SingboxSliceKind};
use crate::core_config::{
    OUTBOUND_KIND_BLOCK, OUTBOUND_KIND_DIRECT, OUTBOUND_KIND_SELECTOR, OUTBOUND_KIND_URLTEST,
    OUTBOUND_TAG_AUTO, OUTBOUND_TAG_BLOCK, OUTBOUND_TAG_DIRECT, OUTBOUND_TAG_FINAL,
    OUTBOUND_TAG_GLOBAL, OUTBOUND_TAG_PROXY, clash_mode_dns_rules,
};

/// 返回指定切片的默认 JSON `Value`（顶层键即切片名）。
#[must_use]
pub fn default_slice_value(kind: SingboxSliceKind) -> Value {
    match kind {
        SingboxSliceKind::Log => json!({
            "log": {
                "level": "info",
                "timestamp": true
            }
        }),
        SingboxSliceKind::Dns => json!({
            "dns": {
                "servers": [
                    {
                        "tag": "local",
                        "type": "https",
                        "server": "223.5.5.5",
                        "server_port": 443
                    },
                    {
                        "tag": "proxy",
                        "type": "udp",
                        "server": "8.8.8.8",
                        "server_port": 53,
                        "detour": OUTBOUND_TAG_PROXY
                    }
                ],
                "rules": clash_mode_dns_rules(),
                "final": "proxy",
                "reverse_mapping": true,
                "strategy": "prefer_ipv4"
            }
        }),
        SingboxSliceKind::Route => json!({
            "route": {
                "rules": [],
                "rule_set": [],
                "final": OUTBOUND_TAG_FINAL,
                "auto_detect_interface": true,
                "default_domain_resolver": {
                    "server": "local"
                }
            }
        }),
        SingboxSliceKind::Outbounds => json!({
            "outbounds": [
                {
                    "type": OUTBOUND_KIND_SELECTOR,
                    "tag": OUTBOUND_TAG_PROXY,
                    "outbounds": [OUTBOUND_TAG_AUTO, NODES_PLACEHOLDER],
                    "default": OUTBOUND_TAG_AUTO
                },
                {
                    "type": OUTBOUND_KIND_URLTEST,
                    "tag": OUTBOUND_TAG_AUTO,
                    "outbounds": [NODES_PLACEHOLDER],
                    "url": "https://www.gstatic.com/generate_204",
                    "interval": "5m",
                    "tolerance": 150
                },
                {
                    "type": OUTBOUND_KIND_DIRECT,
                    "tag": OUTBOUND_TAG_DIRECT
                },
                {
                    "type": OUTBOUND_KIND_BLOCK,
                    "tag": OUTBOUND_TAG_BLOCK
                },
                {
                    "type": OUTBOUND_KIND_SELECTOR,
                    "tag": OUTBOUND_TAG_FINAL,
                    "outbounds": [OUTBOUND_TAG_PROXY, OUTBOUND_TAG_DIRECT],
                    "default": OUTBOUND_TAG_PROXY
                },
                {
                    "type": OUTBOUND_KIND_SELECTOR,
                    "tag": OUTBOUND_TAG_GLOBAL,
                    "outbounds": [
                        OUTBOUND_TAG_PROXY,
                        OUTBOUND_TAG_AUTO,
                        OUTBOUND_TAG_FINAL,
                        OUTBOUND_TAG_DIRECT,
                        OUTBOUND_TAG_BLOCK
                    ],
                    "default": OUTBOUND_TAG_PROXY
                }
            ]
        }),
        SingboxSliceKind::Experimental => json!({
            "experimental": {
                "cache_file": {
                    "enabled": false,
                    "path": "",
                    "cache_id": "",
                    "store_fakeip": false,
                    "store_rdrc": false
                }
            }
        }),
    }
}

/// 返回带尾随换行的美化 JSON 字符串。
#[must_use]
pub fn default_slice_pretty(kind: SingboxSliceKind) -> String {
    let value = default_slice_value(kind);
    let body = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_owned());
    format!("{body}\n")
}
