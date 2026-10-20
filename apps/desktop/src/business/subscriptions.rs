use lanpath_client::{
    CachedSubscriptionContent, Subscription, SubscriptionSource, SubscriptionStore,
    fetch_subscription_for_config, normalize_resource_url, validate_subscription_url,
};
use lanpath_common::{LanpathError, LanpathResult};

use super::BusinessService;

impl BusinessService {
    pub(crate) fn subscription_store(&self) -> SubscriptionStore {
        SubscriptionStore::new(self.root.clone())
    }

    pub(crate) fn subscriptions(&self) -> LanpathResult<Vec<Subscription>> {
        self.subscription_store().load()
    }

    pub(crate) async fn add_subscription(
        &self,
        name: &str,
        url: &str,
        user_agent: Option<&str>,
        profile_id: Option<uuid::Uuid>,
    ) -> LanpathResult<Subscription> {
        let name = required_name(name, "订阅名称")?;
        let url = normalize_resource_url(url.trim());
        validate_subscription_url(&url).map_err(LanpathError::Validation)?;
        self.validate_profile_reference(profile_id)?;
        let user_agent = normalized_text(user_agent);
        let store = self.subscription_store();
        let mut subscription = store.add(&name, &url, true, user_agent)?;
        store.set_profile_id(subscription.id, profile_id)?;
        subscription.profile_id = profile_id;
        self.refresh_value(&store, &mut subscription).await;
        write_refresh_result(&store, &subscription)
    }

    pub(crate) async fn refresh_subscription(&self, id: uuid::Uuid) -> LanpathResult<Subscription> {
        let store = self.subscription_store();
        let mut subscription = store
            .load()?
            .into_iter()
            .find(|subscription| subscription.id == id)
            .ok_or_else(|| LanpathError::Client("订阅不存在".into()))?;
        self.refresh_value(&store, &mut subscription).await;
        write_refresh_result(&store, &subscription)
    }

    pub(crate) fn update_subscription(
        &self,
        id: uuid::Uuid,
        name: &str,
        url: &str,
        user_agent: Option<&str>,
        profile_id: Option<uuid::Uuid>,
    ) -> LanpathResult<Subscription> {
        let name = required_name(name, "订阅名称")?;
        let url = normalize_resource_url(url.trim());
        validate_subscription_url(&url).map_err(LanpathError::Validation)?;
        self.validate_profile_reference(profile_id)?;
        let store = self.subscription_store();
        store.update(id, &name, &url, normalized_text(user_agent))?;
        store.set_profile_id(id, profile_id)?;
        store
            .load()?
            .into_iter()
            .find(|subscription| subscription.id == id)
            .ok_or_else(|| LanpathError::Client("订阅不存在".into()))
    }

    pub(crate) fn remove_subscription(&self, id: uuid::Uuid) -> LanpathResult<()> {
        let mut config = self.client_config()?;
        self.subscription_store().remove(id)?;
        if config.active_subscription_id == Some(id) {
            config.active_subscription_id = None;
            self.set_active_subscription(None)?;
        }
        Ok(())
    }

    pub(crate) fn set_subscription_enabled(
        &self,
        id: uuid::Uuid,
        enabled: bool,
    ) -> LanpathResult<()> {
        self.subscription_store().set_enabled(id, enabled)?;
        if !enabled && self.active_subscription_id()? == Some(id) {
            self.set_active_subscription(None)?;
        }
        Ok(())
    }

    pub(crate) fn subscription_source(
        &self,
        id: uuid::Uuid,
    ) -> LanpathResult<Option<SubscriptionSource>> {
        self.subscription_store().load_source(id)
    }

    pub(crate) fn export_subscription_source(
        &self,
        id: uuid::Uuid,
    ) -> LanpathResult<std::path::PathBuf> {
        let source = self
            .subscription_source(id)?
            .ok_or_else(|| LanpathError::Client("尚无可导出的订阅原文，请先刷新".into()))?;
        let directory = self.root.join("exports");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("subscription-{id}.{}", source.extension()));
        std::fs::write(&path, source.content.as_bytes())?;
        Ok(path)
    }

    async fn refresh_value(&self, store: &SubscriptionStore, subscription: &mut Subscription) {
        let config = match self.client_config() {
            Ok(config) => config,
            Err(error) => {
                subscription.error = Some(format!("读取网络请求设置失败: {error}"));
                return;
            }
        };
        let result = fetch_subscription_for_config(
            &config,
            &subscription.url,
            subscription.user_agent.as_deref(),
        )
        .await;
        let still_current = store.load().ok().is_some_and(|subscriptions| {
            subscriptions.into_iter().any(|candidate| {
                candidate.id == subscription.id
                    && candidate.url == subscription.url
                    && candidate.user_agent == subscription.user_agent
            })
        });
        if !still_current {
            return;
        }
        match result {
            Ok(result) => {
                subscription.node_count = result.singbox_nodes.len() as u64;
                subscription.format = Some(result.format);
                subscription.userinfo = result.userinfo;
                subscription.error = None;
                let source = SubscriptionSource {
                    format: result.format,
                    content: result.source,
                };
                if let Err(error) = store.write_source(subscription.id, &source) {
                    tracing::warn!(id = %subscription.id, %error, "写入实验订阅原文失败");
                }
                let cache = CachedSubscriptionContent {
                    format: result.format,
                    singbox_nodes: result.singbox_nodes,
                };
                if let Err(error) = store.write_cached_content(subscription.id, &cache) {
                    tracing::warn!(id = %subscription.id, %error, "写入实验订阅节点缓存失败");
                }
            }
            Err(error) => subscription.error = Some(format!("拉取失败: {error}")),
        }
    }
}

pub(super) fn write_refresh_result(
    store: &SubscriptionStore,
    subscription: &Subscription,
) -> LanpathResult<Subscription> {
    let mut subscriptions = store.load()?;
    let target = subscriptions
        .iter_mut()
        .find(|candidate| candidate.id == subscription.id)
        .ok_or_else(|| LanpathError::Client("订阅已删除".into()))?;
    let changed = target.url == subscription.url && target.user_agent == subscription.user_agent;
    if changed {
        target.userinfo = subscription.userinfo.clone();
        target.node_count = subscription.node_count;
        target.format = subscription.format;
        target.error = subscription.error.clone();
    }
    let current = target.clone();
    if changed {
        store.save(&subscriptions)?;
    }
    Ok(current)
}

fn required_name(value: &str, field: &str) -> LanpathResult<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(LanpathError::Validation(format!("{field}不能为空")))
    } else {
        Ok(value.to_string())
    }
}

fn normalized_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
