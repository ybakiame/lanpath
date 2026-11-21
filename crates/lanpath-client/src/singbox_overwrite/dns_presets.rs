//! 推荐 DNS 预设目录（中国大陆直连 + 海外/代理上游），供配置切片 DNS 工作台
//! 一键添加或替换现有上游（含默认 `local` / `proxy`）。

use serde_json::{Map, Value, json};

/// 推荐 DNS 所属区域分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsPresetRegion {
    Domestic,
    International,
}

/// 单条推荐 DNS 上游规格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DnsPresetSpec {
    pub id: &'static str,
    pub tag: &'static str,
    pub name_zh: &'static str,
    pub name_en: &'static str,
    pub region: DnsPresetRegion,
    pub server_type: &'static str,
    pub server: &'static str,
    pub server_port: u16,
    pub detour: Option<&'static str>,
    pub description_zh: &'static str,
    pub description_en: &'static str,
}

impl DnsPresetSpec {
    /// 生成合法的 sing-box `dns.servers[]` 条目 JSON。
    #[must_use]
    pub fn to_server_json(self, custom_tag: Option<&str>) -> Value {
        let mut obj = Map::new();
        obj.insert(
            "tag".to_owned(),
            Value::String(custom_tag.unwrap_or(self.tag).to_owned()),
        );
        obj.insert(
            "type".to_owned(),
            Value::String(self.server_type.to_owned()),
        );
        obj.insert("server".to_owned(), Value::String(self.server.to_owned()));
        obj.insert("server_port".to_owned(), json!(self.server_port));
        if let Some(detour) = self.detour {
            obj.insert("detour".to_owned(), Value::String(detour.to_owned()));
        }
        Value::Object(obj)
    }
}

const PRESETS: &[DnsPresetSpec] = &[
    // 国内直连推荐
    DnsPresetSpec {
        id: "ali-doh",
        tag: "ali-doh",
        name_zh: "阿里 DNS (DoH)",
        name_en: "Alibaba DNS (DoH)",
        region: DnsPresetRegion::Domestic,
        server_type: "https",
        server: "223.5.5.5",
        server_port: 443,
        detour: None,
        description_zh: "阿里云公共加密 DNS，国内直连稳定低延迟。",
        description_en: "Alibaba encrypted DoH resolver for mainland China.",
    },
    DnsPresetSpec {
        id: "ali-udp",
        tag: "ali-udp",
        name_zh: "阿里 DNS (UDP)",
        name_en: "Alibaba DNS (UDP)",
        region: DnsPresetRegion::Domestic,
        server_type: "udp",
        server: "223.5.5.5",
        server_port: 53,
        detour: None,
        description_zh: "阿里云经典 UDP 53 解析，适合作为极速直连解析器。",
        description_en: "Alibaba standard UDP resolver on port 53.",
    },
    DnsPresetSpec {
        id: "dnspod-doh",
        tag: "dnspod-doh",
        name_zh: "腾讯 DNSPod (DoH)",
        name_en: "Tencent DNSPod (DoH)",
        region: DnsPresetRegion::Domestic,
        server_type: "https",
        server: "1.12.12.12",
        server_port: 443,
        detour: None,
        description_zh: "腾讯云加密 DoH，覆盖国内三大运营商线路。",
        description_en: "Tencent DNSPod encrypted DoH resolver.",
    },
    DnsPresetSpec {
        id: "dnspod-udp",
        tag: "dnspod-udp",
        name_zh: "腾讯 DNSPod (UDP)",
        name_en: "Tencent DNSPod (UDP)",
        region: DnsPresetRegion::Domestic,
        server_type: "udp",
        server: "119.29.29.29",
        server_port: 53,
        detour: None,
        description_zh: "腾讯公共 UDP DNS，国内 CDN 调度友好。",
        description_en: "Tencent DNSPod public UDP resolver.",
    },
    DnsPresetSpec {
        id: "dns-114",
        tag: "dns-114",
        name_zh: "114 DNS (UDP)",
        name_en: "114 DNS (UDP)",
        region: DnsPresetRegion::Domestic,
        server_type: "udp",
        server: "114.114.114.114",
        server_port: 53,
        detour: None,
        description_zh: "国内通用老牌公共 UDP 解析服务。",
        description_en: "Classic mainland China public UDP DNS.",
    },
    // 海外 / 代理推荐
    DnsPresetSpec {
        id: "cf-doh",
        tag: "cf-doh",
        name_zh: "Cloudflare (DoH)",
        name_en: "Cloudflare (DoH)",
        region: DnsPresetRegion::International,
        server_type: "https",
        server: "1.1.1.1",
        server_port: 443,
        detour: Some("proxy"),
        description_zh: "全球 Anycast 加密 DoH，默认走 proxy 分组防污染。",
        description_en: "Global Anycast DoH routed via proxy outbound.",
    },
    DnsPresetSpec {
        id: "cf-dot",
        tag: "cf-dot",
        name_zh: "Cloudflare (DoT)",
        name_en: "Cloudflare (DoT)",
        region: DnsPresetRegion::International,
        server_type: "tls",
        server: "1.1.1.1",
        server_port: 853,
        detour: Some("proxy"),
        description_zh: "Cloudflare DNS-over-TLS（853 端口），默认走 proxy。",
        description_en: "Cloudflare DNS-over-TLS (port 853) via proxy.",
    },
    DnsPresetSpec {
        id: "google-doh",
        tag: "google-doh",
        name_zh: "Google DNS (DoH)",
        name_en: "Google DNS (DoH)",
        region: DnsPresetRegion::International,
        server_type: "https",
        server: "8.8.8.8",
        server_port: 443,
        detour: Some("proxy"),
        description_zh: "Google 公共加密 DoH，海外域名 EDNS 准确度高。",
        description_en: "Google Public DoH resolver via proxy.",
    },
    DnsPresetSpec {
        id: "google-udp",
        tag: "google-udp",
        name_zh: "Google DNS (UDP)",
        name_en: "Google DNS (UDP)",
        region: DnsPresetRegion::International,
        server_type: "udp",
        server: "8.8.8.8",
        server_port: 53,
        detour: Some("proxy"),
        description_zh: "Google 标准 UDP 53 解析，默认经 proxy 转发。",
        description_en: "Google standard UDP DNS via proxy.",
    },
    DnsPresetSpec {
        id: "quad9-doh",
        tag: "quad9-doh",
        name_zh: "Quad9 安全 DNS (DoH)",
        name_en: "Quad9 Security (DoH)",
        region: DnsPresetRegion::International,
        server_type: "https",
        server: "9.9.9.9",
        server_port: 443,
        detour: Some("proxy"),
        description_zh: "注重隐私与恶意域名拦截的非营利加密 DoH。",
        description_en: "Privacy-focused encrypted DoH with threat blocking.",
    },
];

/// 返回全部推荐 DNS 预设列表。
#[must_use]
pub fn recommended_dns_presets() -> &'static [DnsPresetSpec] {
    PRESETS
}
