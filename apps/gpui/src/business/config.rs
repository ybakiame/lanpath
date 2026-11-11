use lanpath_client::config_slices::{ConfigSlices, ConfigSlicesStore};
use lanpath_client::{
    EffectiveOverrides, PanelFeatures, ProfileStoreV2, RemoteFetchContext, RequestProxyMode,
    SubscriptionStore, apply_panel_features, build_core_config_v2, compose_mihomo_config,
    compose_singbox_config, core_config::MitmChain, inject_local_override_warn_only,
    prepare_missing_rule_sets_warn_only, resolve_remote_overrides_with_context,
};
use lanpath_common::{LanpathError, LanpathResult};
use serde_json::Value;

use super::BusinessService;

impl BusinessService {
    #[cfg(test)]
    pub(crate) async fn build_config(&self, download_rule_sets: bool) -> LanpathResult<Value> {
        self.build_config_report(download_rule_sets, None, self.client_config()?)
            .await
            .map(|(value, _)| value)
    }

    pub(crate) async fn build_config_report(
        &self,
        download_rule_sets: bool,
        mitm_chain: Option<MitmChain>,
        config: lanpath_client::ClientConfig,
    ) -> LanpathResult<(Value, Vec<String>)> {
        // Both preview and start prepare before a core session is admitted.
        // A saved Core port is never permission to borrow an unrelated listener.
        let cache_only = config.request_proxy_mode == RequestProxyMode::Core;
        let use_mihomo = config.core_type == lanpath_common::CoreType::Mihomo;
        let selected_id = config
            .active_subscription_id
            .ok_or_else(|| LanpathError::Client("请先在订阅页选择要使用的订阅".into()))?;
        let store = SubscriptionStore::new(self.root.clone());
        let mut selected = store
            .load()?
            .into_iter()
            .find(|subscription| subscription.id == selected_id)
            .ok_or_else(|| LanpathError::Client("所选订阅不存在，请重新选择".into()))?;
        if !selected.enabled {
            return Err(LanpathError::Client(
                "所选订阅已停用，请先启用或重新选择".into(),
            ));
        }
        let nodes = match store.load_cached_content(selected.id) {
            Some(cached) => cached.singbox_nodes,
            None if cache_only => {
                return Err(LanpathError::Client(
                    "核心请求代理尚未就绪，且所选订阅没有可用节点缓存；请先显式选择其他请求策略获取订阅，或导入已有缓存".into(),
                ));
            }
            None => {
                selected = self
                    .refresh_subscription_for_config(selected.id, &config)
                    .await?;
                store
                    .load_cached_content(selected.id)
                    .map(|cached| cached.singbox_nodes)
                    .ok_or_else(|| {
                        LanpathError::Client(
                            selected
                                .error
                                .unwrap_or_else(|| "订阅没有可用的本地节点缓存，请先刷新".into()),
                        )
                    })?
            }
        };
        let slices = load_slices(&self.root);
        let features = panel_features(self, &slices, &config);
        let mixed_port = config.mixed_port;
        let requests = if cache_only {
            RemoteFetchContext::cache_only()
        } else {
            RemoteFetchContext::stopped(config)?
        };
        let source = store.load_source(selected.id)?;
        let subscription = serde_json::json!({ "outbounds": nodes });
        let (effective, warnings) = match selected.profile_id {
            Some(profile_id) => {
                let profile = ProfileStoreV2::new(self.root.clone())
                    .load()?
                    .into_iter()
                    .find(|profile| profile.id == profile_id)
                    .ok_or_else(|| {
                        LanpathError::Client("订阅关联的覆写模板不存在，请重新关联".into())
                    })?;
                resolve_remote_overrides_with_context(
                    &lanpath_client::resource_storage::cache_path(&self.root, "profile_cache"),
                    &profile,
                    &requests,
                )
                .await
            }
            None => (EffectiveOverrides::default(), Vec::new()),
        };
        for warning in warnings {
            tracing::warn!(%warning, "实验覆写远程资源降级");
        }
        if use_mihomo {
            if slices != ConfigSlices::default() {
                tracing::warn!(
                    "Mihomo 启动未应用 sing-box 配置切片；配置页内容保留供切换回 sing-box 使用"
                );
            }
            let nodes = lanpath_client::subscription::mihomo_nodes_from_subscription(
                source.as_ref(),
                subscription["outbounds"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            )?;
            let mut profile_config =
                lanpath_client::build_mihomo_config_v2(&nodes, &effective).await?;
            compose_mihomo_config(&mut profile_config, mixed_port, mitm_chain)?;
            let mut value = profile_config;
            lanpath_client::core_config::apply_mihomo_panel_features(&mut value, &features);
            remove_mihomo_dashboard_download(&mut value);
            return Ok((value, Vec::new()));
        }
        let profile_config = build_core_config_v2(&subscription, &effective, &slices).await?;
        let mut value = compose_singbox_config(&profile_config, mixed_port, mitm_chain)?;
        if download_rule_sets {
            prepare_missing_rule_sets_warn_only(&self.root, &value, &requests, &|message| {
                tracing::info!(%message, "实验规则集启动补齐");
            })
            .await;
        }
        inject_local_override_warn_only(&self.root, &mut value);
        apply_panel_features(&mut value, &features);
        remove_dashboard_download(&mut value);

        let available = if download_rule_sets {
            lanpath_client::ruleset_manager::prepare_builtin_rule_sets(
                &self.root, &value, &requests,
            )
            .await
        } else {
            lanpath_client::ruleset_manager::existing_rule_sets(&self.root)
        };
        let report = lanpath_client::ruleset_manager::materialize_rule_sets(
            &mut value, &self.root, &available,
        );
        if !report.missing_tags.is_empty() {
            tracing::warn!(missing = ?report.missing_tags, "实验内置规则集缺失，按共享降级规则合成");
        }
        Ok((value, report.missing_tags))
    }

    pub(crate) async fn preview_config(&self) -> LanpathResult<String> {
        // Preview is the start gate, so it must exercise the same resource-fill
        // path as start; otherwise a newly referenced remote rule set would make
        // preview fail before start gets a chance to download it.
        let config = self.client_config()?;
        let validator = self.validator(&config);
        let (value, _) = self.build_config_report(true, None, config).await?;
        validator.validate_value(&value).await?;
        Ok(serde_json::to_string_pretty(&value)?)
    }
}

fn load_slices(root: &std::path::Path) -> ConfigSlices {
    match ConfigSlicesStore::new(root.to_path_buf()).load() {
        Ok(slices) => slices,
        Err(error) => {
            tracing::warn!(%error, "实验配置片段加载失败，回退共享默认值");
            ConfigSlices::default()
        }
    }
}

fn panel_features(
    service: &BusinessService,
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
        data_dir: service.root.join("cache").to_string_lossy().into_owned(),
        dns_mode: lanpath_client::core_config::dns_mode_from_slices(slices),
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
