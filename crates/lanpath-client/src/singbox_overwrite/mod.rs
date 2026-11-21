//! sing-box 分段配置切片目录（`<data_root>/overwrite/sing-box/*.json`）。
//!
//! 每个切片本身是合法的 sing-box 顶层 JSON 片段（`{"log": ...}`、`{"dns": ...}`、
//! `{"route": ...}`、`{"outbounds": [...]}`、`{"experimental": ...}`），初始化时写入
//! 默认基线，支持 GUI 表单与高级 JSON 源码双模式编辑，运行时由 Rust 确定性组合并
//! 注入订阅动态节点（`"$nodes"` 占位符）与 `app.toml` 受管入站/Clash API。

mod compose;
mod defaults;
mod dns_presets;
#[cfg(test)]
mod tests;

use std::io::Write;
use std::path::{Path, PathBuf};

use lanpath_common::{LanpathError, LanpathResult};
use serde_json::Value;

pub use compose::{build_core_config_from_overwrite, compose_from_overwrite, extract_dns_servers};
pub use defaults::{default_slice_pretty, default_slice_value};
pub use dns_presets::{DnsPresetRegion, DnsPresetSpec, recommended_dns_presets};

/// 数据根下的 sing-box 切片相对目录。
pub const OVERWRITE_SUBDIR: &str = "overwrite/sing-box";

/// `outbounds.json` 分组中用于展开全部订阅节点的动态占位符。
pub const NODES_PLACEHOLDER: &str = "$nodes";

/// sing-box 5 个可编辑配置切片。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SingboxSliceKind {
    Log,
    Dns,
    Route,
    Outbounds,
    Experimental,
}

impl SingboxSliceKind {
    pub const ALL: [Self; 5] = [
        Self::Log,
        Self::Dns,
        Self::Route,
        Self::Outbounds,
        Self::Experimental,
    ];

    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Log => "log.json",
            Self::Dns => "dns.json",
            Self::Route => "route.json",
            Self::Outbounds => "outbounds.json",
            Self::Experimental => "experimental.json",
        }
    }

    #[must_use]
    pub const fn section_key(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Dns => "dns",
            Self::Route => "route",
            Self::Outbounds => "outbounds",
            Self::Experimental => "experimental",
        }
    }

    #[must_use]
    pub const fn id(self) -> &'static str {
        self.section_key()
    }

    #[must_use]
    pub fn from_file_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.file_name() == name)
    }
}

/// 一次加载得到的完整 sing-box 切片集合（含损坏回退警告与定制状态）。
#[derive(Debug, Clone)]
pub struct SingboxOverwriteBundle {
    pub log: Value,
    pub dns: Value,
    pub route: Value,
    pub outbounds: Value,
    pub experimental: Value,
    pub warnings: Vec<String>,
    customized_flags: [bool; 5],
}

impl Default for SingboxOverwriteBundle {
    fn default() -> Self {
        Self {
            log: default_slice_value(SingboxSliceKind::Log),
            dns: default_slice_value(SingboxSliceKind::Dns),
            route: default_slice_value(SingboxSliceKind::Route),
            outbounds: default_slice_value(SingboxSliceKind::Outbounds),
            experimental: default_slice_value(SingboxSliceKind::Experimental),
            warnings: Vec::new(),
            customized_flags: [false; 5],
        }
    }
}

impl SingboxOverwriteBundle {
    #[must_use]
    pub fn get(&self, kind: SingboxSliceKind) -> &Value {
        match kind {
            SingboxSliceKind::Log => &self.log,
            SingboxSliceKind::Dns => &self.dns,
            SingboxSliceKind::Route => &self.route,
            SingboxSliceKind::Outbounds => &self.outbounds,
            SingboxSliceKind::Experimental => &self.experimental,
        }
    }

    #[must_use]
    pub fn is_customized(&self) -> bool {
        self.customized_flags.iter().any(|flag| *flag)
    }

    #[must_use]
    pub fn is_slice_customized(&self, kind: SingboxSliceKind) -> bool {
        self.customized_flags[slice_index(kind)]
    }

    fn set_slot(&mut self, kind: SingboxSliceKind, value: Value, customized: bool) {
        match kind {
            SingboxSliceKind::Log => self.log = value,
            SingboxSliceKind::Dns => self.dns = value,
            SingboxSliceKind::Route => self.route = value,
            SingboxSliceKind::Outbounds => self.outbounds = value,
            SingboxSliceKind::Experimental => self.experimental = value,
        }
        self.customized_flags[slice_index(kind)] = customized;
    }
}

const fn slice_index(kind: SingboxSliceKind) -> usize {
    match kind {
        SingboxSliceKind::Log => 0,
        SingboxSliceKind::Dns => 1,
        SingboxSliceKind::Route => 2,
        SingboxSliceKind::Outbounds => 3,
        SingboxSliceKind::Experimental => 4,
    }
}

/// 读写 `<data_root>/overwrite/sing-box/*.json` 的无状态存储句柄。
#[derive(Debug, Clone)]
pub struct SingboxOverwriteStore {
    data_dir: PathBuf,
}

impl SingboxOverwriteStore {
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    #[must_use]
    pub fn dir(&self) -> PathBuf {
        self.data_dir.join(OVERWRITE_SUBDIR)
    }

    #[must_use]
    pub fn slice_path(&self, kind: SingboxSliceKind) -> PathBuf {
        self.dir().join(kind.file_name())
    }

    /// 确保 `overwrite/sing-box/` 与 5 个默认切片文件存在；已有文件原样保留，绝不覆写。
    pub fn ensure_initialized(&self) -> LanpathResult<()> {
        let dir = self.dir();
        std::fs::create_dir_all(&dir)?;
        for kind in SingboxSliceKind::ALL {
            let path = self.slice_path(kind);
            if !path.exists() {
                let text = default_slice_pretty(kind);
                atomic_write_text(&path, &text)?;
            }
        }
        Ok(())
    }

    /// 读取单个切片的原始文本；文件不存在时返回默认美化 JSON。
    pub fn load_raw(&self, kind: SingboxSliceKind) -> LanpathResult<String> {
        let path = self.slice_path(kind);
        if !path.exists() {
            return Ok(default_slice_pretty(kind));
        }
        Ok(std::fs::read_to_string(&path)?)
    }

    /// 读取并校验单个切片为 JSON `Value`；缺失返回默认值，损坏返回真实错误。
    pub fn load_value(&self, kind: SingboxSliceKind) -> LanpathResult<Value> {
        let raw = self.load_raw(kind)?;
        parse_and_validate_slice(kind, &raw)
    }

    /// 读取全部 5 个切片：缺失使用默认，损坏/校验失败记录警告并回退该切片默认值
    /// （不阻断核心启动或预览，但向用户透明告警）。
    #[must_use]
    pub fn load_bundle_resilient(&self) -> SingboxOverwriteBundle {
        let mut bundle = SingboxOverwriteBundle::default();
        for kind in SingboxSliceKind::ALL {
            let path = self.slice_path(kind);
            if !path.exists() {
                continue;
            }
            let default_val = default_slice_value(kind);
            match std::fs::read_to_string(&path)
                .map_err(LanpathError::from)
                .and_then(|raw| parse_and_validate_slice(kind, &raw))
            {
                Ok(value) => {
                    let customized = value != default_val;
                    bundle.set_slot(kind, value, customized);
                }
                Err(error) => {
                    bundle.warnings.push(format!(
                        "配置切片读取失败（{}），已回退默认值：{error}",
                        kind.file_name()
                    ));
                    bundle.set_slot(kind, default_val, false);
                }
            }
        }
        bundle
    }

    /// 校验并原子保存单个切片（美化输出，保持文件可读性）。
    pub fn save_raw(&self, kind: SingboxSliceKind, raw: &str) -> LanpathResult<Value> {
        let value = parse_and_validate_slice(kind, raw)?;
        self.save_value(kind, &value)?;
        Ok(value)
    }

    /// 校验并原子保存单个切片 JSON `Value`。
    pub fn save_value(&self, kind: SingboxSliceKind, value: &Value) -> LanpathResult<()> {
        validate_slice(kind, value)?;
        let dir = self.dir();
        std::fs::create_dir_all(&dir)?;
        let pretty = format!("{}\n", serde_json::to_string_pretty(value)?);
        atomic_write_text(&self.slice_path(kind), &pretty)
    }

    /// 将单个切片重置为默认基线并落盘，返回美化后的文本。
    pub fn reset_slice(&self, kind: SingboxSliceKind) -> LanpathResult<String> {
        let dir = self.dir();
        std::fs::create_dir_all(&dir)?;
        let text = default_slice_pretty(kind);
        atomic_write_text(&self.slice_path(kind), &text)?;
        Ok(text)
    }

    /// 校验备份快照中的切片字节：允许备份可移植占位符 `$LANPATH_DATA/` 前缀，
    /// 其余任何环境变量 `$` 仍严格拒绝。
    pub fn validate_backup_slice_bytes(
        kind: SingboxSliceKind,
        bytes: &[u8],
    ) -> LanpathResult<Value> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| LanpathError::Validation(format!("切片不是有效 UTF-8：{error}")))?;
        let value: Value = serde_json::from_str(text)
            .map_err(|error| LanpathError::Validation(format!("JSON 语法错误：{error}")))?;
        validate_slice_impl(kind, &value, true)?;
        Ok(value)
    }
}

/// 解析文本并执行切片结构与安全边界校验。
pub fn parse_and_validate_slice(kind: SingboxSliceKind, raw: &str) -> LanpathResult<Value> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| LanpathError::Validation(format!("JSON 语法错误：{error}")))?;
    validate_slice(kind, &value)?;
    Ok(value)
}

/// 格式化 JSON 文本（仅检查 JSON 语法并美化输出）。
pub fn format_json(raw: &str) -> LanpathResult<String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| LanpathError::Validation(format!("JSON 语法错误：{error}")))?;
    Ok(format!("{}\n", serde_json::to_string_pretty(&value)?))
}

/// 校验单个切片的结构合法性与安全红线：
/// - 根必须是 JSON 对象，且仅包含本切片对应的顶层键（不允许夹带 `inbounds` 等受管段）；
/// - `outbounds` 必须是对象数组，每项有非空 `type` / `tag` 且 tag 不重复；
/// - `experimental` 不允许夹带受管的 `clash_api`（由 `app.toml` 统一管理并在运行时注入）。
pub fn validate_slice(kind: SingboxSliceKind, value: &Value) -> LanpathResult<()> {
    validate_slice_impl(kind, value, false)
}

fn has_disallowed_env_var(path: &str, allow_backup_placeholder: bool) -> bool {
    let rest = if allow_backup_placeholder {
        path.strip_prefix("$LANPATH_DATA/").unwrap_or(path)
    } else {
        path
    };
    rest.contains('$')
}

fn validate_slice_impl(
    kind: SingboxSliceKind,
    value: &Value,
    allow_backup_placeholder: bool,
) -> LanpathResult<()> {
    let obj = value
        .as_object()
        .ok_or_else(|| LanpathError::Validation("切片根节点必须是 JSON 对象".into()))?;
    let expected = kind.section_key();
    if obj.len() != 1 || !obj.contains_key(expected) {
        return Err(LanpathError::Validation(format!(
            "{} 必须且仅能包含顶层键 \"{expected}\"",
            kind.file_name()
        )));
    }
    let body = &obj[expected];
    match kind {
        SingboxSliceKind::Log => {
            let log = body
                .as_object()
                .ok_or_else(|| LanpathError::Validation("log 必须是 JSON 对象".into()))?;
            if let Some(level) = log.get("level").and_then(Value::as_str)
                && !matches!(
                    level,
                    "trace" | "debug" | "info" | "warn" | "error" | "fatal" | "panic"
                )
            {
                return Err(LanpathError::Validation(format!(
                    "不支持的 log.level 值：{level}"
                )));
            }
            if let Some(output) = log.get("output").and_then(Value::as_str)
                && has_disallowed_env_var(output, allow_backup_placeholder)
            {
                return Err(LanpathError::Validation(
                    "log.output 不允许包含环境变量展开符号 $".into(),
                ));
            }
        }
        SingboxSliceKind::Dns => {
            let dns = body
                .as_object()
                .ok_or_else(|| LanpathError::Validation("dns 必须是 JSON 对象".into()))?;
            if let Some(servers) = dns.get("servers") {
                let arr = servers
                    .as_array()
                    .ok_or_else(|| LanpathError::Validation("dns.servers 必须是数组".into()))?;
                for (index, item) in arr.iter().enumerate() {
                    if !item.is_object() {
                        return Err(LanpathError::Validation(format!(
                            "dns.servers[{index}] 必须是 JSON 对象"
                        )));
                    }
                }
            }
            if let Some(rules) = dns.get("rules")
                && !rules.is_array()
            {
                return Err(LanpathError::Validation("dns.rules 必须是数组".into()));
            }
        }
        SingboxSliceKind::Route => {
            let route = body
                .as_object()
                .ok_or_else(|| LanpathError::Validation("route 必须是 JSON 对象".into()))?;
            if let Some(rules) = route.get("rules")
                && !rules.is_array()
            {
                return Err(LanpathError::Validation("route.rules 必须是数组".into()));
            }
            if let Some(rule_set) = route.get("rule_set")
                && !rule_set.is_array()
            {
                return Err(LanpathError::Validation("route.rule_set 必须是数组".into()));
            }
        }
        SingboxSliceKind::Outbounds => {
            let arr = body
                .as_array()
                .ok_or_else(|| LanpathError::Validation("outbounds 必须是数组".into()))?;
            let mut tags = std::collections::HashSet::new();
            for (index, item) in arr.iter().enumerate() {
                let Some(map) = item.as_object() else {
                    return Err(LanpathError::Validation(format!(
                        "outbounds[{index}] 必须是 JSON 对象"
                    )));
                };
                let kind = map.get("type").and_then(Value::as_str).unwrap_or_default();
                let tag = map.get("tag").and_then(Value::as_str).unwrap_or_default();
                if kind.trim().is_empty() || tag.trim().is_empty() {
                    return Err(LanpathError::Validation(format!(
                        "outbounds[{index}] 必须包含非空的 type 与 tag"
                    )));
                }
                if !tags.insert(tag.to_owned()) {
                    return Err(LanpathError::Validation(format!(
                        "outbounds 中存在重复 tag：{tag}"
                    )));
                }
            }
        }
        SingboxSliceKind::Experimental => {
            let exp = body
                .as_object()
                .ok_or_else(|| LanpathError::Validation("experimental 必须是 JSON 对象".into()))?;
            if exp.contains_key("clash_api") {
                return Err(LanpathError::Validation(
                    "experimental.clash_api 由应用入口与控制 API 统一托管，请在上方的「入口与控制 API」卡片中修改"
                        .into(),
                ));
            }
            if let Some(path) = exp
                .get("cache_file")
                .and_then(|c| c.get("path"))
                .and_then(Value::as_str)
                && has_disallowed_env_var(path, allow_backup_placeholder)
            {
                return Err(LanpathError::Validation(
                    "experimental.cache_file.path 不允许包含环境变量展开符号 $".into(),
                ));
            }
        }
    }
    Ok(())
}

fn atomic_write_text(path: &Path, content: &str) -> LanpathResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| LanpathError::Config("切片文件路径无效".into()))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(content.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}
