//! 配置合成预览业务：共享 `lanpath-client` 合成链落在 NSH 桌面自己的隔离
//! 数据根内。sing-box 从 `<data_root>/overwrite/sing-box/*.json` 确定性组合并
//! 展开订阅节点（`$nodes`），再叠加规则工作台、TS 覆写与 `app.toml` 受管端口/API；
//! Mihomo 输出 YAML（核心直接消费的格式），不应用 sing-box 切片。
//!
//! 本切片只做共享合成预览：不下载/安装/校验核心二进制，不启动核心、系统
//! 代理或 TUN；预览内容未经过核心二进制校验，UI 不得显示“已验证/已启动”。

mod selection;
pub(crate) use selection::SelectedPreview;

use std::path::Path;
use std::sync::Arc;

use lanpath_client::config_slices::DnsMode;
use lanpath_client::singbox_overwrite::{
    SingboxOverwriteBundle, SingboxOverwriteStore, SingboxSliceKind,
    build_core_config_from_overwrite,
};
use lanpath_client::{
    EffectiveOverrides, PanelFeatures, ProfileStoreV2, RemoteFetchContext, RequestProxyMode,
    SubscriptionStore, apply_panel_features, compose_singbox_config,
};
use lanpath_common::{CoreType, LanpathError, LanpathResult};
use serde_json::Value;

use crate::data_root::{DataRoot, open_root, resolve_data_location};
use crate::request_settings::load_request_config;
use crate::subscriptions::{RefreshStatus, Subscriptions};

/// 预览输出格式：sing-box 为 pretty JSON，Mihomo 为 YAML（核心直接消费的格式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreviewFormat {
    Json,
    Yaml,
}

impl PreviewFormat {
    #[cfg(test)]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Yaml => "YAML",
        }
    }
}

/// 一次合成预览的结果：输出格式、核心类型、完整正文（JSON/YAML）、节点数、
/// 生成警告与缺失规则集 tag。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigPreview {
    format: PreviewFormat,
    core_type: CoreType,
    content: String,
    node_count: usize,
    warnings: Vec<String>,
    missing_tags: Vec<String>,
}

impl ConfigPreview {
    pub(crate) fn format(&self) -> PreviewFormat {
        self.format
    }

    #[cfg(test)]
    pub(crate) fn core_type(&self) -> CoreType {
        self.core_type
    }

    pub(crate) fn content(&self) -> &str {
        &self.content
    }

    pub(crate) fn node_count(&self) -> usize {
        self.node_count
    }

    pub(crate) fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub(crate) fn missing_tags(&self) -> &[String] {
        &self.missing_tags
    }
}

/// 配置合成预览业务句柄：与本根订阅业务共享同一句柄（同一把业务锁、同一 Weak 缓存）。
#[derive(Clone)]
pub(crate) struct ConfigComposer {
    root: Arc<DataRoot>,
    subscriptions: Arc<Subscriptions>,
}

impl ConfigComposer {
    pub(crate) fn open() -> LanpathResult<Self> {
        let location = resolve_data_location()?;
        Self::for_root(&location.path)
    }

    pub(crate) fn for_root(path: &Path) -> LanpathResult<Self> {
        Ok(Self {
            root: open_root(path)?,
            subscriptions: Subscriptions::open_service(path)?,
        })
    }

    fn store(&self) -> SubscriptionStore {
        SubscriptionStore::new(self.root.path().to_path_buf())
    }

    #[cfg(test)]
    pub(crate) async fn compose(&self) -> LanpathResult<ConfigPreview> {
        let _activity = self.root.activity().read().await;
        self.root.ensure_writable()?;
        let config = {
            let _guard = self.root.business_guard();
            load_request_config(self.root.path())?
        };
        self.compose_with_config(&config).await
    }

    /// 以调用方固定的 `ClientConfig` 快照合成（核心启动与预览共享路径）。
    pub(crate) async fn compose_with_config(
        &self,
        config: &lanpath_client::ClientConfig,
    ) -> LanpathResult<ConfigPreview> {
        let cache_only = config.request_proxy_mode == RequestProxyMode::Core;
        let mixed_port = config.mixed_port;

        let selected_id = config
            .active_subscription_id
            .ok_or_else(|| LanpathError::Client("请先在订阅页选择要使用的订阅".into()))?;
        let selected = {
            let _guard = self.root.business_guard();
            self.store()
                .load()?
                .into_iter()
                .find(|subscription| subscription.id == selected_id)
                .ok_or_else(|| LanpathError::Client("所选订阅不存在，请重新选择".into()))?
        };
        if !selected.enabled {
            return Err(LanpathError::Client(
                "所选订阅已停用，请先启用或重新选择".into(),
            ));
        }

        let nodes = match self.store().load_cached_content(selected.id) {
            Some(cached) => cached.singbox_nodes,
            None if cache_only => {
                return Err(LanpathError::Client(
                    "核心请求代理尚未就绪，且所选订阅没有可用节点缓存；请先显式选择其他请求策略获取订阅，或导入已有缓存"
                        .into(),
                ));
            }
            None => {
                let status = self
                    .subscriptions
                    .refresh_with_config_inner(selected.id, config)
                    .await?;
                match self.store().load_cached_content(selected.id) {
                    Some(cached) => cached.singbox_nodes,
                    None => {
                        let detail = match status {
                            RefreshStatus::Updated { .. } => {
                                "订阅没有可用的本地节点缓存，请先刷新".to_owned()
                            }
                            RefreshStatus::Failed { error } => error,
                            RefreshStatus::Stale => {
                                "刷新期间订阅已被修改或删除，结果未写入，请重试".to_owned()
                            }
                            RefreshStatus::Busy => "另一个订阅刷新正在进行，请稍后重试".to_owned(),
                        };
                        return Err(LanpathError::Client(detail));
                    }
                }
            }
        };
        let node_count = nodes.len();

        let overwrite = {
            let _guard = self.root.business_guard();
            SingboxOverwriteStore::new(self.root.path().to_path_buf()).load_bundle_resilient()
        };
        let mut warnings = overwrite.warnings.clone();
        let features = panel_features(self.root.path(), &overwrite, config);
        let requests = if cache_only {
            RemoteFetchContext::cache_only()
        } else {
            RemoteFetchContext::stopped(config.clone())?
        };
        let (effective, remote_warnings) = match selected.profile_id {
            Some(profile_id) => {
                let profile = {
                    let _guard = self.root.business_guard();
                    ProfileStoreV2::new(self.root.path().to_path_buf())
                        .load()?
                        .into_iter()
                        .find(|profile| profile.id == profile_id)
                        .ok_or_else(|| {
                            LanpathError::Client("订阅关联的覆写模板不存在，请重新关联".into())
                        })?
                };
                crate::ts_override::resolve(
                    &lanpath_client::resource_storage::cache_path(
                        self.root.path(),
                        "profile_cache",
                    ),
                    &profile,
                    &requests,
                )
                .await?
            }
            None => (EffectiveOverrides::default(), Vec::new()),
        };
        warnings.extend(remote_warnings);

        if config.core_type == CoreType::Mihomo {
            if overwrite.is_customized() {
                warnings.push(
                    "Mihomo 合成预览未应用 sing-box 配置切片（overwrite/sing-box 内容保留，供切换回 sing-box 使用）"
                        .to_owned(),
                );
            }
            let source = {
                let _guard = self.root.business_guard();
                self.store().load_source(selected.id)?
            };
            let nodes = lanpath_client::subscription::mihomo_nodes_from_subscription(
                source.as_ref(),
                &nodes,
            )?;
            let mut profile_config =
                lanpath_client::build_mihomo_config_v2(&nodes, &effective).await?;
            lanpath_client::compose_mihomo_config(&mut profile_config, mixed_port, None)?;
            let mut value = profile_config;
            let rules = crate::rules::Rules::for_root(self.root.path())?.load()?;
            warnings.extend(crate::rules::apply_mihomo_rules(&mut value, &rules));
            lanpath_client::core_config::apply_mihomo_panel_features(&mut value, &features);
            remove_mihomo_dashboard_download(&mut value);
            let content = serde_yaml::to_string(&value)?;
            let warning_count = warnings.len();
            tracing::info!(
                target: "lanpath::config_composer",
                "配置合成预览完成：Mihomo YAML，{node_count} 个节点，{warning_count} 条警告"
            );
            return Ok(ConfigPreview {
                format: PreviewFormat::Yaml,
                core_type: CoreType::Mihomo,
                content,
                node_count,
                warnings,
                missing_tags: Vec::new(),
            });
        }

        let profile_config =
            build_core_config_from_overwrite(&nodes, &effective, &overwrite).await?;
        let mut value = compose_singbox_config(&profile_config, mixed_port, None)?;
        let rules = crate::rules::Rules::for_root(self.root.path())?.load()?;
        crate::rules::apply_singbox_rules(&mut value, &rules);
        crate::rules::normalize_singbox_ports(&mut value);

        let ruleset_store =
            lanpath_client::rulesets::RuleSetStore::new(self.root.path().to_path_buf());
        let mut manifest = {
            let _guard = self.root.business_guard();
            ruleset_store.load_manifest()?
        };
        if !manifest.groups.is_empty() || !manifest.items.is_empty() {
            warnings.extend(lanpath_client::rulesets::expand_rule_set_groups(
                &mut value,
                &manifest.groups,
            ));
            if !manifest.items.is_empty() {
                let (changed, rs_warnings) = lanpath_client::rulesets::prepare_referenced_missing(
                    self.root.path(),
                    &value,
                    &mut manifest.items,
                    &requests,
                )
                .await;
                warnings.extend(rs_warnings);
                if changed {
                    let _guard = self.root.business_guard();
                    let _ = ruleset_store.save_manifest(&manifest);
                }
            }
        }
        let missing_tags = lanpath_client::rulesets::apply_managed_rule_sets(
            self.root.path(),
            &mut value,
            &manifest.items,
        );
        if !missing_tags.is_empty() {
            warnings.push(format!(
                "受管规则集缺少本地缓存文件 {} 个：{}",
                missing_tags.len(),
                missing_tags.join(", ")
            ));
        }

        apply_panel_features(&mut value, &features);
        remove_dashboard_download(&mut value);

        let content = serde_json::to_string_pretty(&value)?;
        let warning_count = warnings.len();
        tracing::info!(
            target: "lanpath::config_composer",
            "配置合成预览完成：sing-box JSON，{node_count} 个节点，{warning_count} 条警告，缺失规则集 {} 个",
            missing_tags.len()
        );
        Ok(ConfigPreview {
            format: PreviewFormat::Json,
            core_type: CoreType::SingBox,
            content,
            node_count,
            warnings,
            missing_tags,
        })
    }
}

fn panel_features(
    root: &Path,
    overwrite: &SingboxOverwriteBundle,
    config: &lanpath_client::ClientConfig,
) -> PanelFeatures {
    let dns_mode = if overwrite.is_slice_customized(SingboxSliceKind::Dns) {
        DnsMode::Takeover
    } else {
        DnsMode::FollowSystem
    };
    PanelFeatures {
        tun_enabled: config.tun_enabled,
        tun_stack: config.tun_stack.clone(),
        tun_auto_route: config.tun_auto_route,
        clash_api_enabled: config.clash_api_enabled,
        clash_api_port: config.clash_api_port,
        clash_api_secret: config.clash_api_secret.clone(),
        clash_api_ui: config.clash_api_ui.clone(),
        rule_mode: config.normalized_rule_mode().to_owned(),
        ipv6_enabled: config.ipv6_enabled,
        dns_fakeip_enabled: config.dns_fakeip_enabled,
        data_dir: root.join("runtime/db").to_string_lossy().into_owned(),
        dns_mode,
        github_proxy_prefix: config.github_proxy_prefix.clone(),
    }
}

fn remove_dashboard_download(value: &mut Value) {
    if let Some(api) = value
        .pointer_mut("/experimental/clash_api")
        .and_then(Value::as_object_mut)
    {
        api.remove("external_ui");
        api.remove("external_ui_download_url");
    }
}

fn remove_mihomo_dashboard_download(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("external-ui");
        object.remove("external-ui-url");
    }
}

#[cfg(test)]
#[path = "config_composer_tests.rs"]
mod tests;
