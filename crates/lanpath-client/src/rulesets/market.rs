//! 精选社区 sing-box 规则集市场目录（MetaCubeX `meta-rules-dat@sing`）。

use super::RuleSetFormat;

/// 市场规则集镜像源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketMirror {
    JsDelivr,
    GitHubRaw,
}

impl MarketMirror {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::JsDelivr => "jsdelivr",
            Self::GitHubRaw => "github_raw",
        }
    }
}

/// 单条精选市场规则集规格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarketRuleSetSpec {
    pub tag: &'static str,
    pub name_zh: &'static str,
    pub name_en: &'static str,
    pub category: &'static str,
    pub description_zh: &'static str,
    pub description_en: &'static str,
    pub repo_path: &'static str,
    pub format: RuleSetFormat,
}

impl MarketRuleSetSpec {
    /// 生成指定镜像源的下载 URL。
    #[must_use]
    pub fn url_for_mirror(self, mirror: MarketMirror) -> String {
        match mirror {
            MarketMirror::JsDelivr => format!(
                "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/{}",
                self.repo_path
            ),
            MarketMirror::GitHubRaw => format!(
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/{}",
                self.repo_path
            ),
        }
    }
}

const CATALOG: &[MarketRuleSetSpec] = &[
    MarketRuleSetSpec {
        tag: "geosite-private",
        name_zh: "局域网与私有域名",
        name_en: "Private & LAN Domains",
        category: "cn",
        description_zh: "本机、局域网与私有内网域名（.local / .lan 等），按需添加直连。",
        description_en: "Localhost, LAN, and private intranet domains for opt-in direct routing.",
        repo_path: "geo/geosite/private.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geoip-private",
        name_zh: "局域网与私有 IP",
        name_en: "Private & LAN IP Ranges",
        category: "cn",
        description_zh: "RFC 1918 / 回环 / 链路本地私有网段，按需配合私有域名直连。",
        description_en: "RFC 1918, loopback, and link-local IP ranges for opt-in direct routing.",
        repo_path: "geo/geoip/private.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-cn",
        name_zh: "中国大陆域名",
        name_en: "China Domains",
        category: "cn",
        description_zh: "常见中国大陆服务与网站域名列表，适合直连分流。",
        description_en: "Mainland China services and websites for direct routing.",
        repo_path: "geo/geosite/cn.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geoip-cn",
        name_zh: "中国大陆 IP",
        name_en: "China IP Ranges",
        category: "cn",
        description_zh: "中国大陆公网 IPv4/IPv6 CIDR 集合，配合 geosite-cn 实现完整直连。",
        description_en: "Mainland China IPv4/IPv6 CIDR ranges for direct routing.",
        repo_path: "geo/geoip/cn.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-geolocation-!cn",
        name_zh: "海外常用域名",
        name_en: "Non-CN Domains",
        category: "proxy",
        description_zh: "非中国大陆主流国际网站与云服务域名。",
        description_en: "Common international websites and cloud services outside China.",
        repo_path: "geo/geosite/geolocation-!cn.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-category-ads-all",
        name_zh: "广告与追踪拦截",
        name_en: "Ads & Trackers",
        category: "ads",
        description_zh: "主流广告联盟与统计追踪域名，适合配合拒绝动作使用。",
        description_en: "Common ad networks and telemetry trackers for reject rules.",
        repo_path: "geo/geosite/category-ads-all.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-openai",
        name_zh: "OpenAI / ChatGPT",
        name_en: "OpenAI / ChatGPT",
        category: "ai",
        description_zh: "ChatGPT 与 OpenAI API 相关域名。",
        description_en: "Domains for ChatGPT and OpenAI APIs.",
        repo_path: "geo/geosite/openai.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-anthropic",
        name_zh: "Anthropic / Claude",
        name_en: "Anthropic / Claude",
        category: "ai",
        description_zh: "Claude 网页端与 Anthropic API 域名。",
        description_en: "Domains for Claude web and Anthropic APIs.",
        repo_path: "geo/geosite/anthropic.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-google",
        name_zh: "Google 服务",
        name_en: "Google Services",
        category: "proxy",
        description_zh: "Google 搜索、Gmail、Gemini、Drive 等核心域名。",
        description_en: "Google Search, Gmail, Gemini, Drive, and core domains.",
        repo_path: "geo/geosite/google.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-github",
        name_zh: "GitHub",
        name_en: "GitHub",
        category: "proxy",
        description_zh: "GitHub 网页、Raw 资源、Releases 与 Copilot 域名。",
        description_en: "GitHub web, raw content, releases, and Copilot domains.",
        repo_path: "geo/geosite/github.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-youtube",
        name_zh: "YouTube",
        name_en: "YouTube",
        category: "media",
        description_zh: "YouTube 视频播放与静态资源域名。",
        description_en: "YouTube streaming and media CDN domains.",
        repo_path: "geo/geosite/youtube.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-netflix",
        name_zh: "Netflix",
        name_en: "Netflix",
        category: "media",
        description_zh: "Netflix 流媒体服务与视频分发域名。",
        description_en: "Netflix streaming and CDN domains.",
        repo_path: "geo/geosite/netflix.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-telegram",
        name_zh: "Telegram 域名",
        name_en: "Telegram Domains",
        category: "proxy",
        description_zh: "Telegram 网页与客户端服务域名。",
        description_en: "Telegram web and service domains.",
        repo_path: "geo/geosite/telegram.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geoip-telegram",
        name_zh: "Telegram IP",
        name_en: "Telegram IP Ranges",
        category: "proxy",
        description_zh: "Telegram 数据中心 IP 段，建议与 geosite-telegram 搭配使用。",
        description_en: "Telegram datacenter IP ranges.",
        repo_path: "geo/geoip/telegram.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-apple",
        name_zh: "Apple 服务",
        name_en: "Apple Services",
        category: "cn",
        description_zh: "Apple 生态、iCloud 与 App Store 相关域名。",
        description_en: "Apple ecosystem, iCloud, and App Store domains.",
        repo_path: "geo/geosite/apple.srs",
        format: RuleSetFormat::Binary,
    },
    MarketRuleSetSpec {
        tag: "geosite-microsoft",
        name_zh: "Microsoft 服务",
        name_en: "Microsoft Services",
        category: "cn",
        description_zh: "Microsoft 365、Windows Update 与 OneDrive 相关域名。",
        description_en: "Microsoft 365, Windows Update, and OneDrive domains.",
        repo_path: "geo/geosite/microsoft.srs",
        format: RuleSetFormat::Binary,
    },
];

/// 返回全部精选社区规则集目录。
#[must_use]
pub fn market_catalog() -> &'static [MarketRuleSetSpec] {
    CATALOG
}
