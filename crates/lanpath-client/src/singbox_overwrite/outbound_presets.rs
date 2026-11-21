//! `overwrite/sing-box/outbounds.json` 的节点正则筛选（`_filter`）、地区预设分组与出站标签提取。
//!
//! `_filter` 仅在配置切片层使用，合成最终 `runtime/sing-box/config.json` 时按条件展开
//! `"$nodes"` 并自动剥离 `_filter`，保持最终产物为纯原生 sing-box Schema。

use lanpath_common::{LanpathError, LanpathResult};
use regex::{Regex, RegexBuilder};
use serde_json::{Map, Value, json};

use super::NODES_PLACEHOLDER;

/// `outbounds.json` 分组对象上可选的节点筛选扩展键名。
pub const OUTBOUND_FILTER_KEY: &str = "_filter";

/// 分组节点筛选条件（包含 / 排除正则，默认大小写不敏感）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeFilterSpec {
    pub include: String,
    pub exclude: String,
}

/// 编译后的节点筛选器。
pub struct CompiledNodeFilter {
    include: Option<Regex>,
    exclude: Option<Regex>,
}

impl NodeFilterSpec {
    /// 从分组 JSON 对象的 `_filter` 字段解析筛选条件（缺省或为空字段均允许）。
    pub fn from_outbound_map(map: &Map<String, Value>) -> LanpathResult<Option<Self>> {
        let Some(raw) = map.get(OUTBOUND_FILTER_KEY) else {
            return Ok(None);
        };
        if raw.is_null() {
            return Ok(None);
        }
        let Some(obj) = raw.as_object() else {
            return Err(LanpathError::Validation(format!(
                "{OUTBOUND_FILTER_KEY} 必须是 JSON 对象（含 include / exclude 字符串字段）"
            )));
        };
        let include = match obj.get("include") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.trim().to_owned(),
            Some(_) => {
                return Err(LanpathError::Validation(format!(
                    "{OUTBOUND_FILTER_KEY}.include 必须是字符串"
                )));
            }
        };
        let exclude = match obj.get("exclude") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.trim().to_owned(),
            Some(_) => {
                return Err(LanpathError::Validation(format!(
                    "{OUTBOUND_FILTER_KEY}.exclude 必须是字符串"
                )));
            }
        };
        Ok(Some(Self { include, exclude }))
    }

    /// 是否未配置任何有效筛选规则。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.include.trim().is_empty() && self.exclude.trim().is_empty()
    }

    /// 编译包含 / 排除正则表达式（大小写不敏感），非法正则返回校验错误。
    pub fn compile(&self) -> LanpathResult<CompiledNodeFilter> {
        let include = compile_optional(&self.include, "include")?;
        let exclude = compile_optional(&self.exclude, "exclude")?;
        Ok(CompiledNodeFilter { include, exclude })
    }

    /// 序列化为可写入 `outbounds.json` 的 `_filter` JSON 对象（均为空时返回 `None`）。
    #[must_use]
    pub fn to_json_value(&self) -> Option<Value> {
        let inc = self.include.trim();
        let exc = self.exclude.trim();
        if inc.is_empty() && exc.is_empty() {
            return None;
        }
        let mut map = Map::new();
        if !inc.is_empty() {
            map.insert("include".to_owned(), Value::String(inc.to_owned()));
        }
        if !exc.is_empty() {
            map.insert("exclude".to_owned(), Value::String(exc.to_owned()));
        }
        Some(Value::Object(map))
    }
}

impl CompiledNodeFilter {
    /// 判断节点 tag 是否命中筛选条件（满足 include 且不命中 exclude）。
    #[must_use]
    pub fn matches(&self, tag: &str) -> bool {
        if let Some(re) = &self.include
            && !re.is_match(tag)
        {
            return false;
        }
        if let Some(re) = &self.exclude
            && re.is_match(tag)
        {
            return false;
        }
        true
    }
}

fn compile_optional(pattern: &str, field: &str) -> LanpathResult<Option<Regex>> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    RegexBuilder::new(trimmed)
        .case_insensitive(true)
        .size_limit(1 << 20)
        .build()
        .map(Some)
        .map_err(|err| {
            LanpathError::Validation(format!(
                "{OUTBOUND_FILTER_KEY}.{field} 正则无效（{trimmed}）：{err}"
            ))
        })
}

/// 一键生成的常用地区节点分组预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionGroupPreset {
    pub tag: &'static str,
    pub label_zh: &'static str,
    pub label_en: &'static str,
    pub include_pattern: &'static str,
    pub exclude_pattern: &'static str,
}

impl RegionGroupPreset {
    /// 构造带 `"$nodes"` 与 `_filter` 的 sing-box 分组对象（默认 `urltest`）。
    #[must_use]
    pub fn to_outbound_json(&self, group_type: &str) -> Value {
        let filter = NodeFilterSpec {
            include: self.include_pattern.to_owned(),
            exclude: self.exclude_pattern.to_owned(),
        };
        let mut obj = if group_type == "selector" {
            json!({
                "type": "selector",
                "tag": self.tag,
                "outbounds": [NODES_PLACEHOLDER]
            })
        } else {
            json!({
                "type": "urltest",
                "tag": self.tag,
                "outbounds": [NODES_PLACEHOLDER],
                "url": "https://www.gstatic.com/generate_204",
                "interval": "5m",
                "tolerance": 150
            })
        };
        if let Some(filter_val) = filter.to_json_value()
            && let Some(map) = obj.as_object_mut()
        {
            map.insert(OUTBOUND_FILTER_KEY.to_owned(), filter_val);
        }
        obj
    }
}

const REGION_PRESETS: &[RegionGroupPreset] = &[
    RegionGroupPreset {
        tag: "HK",
        label_zh: "香港 HK",
        label_en: "Hong Kong (HK)",
        include_pattern: r"香港|HK|Hong\s*Kong|🇭🇰",
        exclude_pattern: "",
    },
    RegionGroupPreset {
        tag: "TW",
        label_zh: "台湾 TW",
        label_en: "Taiwan (TW)",
        include_pattern: r"台湾|台灣|TW|Taiwan|🇹🇼",
        exclude_pattern: "",
    },
    RegionGroupPreset {
        tag: "JP",
        label_zh: "日本 JP",
        label_en: "Japan (JP)",
        include_pattern: r"日本|JP|Japan|东京|大阪|🇯🇵",
        exclude_pattern: "",
    },
    RegionGroupPreset {
        tag: "SG",
        label_zh: "新加坡 SG",
        label_en: "Singapore (SG)",
        include_pattern: r"新加坡|狮城|SG|Singapore|🇸🇬",
        exclude_pattern: "",
    },
    RegionGroupPreset {
        tag: "US",
        label_zh: "美国 US",
        label_en: "United States (US)",
        include_pattern: r"美国|美國|US|United\s*States|洛杉矶|硅谷|西雅图|🇺🇸",
        exclude_pattern: "",
    },
    RegionGroupPreset {
        tag: "EU",
        label_zh: "欧洲 EU",
        label_en: "Europe (EU)",
        include_pattern: r"英国|德国|法国|荷兰|欧洲|UK|DE|FR|NL|Europe|🇬🇧|🇩🇪|🇫🇷|🇳🇱",
        exclude_pattern: "",
    },
];

/// 返回内置地区节点分组预设列表。
#[must_use]
pub fn recommended_region_presets() -> &'static [RegionGroupPreset] {
    REGION_PRESETS
}

/// 从 `outbounds.json` 切片值中提取全部可用于路由目标的出站 / 策略组 tag（去重保序）。
#[must_use]
pub fn extract_outbound_tags(outbounds_slice: &Value) -> Vec<String> {
    let mut tags = Vec::new();
    let Some(arr) = outbounds_slice.get("outbounds").and_then(Value::as_array) else {
        return tags;
    };
    for item in arr {
        let Some(tag) = item.get("tag").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if tag.is_empty() || tags.iter().any(|existing: &String| existing == tag) {
            continue;
        }
        tags.push(tag.to_owned());
    }
    tags
}
