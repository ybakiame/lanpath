//! 配置合成预览业务：共享 `lanpath-client` 合成链落在 NSH 桌面自己的隔离
//! 数据根内。从 apps/gpui 的 `business/config.rs`（`build_config_report` 的
//! sing-box 与 Mihomo 两条分支、`panel_features`、移除面板下载字段）迁移语义：
//! 用户点击后按已保存配置快照合成完整配置，供“配置”页只读展示——sing-box
//! 输出 pretty JSON，Mihomo 输出 YAML（核心直接消费的格式）。
//!
//! 本切片只做共享合成预览：不下载/安装/校验核心二进制，不启动核心、系统
//! 代理或 TUN；预览内容未经过核心二进制校验，UI 不得显示“已验证/已启动”。
//! 核心以 sing-box 为主；Mihomo 现阶段支持订阅配置合成运行（YAML 预览），
//! 不做 Mihomo 结构化切片编辑——非默认 sing-box 配置切片只警告、不应用。
//!
//! 合成全程持有进入网络前固定的 `ClientConfig` 快照（业务锁内读出），网络
//! 途中设置页的修改不改变本次请求策略；业务锁不跨 await，写回仍走
//! `Subscriptions::refresh_with_config` 的 id/URL/UA 防 stale 实现。

mod selection;
pub(crate) use selection::SelectedPreview;

use std::path::Path;
use std::sync::Arc;

use lanpath_client::config_slices::{ConfigSlices, ConfigSlicesStore};
use lanpath_client::{
    EffectiveOverrides, PanelFeatures, ProfileStoreV2, RemoteFetchContext, RequestProxyMode,
    SubscriptionStore, apply_panel_features, build_core_config_v2, compose_singbox_config,
    inject_local_override_warn_only, prepare_missing_rule_sets_warn_only,
};
use lanpath_common::{CoreType, LanpathError, LanpathResult};
use serde_json::Value;

use crate::data_root::{DataRoot, open_root, resolve_data_location};
use crate::request_settings::load_request_config;
use crate::subscriptions::{RefreshStatus, Subscriptions};

/// 预览输出格式：sing-box 为 pretty JSON，Mihomo 为 YAML（核心直接消费的
/// 格式）。
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
/// 生成警告与缺失规则集 tag。警告包含配置切片读取回退、Mihomo 切片未应用
/// 与远程覆写降级等共享层真实警告，不静默。
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

/// 配置合成预览业务句柄：与本根订阅业务共享同一句柄（同一把业务锁、同一
/// Weak 缓存），克隆廉价；所有清单/配置读写在临界区内完成，锁不跨 await。
#[derive(Clone)]
pub(crate) struct ConfigComposer {
    root: Arc<DataRoot>,
    subscriptions: Arc<Subscriptions>,
}

impl ConfigComposer {
    /// 解析本进程数据根并打开合成预览业务（应用启动路径）。
    pub(crate) fn open() -> LanpathResult<Self> {
        let location = resolve_data_location()?;
        Self::for_root(&location.path)
    }

    /// 指定数据根打开（测试与显式调用路径，同样走完整准入）。
    pub(crate) fn for_root(path: &Path) -> LanpathResult<Self> {
        Ok(Self {
            root: open_root(path)?,
            subscriptions: Subscriptions::open_service(path)?,
        })
    }

    fn store(&self) -> SubscriptionStore {
        SubscriptionStore::new(self.root.path().to_path_buf())
    }

    /// 按已保存配置快照合成配置预览（GPUI `build_config_report` 的 sing-box
    /// 与 Mihomo 两条分支语义）：sing-box 输出 pretty JSON，Mihomo 输出
    /// YAML。sing-box 侧 `download_rule_sets = true`：与 GPUI 启动前预览
    /// 同路径补齐规则集（cache_only 只读缓存，非 Core 用已停止运行时的
    /// 请求策略），缺失按共享降级并返回 missing tags。
    ///
    /// 全程持有活动读保护：备份恢复准备（写保护）会等本次合成真正结束；
    /// 恢复已准备（待重启）时直接拒绝。日志只记节点数、警告数与结果，
    /// 不记配置正文、节点凭据、远程地址或代理地址。
    #[cfg(test)]
    pub(crate) async fn compose(&self) -> LanpathResult<ConfigPreview> {
        let _activity = self.root.activity().read().await;
        self.root.ensure_writable()?;
        // 快照在进入网络前于业务锁内读出：本次合成的请求策略/端口/active
        // 全部固定，网络途中设置页的修改不影响本次预览。
        let config = {
            let _guard = self.root.business_guard();
            load_request_config(self.root.path())?
        };
        self.compose_with_config(&config).await
    }

    /// 以调用方固定的 `ClientConfig` 快照合成（核心启动路径）：订阅刷新、
    /// 覆写解析、规则资源补齐与端口/API 注入全部使用同一快照——网络途中
    /// 设置页的修改不改变本次合成，同一次合成不会重复联网。`compose`
    /// 只是先读已保存快照再调用本方法的便捷入口。
    ///
    /// 调用方必须已持有本根活动读保护并已过 pending 门（[`ConfigComposer::compose`]
    /// 与核心启动路径负责）；内部订阅刷新用无保护的 inner 变体，避免在
    /// 已持读保护时重复获取读锁（写者排队时会死锁）。
    pub(crate) async fn compose_with_config(
        &self,
        config: &lanpath_client::ClientConfig,
    ) -> LanpathResult<ConfigPreview> {
        let cache_only = config.request_proxy_mode == RequestProxyMode::Core;
        let mixed_port = config.mixed_port;

        let selected_id = config
            .active_subscription_id
            .ok_or_else(|| LanpathError::Client("请先在订阅页选择要使用的订阅".into()))?;
        // 生效订阅必须存在且启用（与 GPUI 相同的三条前置校验）。
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

        // 节点优先取本地缓存（不联网）；无缓存时 Core 策略明确拒绝，
        // 其余模式用本次快照刷新一次后重读缓存。
        let nodes = match self.store().load_cached_content(selected.id) {
            Some(cached) => cached.singbox_nodes,
            None if cache_only => {
                return Err(LanpathError::Client(
                    "核心请求代理尚未就绪，且所选订阅没有可用节点缓存；请先显式选择其他请求策略获取订阅，或导入已有缓存"
                        .into(),
                ));
            }
            None => {
                // 外层长任务（compose/核心启动）已持本根活动读保护：刷新
                // 用 inner 变体，重复获取读锁在写者排队时会死锁。
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

        let mut warnings = Vec::new();
        // 配置切片读取失败按 GPUI 回退共享默认值，但把真实警告展示给用户
        // （共享存储层自身只记 tracing 警告，不区分回退路径）。
        let slices = load_slices(self.root.path(), &mut warnings);
        let features = panel_features(self.root.path(), &slices, config);
        let requests = if cache_only {
            RemoteFetchContext::cache_only()
        } else {
            RemoteFetchContext::stopped(config.clone())?
        };
        // 关联 Profile 必须真实存在；远程 TS 经共享缓存目录解析并转译，
        // 远程失败按共享降级为缓存/空并返回警告，不静默。覆写链与核心类型
        // 无关：sing-box 与 Mihomo 共用同一远程+本地覆写实现。
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

        // Mihomo 分支（GPUI `build_config_report` 同路径）：现阶段支持订阅
        // 配置合成运行，输出 YAML。不做 Mihomo 结构化切片编辑——用户定制的
        // sing-box 配置切片只警告、不应用（切片内容保留，供切回 sing-box）。
        if config.core_type == CoreType::Mihomo {
            if slices_customized(&slices) {
                warnings.push(
                    "Mihomo 合成预览未应用 sing-box 配置切片（切片内容保留，供切换回 sing-box 使用）"
                        .to_owned(),
                );
            }
            // 优先原始 Clash 订阅原文提取代理（无损耗往返）；其余格式按共享
            // 转换逐节点处理，Mihomo 无法表示的协议真实报错。
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
            tracing::info!(target: "lanpath_desktop_nsh::config_composer", "配置合成预览完成：Mihomo YAML，{node_count} 个节点，{warning_count} 条警告");
            return Ok(ConfigPreview {
                format: PreviewFormat::Yaml,
                core_type: CoreType::Mihomo,
                content,
                node_count,
                warnings,
                missing_tags: Vec::new(),
            });
        }

        let subscription = serde_json::json!({ "outbounds": nodes });
        let profile_config = build_core_config_v2(&subscription, &effective, &slices).await?;
        let mut value = compose_singbox_config(&profile_config, mixed_port, None)?;
        // 与 GPUI 启动前预览同路径：先补齐缺失规则集（进度消息只含 tag，
        // 不含 URL），再物化内置规则集；全部缺失走共享降级并回报 missing tags。
        prepare_missing_rule_sets_warn_only(self.root.path(), &value, &requests, &|_message| ())
            .await;
        inject_local_override_warn_only(self.root.path(), &mut value);
        crate::rules::normalize_singbox_ports(&mut value);
        apply_panel_features(&mut value, &features);
        remove_dashboard_download(&mut value);

        let available = lanpath_client::ruleset_manager::prepare_builtin_rule_sets(
            self.root.path(),
            &value,
            &requests,
        )
        .await;
        let report = lanpath_client::ruleset_manager::materialize_rule_sets(
            &mut value,
            self.root.path(),
            &available,
        );
        if !report.missing_tags.is_empty() {
            warnings.push(format!(
                "内置规则集缺失 {} 个，相关引用已按共享降级规则合成:{}",
                report.missing_tags.len(),
                report.missing_tags.join(", ")
            ));
        }

        let content = serde_json::to_string_pretty(&value)?;
        let warning_count = warnings.len();
        tracing::info!(target: "lanpath_desktop_nsh::config_composer", "配置合成预览完成：sing-box JSON，{node_count} 个节点，{warning_count} 条警告，缺失规则集 {} 个", report.missing_tags.len());
        Ok(ConfigPreview {
            format: PreviewFormat::Json,
            core_type: CoreType::SingBox,
            content,
            node_count,
            warnings,
            missing_tags: report.missing_tags,
        })
    }
}

/// 读取配置切片：共享 `ConfigSlicesStore::load` 对损坏文件只记 tracing
/// 警告并回退播种默认值，这里把同一回退判定显式化——文件不可读 / 非法
/// JSON / schema 不匹配都生成用户可见警告（与共享层同一回退结果）。
fn load_slices(root: &std::path::Path, warnings: &mut Vec<String>) -> ConfigSlices {
    let path = root.join("config_slices.json");
    if path.exists() {
        let unusable = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|value| serde_json::from_value::<ConfigSlices>(value).ok())
                .is_none(),
            Err(_) => true,
        };
        if unusable {
            warnings.push("配置切片读取失败，已回退共享默认值".to_owned());
        }
    }
    ConfigSlicesStore::new(root.to_path_buf())
        .load()
        .unwrap_or_default()
}

/// PanelFeatures 按 GPUI 字段（`config.normalized_rule_mode()`、
/// `dns_mode_from_slices`），`data_dir` 为本数据根 `cache`。
fn panel_features(
    root: &Path,
    slices: &ConfigSlices,
    config: &lanpath_client::ClientConfig,
) -> PanelFeatures {
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
        data_dir: root.join("cache").to_string_lossy().into_owned(),
        dns_mode: lanpath_client::core_config::dns_mode_from_slices(slices),
        github_proxy_prefix: config.github_proxy_prefix.clone(),
    }
}

/// 与 GPUI `remove_dashboard_download` 相同：预览不得携带面板目录/下载地址，
/// 避免任何消费方据此触发面板下载。
fn remove_dashboard_download(value: &mut Value) {
    if let Some(api) = value
        .pointer_mut("/experimental/clash_api")
        .and_then(Value::as_object_mut)
    {
        api.remove("external_ui");
        api.remove("external_ui_download_url");
    }
}

/// 与 GPUI `remove_mihomo_dashboard_download` 相同：Mihomo 预览不得携带
/// 面板目录/下载地址（`external-ui`/`external-ui-url` 为顶层键）。
fn remove_mihomo_dashboard_download(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("external-ui");
        object.remove("external-ui-url");
    }
}

/// 存储层会把内置分组（proxy/auto）播种进 outbounds 切片，那是默认内容的
/// 一部分；只有用户自己加过/改过非内置内容，才算“非默认切片”。
fn slices_customized(slices: &ConfigSlices) -> bool {
    let mut normalized = slices.clone();
    normalized.outbounds.items.retain(|item| !item.builtin);
    normalized != ConfigSlices::default()
}

#[cfg(test)]
#[path = "config_composer_tests.rs"]
mod tests;
