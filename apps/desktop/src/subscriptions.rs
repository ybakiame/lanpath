//! 订阅业务：共享 `lanpath-client::SubscriptionStore` 落在 NSH 桌面自己的
//! 隔离数据根内。不读取/迁移 Tauri/GPUI/apps/nsh 数据；仅在新增/刷新
//! 订阅时按需联网，不启动核心、系统代理或 TUN。
//!
//! 所有清单/配置的读写都经数据根的共享业务锁串行化：刷新的后台写回
//! 与 UI 线程的启停/生效/删除/编辑/覆写绑定在同一侧排队，且与覆写模板
//! 业务（Profiles）共用同一把锁——校验引用与落盘在同一临界区内完成。
//! 锁不跨网络 await：刷新在请求前后分别短暂取锁。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};

use lanpath_client::{
    CachedSubscriptionContent, ClientConfig, RequestProxyMode, SubFormat, Subscription,
    SubscriptionSource, SubscriptionStore, fetch_subscription_for_config, normalize_resource_url,
    validate_subscription_url,
};
use lanpath_common::{LanpathError, LanpathResult};
use uuid::Uuid;

use crate::data_root::{DataRoot, open_root, resolve_data_location};
use crate::profiles::validate_profile_reference_locked;

/// 每数据根一个订阅业务句柄：Weak 缓存，全部句柄释放后自动移除。
/// 同根句柄共享同一个 `Arc`：切页后新页面拿到的是同一个句柄，
/// 在途刷新的旧句柄与新句柄经数据根的共享业务锁排队，不会并发读写。
static SERVICES: OnceLock<std::sync::Mutex<HashMap<PathBuf, Weak<Subscriptions>>>> =
    OnceLock::new();

/// 一次刷新的落盘结果：成功（节点数/格式已写回）、失败（真实错误已写回
/// 记录、旧原文/缓存保留）或已过期（刷新期间订阅被编辑/删除，结果未写入，
/// 避免旧结果覆盖新状态）。
#[derive(Debug)]
pub(crate) enum RefreshStatus {
    Updated { node_count: u64, format: SubFormat },
    Failed { error: String },
    Stale,
}

pub(crate) struct Subscriptions {
    root: Arc<DataRoot>,
}

impl Subscriptions {
    /// 解析本进程数据根并打开订阅业务（应用启动路径）。
    pub(crate) fn open() -> LanpathResult<Arc<Self>> {
        let location = resolve_data_location()?;
        Self::open_service(&location.path)
    }

    /// 指定数据根打开（测试与显式调用路径，同样走完整准入）。
    #[cfg(test)]
    pub(crate) fn for_root(path: &Path) -> LanpathResult<Arc<Self>> {
        Self::open_service(path)
    }

    /// 按规范根路径缓存句柄：同根重开（页面切换、测试重开）拿到同一个
    /// `Arc`；所有句柄释放后 Weak 条目自然失效。业务锁在共享数据根上，
    /// 同根的 Profiles/Subscriptions 操作天然互斥。
    fn open_service(path: &Path) -> LanpathResult<Arc<Self>> {
        let root = open_root(path)?;
        let key = root.path().to_path_buf();
        let mut services = SERVICES
            .get_or_init(|| std::sync::Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(service) = services.get(&key).and_then(Weak::upgrade) {
            return Ok(service);
        }
        let service = Arc::new(Self { root });
        services.insert(key, Arc::downgrade(&service));
        Ok(service)
    }

    fn store(&self) -> SubscriptionStore {
        SubscriptionStore::new(self.root.path().to_path_buf())
    }

    /// 读取订阅列表；文件缺失按空列表，损坏时共享存储层已回退空列表并告警。
    pub(crate) fn load(&self) -> LanpathResult<Vec<Subscription>> {
        let _guard = self.root.business_guard();
        self.store().load()
    }

    /// 校验并新增订阅后持久化；任何失败都不触碰已有数据。
    /// 新增订阅的覆写模板绑定固定为 `None`（在编辑弹窗中另行绑定）。
    pub(crate) fn add(
        &self,
        name: &str,
        url: &str,
        user_agent: Option<&str>,
    ) -> LanpathResult<Subscription> {
        let name = required_text(name, "订阅名称")?;
        let url = normalize_resource_url(url.trim());
        validate_subscription_url(&url).map_err(LanpathError::Validation)?;
        let _guard = self.root.business_guard();
        let subscription = self
            .store()
            .add(&name, &url, true, normalized_text(user_agent))?;
        tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "新增订阅：{}", subscription.name.escape_debug());
        Ok(subscription)
    }

    /// 编辑订阅名称/地址/可选 User-Agent/覆写模板绑定。URL 变更时沿用
    /// 共享 `store.update` 清除旧原文/缓存与上次结果；URL 不变则保留。
    /// 绑定非空时校验模板真实存在（同一锁内与模板清单比对），失效引用
    /// 报真实错误、不落盘、不静默改绑。
    pub(crate) fn update(
        &self,
        id: Uuid,
        name: &str,
        url: &str,
        user_agent: Option<&str>,
        profile_id: Option<Uuid>,
    ) -> LanpathResult<Subscription> {
        let name = required_text(name, "订阅名称")?;
        let url = normalize_resource_url(url.trim());
        validate_subscription_url(&url).map_err(LanpathError::Validation)?;
        let _guard = self.root.business_guard();
        let store = self.store();
        if let Some(profile_id) = profile_id {
            validate_profile_reference_locked(self.root.path(), profile_id)?;
        }
        store.update(id, &name, &url, normalized_text(user_agent))?;
        store.set_profile_id(id, profile_id)?;
        tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "编辑订阅：{}", name.escape_debug());
        store
            .load()?
            .into_iter()
            .find(|subscription| subscription.id == id)
            .ok_or_else(|| LanpathError::Client("订阅不存在".into()))
    }

    /// 停用当前生效订阅时同时取消其生效状态；不影响覆写模板绑定。
    pub(crate) fn set_enabled(&self, id: Uuid, enabled: bool) -> LanpathResult<()> {
        let _guard = self.root.business_guard();
        self.store().set_enabled(id, enabled)?;
        if !enabled {
            self.clear_active_if_matches_locked(id)?;
        }
        tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "{}订阅：{id}", if enabled { "启用" } else { "停用" });
        Ok(())
    }

    /// 删除订阅（共享存储层同时清除原文/缓存）；删除当前生效订阅时
    /// 同时取消其生效状态。
    pub(crate) fn remove(&self, id: Uuid) -> LanpathResult<()> {
        let _guard = self.root.business_guard();
        self.store().remove(id)?;
        self.clear_active_if_matches_locked(id)?;
        tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "删除订阅：{id}");
        Ok(())
    }

    /// 最后一次成功刷新的原文；从未成功刷新过时为 `None`。
    pub(crate) fn source(&self, id: Uuid) -> LanpathResult<Option<SubscriptionSource>> {
        let _guard = self.root.business_guard();
        self.store().load_source(id)
    }

    /// 按需联网刷新：始终使用已保存的 URL/User-Agent，绝不回写未保存表单；
    /// 网络请求不在 UI 线程执行（由调用方放入后台任务），写回与 UI 线程的
    /// 同步操作经互斥排队，不并发写坏清单。
    ///
    /// 返回前后都重新校验 id/URL/User-Agent：刷新期间被编辑或删除时结果为
    /// [`RefreshStatus::Stale`]，旧响应不会覆盖新状态。
    pub(crate) async fn refresh(&self, id: Uuid) -> LanpathResult<RefreshStatus> {
        let (subscription, config) = {
            let _guard = self.root.business_guard();
            let subscription = self
                .store()
                .load()?
                .into_iter()
                .find(|subscription| subscription.id == id)
                .ok_or_else(|| LanpathError::Client("订阅不存在".into()))?;
            (subscription, request_config(self.root.path())?)
        };
        // 共享请求策略：连接/超时失败自动重试一次，请求自身有 30 秒超时。
        let result = fetch_subscription_for_config(
            &config,
            &subscription.url,
            subscription.user_agent.as_deref(),
        )
        .await;

        // 写回前重新读取并比对：id 消失（被删除）或 URL/UA 变化（被编辑）
        // 都丢弃本次结果。
        let _guard = self.root.business_guard();
        let store = self.store();
        let mut subscriptions = store.load()?;
        let Some(target) = subscriptions
            .iter_mut()
            .find(|candidate| candidate.id == id)
        else {
            return Ok(RefreshStatus::Stale);
        };
        if target.url != subscription.url || target.user_agent != subscription.user_agent {
            return Ok(RefreshStatus::Stale);
        }
        match result {
            Ok(fetched) => {
                let format = fetched.format;
                let node_count = fetched.singbox_nodes.len() as u64;
                target.node_count = node_count;
                target.format = Some(format);
                target.userinfo = fetched.userinfo;
                target.error = None;
                let source = SubscriptionSource {
                    format,
                    content: fetched.source,
                };
                store.write_source(id, &source)?;
                let cache = CachedSubscriptionContent {
                    format,
                    singbox_nodes: fetched.singbox_nodes,
                };
                store.write_cached_content(id, &cache)?;
                store.save(&subscriptions)?;
                tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "刷新订阅完成：{node_count} 个节点");
                Ok(RefreshStatus::Updated { node_count, format })
            }
            Err(error) => {
                let message = format!("拉取失败: {error}");
                target.error = Some(message.clone());
                store.save(&subscriptions)?;
                tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "刷新订阅失败：{message}");
                Ok(RefreshStatus::Failed { error: message })
            }
        }
    }

    /// 当前生效订阅（持久化在本应用 `client.json` 的 `active_subscription_id`）。
    pub(crate) fn active_id(&self) -> LanpathResult<Option<Uuid>> {
        let _guard = self.root.business_guard();
        Ok(request_config(self.root.path())?.active_subscription_id)
    }

    /// 设置当前生效订阅：拒绝不存在或已停用的订阅。
    pub(crate) fn set_active(&self, id: Uuid) -> LanpathResult<()> {
        let _guard = self.root.business_guard();
        let selected = self
            .store()
            .load()?
            .into_iter()
            .find(|subscription| subscription.id == id);
        match selected {
            Some(subscription) if subscription.enabled => {}
            Some(_) => return Err(LanpathError::Client("所选订阅已停用，请先启用".into())),
            None => return Err(LanpathError::Client("所选订阅不存在".into())),
        }
        let mut config = request_config(self.root.path())?;
        config.active_subscription_id = Some(id);
        config.save()?;
        tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "设置当前生效订阅：{id}");
        Ok(())
    }

    /// 生效订阅被停用/删除时清空选择（`id` 不匹配则不动配置）。
    /// 调用方须已持有写锁。
    fn clear_active_if_matches_locked(&self, id: Uuid) -> LanpathResult<()> {
        let mut config = request_config(self.root.path())?;
        if config.active_subscription_id == Some(id) {
            config.active_subscription_id = None;
            config.save()?;
        }
        Ok(())
    }
}

/// 本应用的网络请求配置：共享 `load_for_requests` 读请求设置，`data_dir`
/// 由它固定为本应用数据根，不信任文件里的旧路径。已有配置原样保留
/// `request_proxy_mode` / `request_proxy_url` / `github_proxy_prefix` 等
/// 请求字段（active 的保存不改写它们，刷新遵守已保存策略）；只有新装
/// （数据根还没有 `client.json` / `app.toml`）才设直连 `None`，与 GPUI
/// 为新配置设定的策略一致，避免环境系统代理把订阅地址转发到第三方。
fn request_config(root: &std::path::Path) -> LanpathResult<ClientConfig> {
    let fresh_install = !root.join("client.json").exists()
        && !lanpath_client::settings::app_settings_path(root).exists();
    let mut config = ClientConfig::load_for_requests(root)?;
    if fresh_install {
        config.request_proxy_mode = RequestProxyMode::None;
    }
    Ok(config)
}

fn required_text(value: &str, field: &str) -> LanpathResult<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(LanpathError::Validation(format!("{field}不能为空")))
    } else {
        Ok(value.to_string())
    }
}

/// 空白 User-Agent 归一化为 `None`（共享存储层按 `None` 使用默认 UA）。
fn normalized_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "subscriptions_tests.rs"]
mod tests;
