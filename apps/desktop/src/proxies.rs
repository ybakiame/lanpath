//! 运行时出站策略与节点业务：通过 Clash API 查询策略组/节点、热切换选中节点、
//! 执行单节点与分组延迟测速、切换 Rule/Global/Direct 运行模式，并将选中记录与模式
//! 持久化到 `app.toml` `[client]`（下次核心启动时自动回放）。

use std::path::Path;
use std::sync::Arc;

use lanpath_client::core_config::push_clash_mode;
use lanpath_client::{
    DelayResult, GroupView, NodeView, clash_get_mode, clash_get_proxies, clash_select_proxy,
    clash_test_delay, clash_test_group,
};
use lanpath_common::{CoreType, LanpathError, LanpathResult};

use crate::core_runtime::CoreRuntime;
use crate::data_root::{DataRoot, open_root, resolve_data_location};
use crate::request_settings::load_request_config;

/// 当前运行中核心的 Clash API 端点快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClashEndpoint {
    pub(crate) port: u16,
    pub(crate) secret: String,
    pub(crate) core_type: CoreType,
}

/// 一次从运行中核心拉取的完整策略与节点视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveProxiesSnapshot {
    pub(crate) groups: Vec<GroupView>,
    pub(crate) nodes: Vec<NodeView>,
    pub(crate) mode: String,
    pub(crate) endpoint: ClashEndpoint,
}

/// 桌面端运行时策略与节点服务句柄。
#[derive(Clone)]
pub(crate) struct ProxiesService {
    root: Arc<DataRoot>,
}

impl ProxiesService {
    pub(crate) fn open() -> LanpathResult<Self> {
        let location = resolve_data_location()?;
        Self::for_root(&location.path)
    }

    pub(crate) fn for_root(path: &Path) -> LanpathResult<Self> {
        Ok(Self {
            root: open_root(path)?,
        })
    }

    /// 读取持久化保存的运行模式（`rule` / `global` / `direct`），核心未运行时也可用。
    pub(crate) fn load_saved_rule_mode(&self) -> String {
        let _guard = self.root.business_guard();
        load_request_config(self.root.path())
            .map(|c| normalize_rule_mode(&c.rule_mode).to_owned())
            .unwrap_or_else(|_| "rule".to_owned())
    }

    /// 检查当前数据根是否有运行中的核心且启用了 Clash API；若测试环境指定了显式覆写端点则优先使用。
    pub(crate) fn active_endpoint(&self) -> Option<ClashEndpoint> {
        if let Some(runtime) = CoreRuntime::cached(self.root.path())
            && let Some((cfg, _)) = runtime.diagnostic_context()
            && cfg.clash_api_enabled
            && cfg.clash_api_port > 0
        {
            return Some(ClashEndpoint {
                port: cfg.clash_api_port,
                secret: cfg.clash_api_secret,
                core_type: cfg.core_type,
            });
        }
        None
    }

    /// 读取已保存的 Clash API 配置（用于判断「核心未运行」还是「已关闭 Clash API」）。
    pub(crate) fn saved_clash_api_enabled(&self) -> bool {
        let _guard = self.root.business_guard();
        load_request_config(self.root.path())
            .map(|c| c.clash_api_enabled)
            .unwrap_or(true)
    }

    /// 拉取当前运行核心的策略组、节点列表与实时模式。
    pub(crate) async fn fetch_live_proxies(&self) -> LanpathResult<LiveProxiesSnapshot> {
        let endpoint = self.require_endpoint()?;
        let mut list = clash_get_proxies(endpoint.port, &endpoint.secret).await?;
        sort_groups_for_display(&mut list.groups);
        let mode = match clash_get_mode(endpoint.port, &endpoint.secret).await {
            Ok(m) => normalize_rule_mode(&m).to_owned(),
            Err(_) => self.load_saved_rule_mode(),
        };
        Ok(LiveProxiesSnapshot {
            groups: list.groups,
            nodes: list.nodes,
            mode,
            endpoint,
        })
    }

    /// 在运行中核心的指定策略组中切换目标成员，并持久化到 `app.toml`。
    pub(crate) async fn select_proxy(&self, group: &str, member: &str) -> LanpathResult<()> {
        let endpoint = self.require_endpoint()?;
        clash_select_proxy(endpoint.port, &endpoint.secret, group, member).await?;
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let mut cfg = load_request_config(self.root.path())?;
        cfg.group_selections
            .insert(group.to_owned(), member.to_owned());
        cfg.save()?;
        tracing::info!(
            target: "lanpath_desktop_nsh::proxies",
            group,
            member,
            "已切换策略组节点并持久化"
        );
        Ok(())
    }

    /// 对单个节点或出站执行 Clash API 延迟测速。
    pub(crate) async fn test_node_delay(&self, name: &str) -> LanpathResult<Option<u16>> {
        let endpoint = self.require_endpoint()?;
        clash_test_delay(endpoint.port, &endpoint.secret, name, None, 5000).await
    }

    /// 并发测试指定策略组内全部成员延迟。
    pub(crate) async fn test_group_delay(&self, group: &str) -> LanpathResult<Vec<DelayResult>> {
        let endpoint = self.require_endpoint()?;
        clash_test_group(endpoint.port, &endpoint.secret, group).await
    }

    /// 切换运行模式（`rule` / `global` / `direct`）：始终写入 `app.toml`，核心运行中则同时热推送。
    pub(crate) async fn set_rule_mode(&self, mode: &str) -> LanpathResult<String> {
        let normalized = normalize_rule_mode(mode).to_owned();
        {
            let _guard = self.root.business_guard();
            self.root.ensure_writable()?;
            let mut cfg = load_request_config(self.root.path())?;
            cfg.rule_mode.clone_from(&normalized);
            cfg.save()?;
        }
        if let Some(endpoint) = self.active_endpoint() {
            push_clash_mode(endpoint.port, &endpoint.secret, &normalized).await?;
        }
        tracing::info!(
            target: "lanpath_desktop_nsh::proxies",
            mode = %normalized,
            "已更新代理运行模式"
        );
        Ok(normalized)
    }

    fn require_endpoint(&self) -> LanpathResult<ClashEndpoint> {
        self.active_endpoint().ok_or_else(|| {
            if self.saved_clash_api_enabled() {
                LanpathError::Client(
                    "核心未运行，请先在「概览」页启动核心后再管理策略与节点".into(),
                )
            } else {
                LanpathError::Client(
                    "Clash API 未启用，请在「配置切片」页开启控制 API 并重启核心".into(),
                )
            }
        })
    }
}

/// 归一化规则模式；未知输入回退 `rule`。
pub(crate) fn normalize_rule_mode(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "global" => "global",
        "direct" => "direct",
        _ => "rule",
    }
}

/// 按语义优先级排序策略组：`proxy` → `auto` → 自定义组（按字母） → `final` / `global`。
fn sort_groups_for_display(groups: &mut [GroupView]) {
    fn rank(name: &str) -> (u8, String) {
        let lower = name.to_ascii_lowercase();
        let bucket = match lower.as_str() {
            "proxy" => 0,
            "auto" => 1,
            "final" => 3,
            "global" => 4,
            _ => 2,
        };
        (bucket, lower)
    }
    groups.sort_by_key(|g| rank(&g.name));
}
