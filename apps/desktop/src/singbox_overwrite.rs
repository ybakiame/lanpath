//! sing-box 配置切片与受管入口/控制 API 业务服务。
//!
//! - 5 个 sing-box 切片存储在 `<data_root>/overwrite/sing-box/{log,dns,route,outbounds,experimental}.json`；
//! - 跨核心受管端口与控制 API（`mixed_port`、`clash_api_enabled`、`clash_api_port`、`clash_api_secret`）
//!   继续存储在 `app.toml` `[client]`，供 sing-box / Mihomo / 系统代理守护 / 诊断统一消费；
//! - 所有读写在 `DataRoot::business_guard()` 内执行，写前经过 `ensure_writable()` 门控。

use std::path::Path;
use std::sync::Arc;

use lanpath_client::singbox_overwrite::{
    SingboxOverwriteStore, SingboxSliceKind, default_slice_value, extract_outbound_tags,
    format_json, parse_and_validate_slice,
};
use lanpath_common::{CoreType, LanpathError, LanpathResult};
use serde_json::Value;

use crate::data_root::{DataRoot, open_root, resolve_data_location};
use crate::request_settings::load_request_config;

/// `app.toml` `[client]` 中跨核心受管的本地入站与控制 API 绑定（仅限 127.0.0.1）。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ManagedBindings {
    pub(crate) mixed_port: u16,
    pub(crate) clash_api_enabled: bool,
    pub(crate) clash_api_port: u16,
    pub(crate) clash_api_secret: String,
    pub(crate) core_type: CoreType,
}

impl ManagedBindings {
    pub(crate) fn validate(&self) -> LanpathResult<()> {
        if self.mixed_port == 0 {
            return Err(LanpathError::Validation(
                "本地 Mixed 端口必须为 1–65535".into(),
            ));
        }
        if self.clash_api_enabled {
            if self.clash_api_port == 0 {
                return Err(LanpathError::Validation(
                    "Clash API 端口必须为 1–65535".into(),
                ));
            }
            if self.clash_api_port == self.mixed_port {
                return Err(LanpathError::Validation(
                    "Mixed 代理端口与 Clash API 端口不能相同".into(),
                ));
            }
        }
        Ok(())
    }
}

/// 单个切片的快照（原始文本 + 是否相对默认已自定义）。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SliceSnapshot {
    pub(crate) kind: SingboxSliceKind,
    pub(crate) raw: String,
    pub(crate) customized: bool,
}

/// 桌面端配置切片服务句柄。
#[derive(Clone)]
pub(crate) struct SingboxOverwriteService {
    root: Arc<DataRoot>,
}

impl SingboxOverwriteService {
    pub(crate) fn open() -> LanpathResult<Self> {
        let location = resolve_data_location()?;
        Self::for_root(&location.path)
    }

    pub(crate) fn for_root(path: &Path) -> LanpathResult<Self> {
        Ok(Self {
            root: open_root(path)?,
        })
    }

    fn store(&self) -> SingboxOverwriteStore {
        SingboxOverwriteStore::new(self.root.path().to_path_buf())
    }

    pub(crate) fn overwrite_dir_display(&self) -> String {
        self.store().dir().display().to_string()
    }

    /// 读取全部 5 个切片的文本与自定义标记（只读，不修改磁盘）。
    pub(crate) fn load_all_slices(&self) -> LanpathResult<Vec<SliceSnapshot>> {
        let _guard = self.root.business_guard();
        let store = self.store();
        let mut out = Vec::with_capacity(SingboxSliceKind::ALL.len());
        for kind in SingboxSliceKind::ALL {
            let raw = store.load_raw(kind)?;
            let customized = match serde_json::from_str::<Value>(&raw) {
                Ok(val) => val != default_slice_value(kind),
                Err(_) => true,
            };
            out.push(SliceSnapshot {
                kind,
                raw,
                customized,
            });
        }
        Ok(out)
    }

    /// 校验并原子保存指定切片，返回格式化后的最终文本与是否偏离默认。
    pub(crate) fn save_slice(
        &self,
        kind: SingboxSliceKind,
        raw: &str,
    ) -> LanpathResult<SliceSnapshot> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let value = self.store().save_raw(kind, raw)?;
        let formatted = self.store().load_raw(kind)?;
        let customized = value != default_slice_value(kind);
        tracing::info!(
            target: "lanpath::singbox_overwrite",
            "已保存配置切片：{}",
            kind.file_name()
        );
        Ok(SliceSnapshot {
            kind,
            raw: formatted,
            customized,
        })
    }

    /// 将单个切片恢复为默认基线并落盘。
    pub(crate) fn reset_slice(&self, kind: SingboxSliceKind) -> LanpathResult<SliceSnapshot> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let raw = self.store().reset_slice(kind)?;
        tracing::info!(
            target: "lanpath::singbox_overwrite",
            "已重置配置切片为默认：{}",
            kind.file_name()
        );
        Ok(SliceSnapshot {
            kind,
            raw,
            customized: false,
        })
    }

    /// 读取 `outbounds.json` 中定义的全部出站 / 策略组 tag（供「规则」页与 `route.json` 目标出站选择）。
    pub(crate) fn load_outbound_tags(&self) -> Vec<String> {
        let _guard = self.root.business_guard();
        let val = self
            .store()
            .load_value(SingboxSliceKind::Outbounds)
            .unwrap_or_else(|_| default_slice_value(SingboxSliceKind::Outbounds));
        extract_outbound_tags(&val)
    }

    /// 离线读取当前生效订阅已缓存的 sing-box 节点 tag 列表（用于分组 `_filter` 实时匹配计数预览）。
    pub(crate) fn load_active_subscription_node_tags(&self) -> Vec<String> {
        let _guard = self.root.business_guard();
        let Ok(config) = load_request_config(self.root.path()) else {
            return Vec::new();
        };
        let Some(active_id) = config.active_subscription_id else {
            return Vec::new();
        };
        let store = lanpath_client::SubscriptionStore::new(self.root.path().to_path_buf());
        let Some(cached) = store.load_cached_content(active_id) else {
            return Vec::new();
        };
        let wrapper = serde_json::json!({ "outbounds": cached.singbox_nodes });
        lanpath_client::extract_nodes_singbox(&wrapper)
            .into_iter()
            .filter_map(|node| node.get("tag").and_then(Value::as_str).map(str::to_owned))
            .collect()
    }

    /// 本地静态校验（JSON 语法 + 切片结构 + 安全红线）。
    pub(crate) fn validate_slice(kind: SingboxSliceKind, raw: &str) -> LanpathResult<Value> {
        parse_and_validate_slice(kind, raw)
    }

    /// 美化 JSON 文本。
    pub(crate) fn format_slice(raw: &str) -> LanpathResult<String> {
        format_json(raw)
    }

    /// 读取 `app.toml` `[client]` 中的受管入站端口与控制 API 绑定。
    pub(crate) fn load_bindings(&self) -> LanpathResult<ManagedBindings> {
        let _guard = self.root.business_guard();
        let config = load_request_config(self.root.path())?;
        Ok(ManagedBindings {
            mixed_port: config.mixed_port,
            clash_api_enabled: config.clash_api_enabled,
            clash_api_port: config.clash_api_port,
            clash_api_secret: config.clash_api_secret,
            core_type: config.core_type,
        })
    }

    /// 保存受管入站端口与控制 API 绑定到 `app.toml` `[client]`（保留所有无关字段）。
    pub(crate) fn save_bindings(
        &self,
        bindings: &ManagedBindings,
    ) -> LanpathResult<ManagedBindings> {
        bindings.validate()?;
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let mut config = load_request_config(self.root.path())?;
        config.mixed_port = bindings.mixed_port;
        config.clash_api_enabled = bindings.clash_api_enabled;
        config.clash_api_port = bindings.clash_api_port;
        config
            .clash_api_secret
            .clone_from(&bindings.clash_api_secret);
        config.save()?;
        tracing::info!(
            target: "lanpath::singbox_overwrite",
            "已保存受管端口与控制 API：mixed={} clash_api_enabled={} api_port={}",
            config.mixed_port,
            config.clash_api_enabled,
            config.clash_api_port
        );
        Ok(ManagedBindings {
            mixed_port: config.mixed_port,
            clash_api_enabled: config.clash_api_enabled,
            clash_api_port: config.clash_api_port,
            clash_api_secret: config.clash_api_secret,
            core_type: config.core_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_loads_saves_resets_slices_and_bindings() {
        let dir = tempfile::tempdir().unwrap();
        let service = SingboxOverwriteService::for_root(dir.path()).unwrap();

        let all = service.load_all_slices().unwrap();
        assert_eq!(all.len(), 5);
        assert!(all.iter().all(|s| !s.customized));

        let saved = service
            .save_slice(
                SingboxSliceKind::Log,
                r#"{ "log": { "level": "warn", "timestamp": false } }"#,
            )
            .unwrap();
        assert!(saved.customized);
        assert!(saved.raw.contains("\"warn\""));

        let reset = service.reset_slice(SingboxSliceKind::Log).unwrap();
        assert!(!reset.customized);
        assert!(reset.raw.contains("\"info\""));

        let mut bindings = service.load_bindings().unwrap();
        bindings.mixed_port = 17910;
        bindings.clash_api_enabled = true;
        bindings.clash_api_port = 19110;
        bindings.clash_api_secret = "secret-token".into();
        let saved_bindings = service.save_bindings(&bindings).unwrap();
        assert_eq!(saved_bindings.mixed_port, 17910);
        assert_eq!(saved_bindings.clash_api_port, 19110);
        assert_eq!(saved_bindings.clash_api_secret, "secret-token");

        // 端口冲突必须被拒绝。
        let mut conflict = saved_bindings.clone();
        conflict.clash_api_port = conflict.mixed_port;
        assert!(service.save_bindings(&conflict).is_err());
    }
}
