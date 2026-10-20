use lanpath_client::config_slices::{ConfigSlices, ConfigSlicesStore};
use lanpath_client::{
    EffectiveOverrides, PanelFeatures, ProfileStoreV2, SubscriptionStore, apply_panel_features,
    build_core_config_v2, compose_singbox_config, download_missing_rule_sets_warn_only,
    inject_local_override_warn_only, resolve_remote_overrides,
};
use lanpath_common::{LanpathError, LanpathResult};
use serde_json::Value;

use super::BusinessService;

impl BusinessService {
    pub(crate) async fn build_config(&self, download_rule_sets: bool) -> LanpathResult<Value> {
        let config = self.client_config()?;
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
            None => {
                selected = self.refresh_subscription(selected.id).await?;
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
                resolve_remote_overrides(&self.root.join("profile_cache"), &profile).await
            }
            None => (EffectiveOverrides::default(), Vec::new()),
        };
        for warning in warnings {
            tracing::warn!(%warning, "实验覆写远程资源降级");
        }
        let slices = load_slices(&self.root);
        let profile_config = build_core_config_v2(&subscription, &effective, &slices).await?;
        let mut value = compose_singbox_config(&profile_config, self.bindings.mixed_port(), None)?;
        if download_rule_sets {
            download_missing_rule_sets_warn_only(&self.root, &value, &|message| {
                tracing::info!(%message, "实验规则集启动补齐");
            })
            .await;
        }
        inject_local_override_warn_only(&self.root, &mut value);
        apply_panel_features(
            &mut value,
            &safe_features(self, &slices, config.normalized_rule_mode()),
        );
        remove_dashboard_download(&mut value);

        let available = if download_rule_sets {
            lanpath_client::ruleset_manager::ensure_builtin_rule_sets(&self.root, "").await
        } else {
            lanpath_client::ruleset_manager::existing_rule_sets(&self.root)
        };
        let report = lanpath_client::ruleset_manager::materialize_rule_sets(
            &mut value, &self.root, &available,
        );
        if !report.missing_tags.is_empty() {
            tracing::warn!(missing = ?report.missing_tags, "实验内置规则集缺失，按共享降级规则合成");
        }
        Ok(value)
    }

    pub(crate) async fn preview_config(&self) -> LanpathResult<String> {
        // Preview is the start gate, so it must exercise the same resource-fill
        // path as start; otherwise a newly referenced remote rule set would make
        // preview fail before start gets a chance to download it.
        let value = self.build_config(true).await?;
        self.validator().validate_value(&value).await?;
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

fn safe_features(
    service: &BusinessService,
    slices: &ConfigSlices,
    rule_mode: &str,
) -> PanelFeatures {
    PanelFeatures {
        tun_enabled: false,
        tun_stack: "mixed".into(),
        tun_auto_route: false,
        clash_api_enabled: true,
        clash_api_port: service.bindings.api_port(),
        clash_api_secret: String::new(),
        clash_api_ui: "zashboard".into(),
        rule_mode: rule_mode.to_string(),
        ipv6_enabled: false,
        dns_fakeip_enabled: false,
        data_dir: service.root.join("cache").to_string_lossy().into_owned(),
        dns_mode: lanpath_client::core_config::dns_mode_from_slices(slices),
        github_proxy_prefix: String::new(),
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
