//! Startup composition and progress reporting.
use super::*;

impl ClientState {
    /// Start with synchronous stage feedback; never include subscription URLs or secrets.
    pub async fn start_with_progress(
        &mut self,
        progress: &(dyn Fn(&str) + Send + Sync),
    ) -> LanpathResult<()> {
        // Always reload latest config from disk: settings page saves immediately (8eab048) only writes
        // client.json to disk, this instance may hold an old snapshot; starting with cached config would make
        // saved settings (e.g. TUN toggle) not take effect. data_dir is determined by app state, use
        // the instance's current value (prevents old path in client.json from overwriting).
        let data_dir = self.config.data_dir.clone();
        let mut loaded = ClientConfig::load(&data_dir)?;
        loaded.data_dir = data_dir;
        self.config = loaded;

        // Android forced override: MITM / system proxy / TUN are desktop-exclusive features,
        // no corresponding implementation on Android or would incorrectly block startup (TUN privilege check,
        // system proxy stub). Normalize immediately after config load to avoid startup failure from persisted desktop config.
        #[cfg(target_os = "android")]
        compat::apply_android_overrides(&mut self.config);

        // TUN mode pre-start privilege check: if not authorized, reject startup and return clear error (error message starts with
        // `tun_auth_required` and contains binary path, frontend shows authorization entry accordingly).
        //
        // Android authorization goes through VpnService system dialog (VpnPlugin.prepare), unrelated to desktop
        // privilege check, skip desktop core binary authorization pre-check (and Android has no `core_binary`).
        #[cfg(not(target_os = "android"))]
        if self.config.tun_enabled {
            match (self.tun_auth_check)(&self.config.core_binary) {
                TunAuthStatus::Authorized => {}
                TunAuthStatus::NeedsAuth => {
                    return Err(LanpathError::Client(format!(
                        "tun_auth_required: TUN 模式需要特权，请先授权核心二进制 {}",
                        self.config.core_binary.display()
                    )));
                }
                TunAuthStatus::Unsupported(reason) => {
                    return Err(LanpathError::Client(format!(
                        "tun_auth_required: {reason}（核心二进制 {}）",
                        self.config.core_binary.display()
                    )));
                }
            }
        }

        progress("读取订阅与节点缓存");
        tracing::info!("客户端启动：解析订阅");
        let sub_store = subscription::SubscriptionStore::new(self.config.data_dir.clone());

        // Subscription selection: the subscription selected on home page (config.active_subscription_id) is the only one effective.
        // Selected subscription must exist and be enabled (can be selected on home page).
        //
        // Subscription → Profile layer: subscription content is only used to extract nodes, local template generates groups/routing; override
        // template takes the currently effective subscription's `profile_id` (see override parsing below).
        //
        // 订阅格式兼容：ClashYaml 订阅在拉取时已转换为 sing-box 节点（见
        // [`subscription::parse_subscription_body`] 的 `singbox_nodes`），三种格式统一走
        // sing-box 本地模板路径。
        let active_sub = match self.config.active_subscription_id {
            Some(id) => Some(
                sub_store
                    .load()?
                    .into_iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| {
                        LanpathError::Client("所选订阅不存在，请在首页重新选择".to_string())
                    })?,
            ),
            None => None,
        };
        let (linked_profile_id, sub_content) = if let Some(sub) = active_sub {
            if !sub.enabled {
                return Err(LanpathError::Client(
                    "所选订阅已停用，请在订阅页启用或在首页重新选择".to_string(),
                ));
            }
            // 启动 VPN 不主动拉取订阅（2026-09 起）：订阅更新只在用户手动刷新（或后续
            // 的自动更新）时进行，两者都会把内容写入 `subscription_cache/<id>.json`；
            // 启动一律读缓存。缓存缺失/损坏（如手工删档、旧版本从未写过缓存）才拉取
            // 一次兜底并回写缓存——否则首次启动将无节点可用。
            let nodes = match sub_store.load_cached_content(sub.id) {
                Some(cached) => cached.singbox_nodes,
                None => {
                    tracing::info!("订阅缓存缺失，启动时兜底拉取一次");
                    let fetch = subscription::fetch_subscription_for_config(
                        &self.config,
                        &sub.url,
                        sub.user_agent.as_deref(),
                    )
                    .await?;
                    let source = subscription::SubscriptionSource {
                        format: fetch.format,
                        content: fetch.source.clone(),
                    };
                    if let Err(e) = sub_store.write_source(sub.id, &source) {
                        tracing::warn!(error = %e, "回写订阅原文失败（不阻断启动）");
                    }
                    let cache = subscription::CachedSubscriptionContent {
                        format: fetch.format,
                        singbox_nodes: fetch.singbox_nodes.clone(),
                    };
                    if let Err(e) = sub_store.write_cached_content(sub.id, &cache) {
                        tracing::warn!(error = %e, "回写订阅缓存失败（不阻断启动）");
                    }
                    fetch.singbox_nodes
                }
            };
            let content = serde_json::json!({
                "outbounds": nodes,
            });
            (sub.profile_id, content)
        } else {
            return Err(LanpathError::Client(
                "请先在首页选择要使用的订阅".to_string(),
            ));
        };
        progress("合成配置与订阅覆写");
        let store = profile::ProfileStoreV2::new(self.config.data_dir.clone());
        // Override parsing (pure association): the override used at runtime = the override template associated with the currently effective subscription;
        // when subscription is not associated, no override is used. When template does not exist, return clear error.
        // When matched, parse remote override (fetch/cache fallback/skip) → remote as base, local overlay v2 build flow.
        let (effective, warnings) = match linked_profile_id {
            Some(pid) => {
                let profiles = store.load()?;
                let linked = profiles.iter().find(|p| p.id == pid).ok_or_else(|| {
                    LanpathError::Client("订阅关联的覆写模板不存在，请在首页重新关联".to_string())
                })?;
                profile::resolve_remote_overrides(
                    &self.config.data_dir.join("profile_cache"),
                    linked,
                )
                .await
            }
            None => (profile::EffectiveOverrides::default(), Vec::new()),
        };
        for warning in &warnings {
            tracing::warn!(warning, "profile remote override");
        }
        // ⓪ Config slices (ADR-0005 §3.2): load from `data_dir/config_slices.json` and inject
        // into the template before profile overrides. A load failure (unreadable/corrupted file
        // is already handled inside the store) falls back to default so startup never blocks.
        let slices =
            match crate::config_slices::ConfigSlicesStore::new(self.config.data_dir.clone()).load()
            {
                Ok(slices) => slices,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "failed to load config_slices.json, falling back to default"
                    );
                    crate::config_slices::ConfigSlices::default()
                }
            };
        let profile_cfg = profile::build_core_config_v2(&sub_content, &effective, &slices).await?;

        // MITM starts before core: need listen address to inject core routing rules.
        let chain = self.start_mitm_chain().await?;

        // Scheduled task scheduler: after phase ③ decoupling, independent of MITM startup.
        // Read task scripts from remote cache (regardless of MITM enabled), start scheduler as long as there are enabled tasks.
        // Failure is only logged as warning, does not block core startup (isolated from MITM failure).
        if let Err(e) = self.start_scheduler_from_cache().await {
            tracing::warn!(error = %e, "定时任务调度器启动失败（不影响核心启动）");
        }

        // TUN / Clash dashboard config (settings page highest priority): forcibly injected after compose (build_core_config +
        // override), same-name fields in template/override are replaced as a whole by settings.
        //
        // Android's TUN toggle is decoupled from desktop semantics: Android traffic is taken over by VpnService (i.e. TUN),
        // sing-box needs config-level tun inbound to trigger libbox callback `openTun()` to establish VPN interface
        // (core shows running but no interface without it), so Android always forces `tun_enabled=true`.
        // Desktop passes through user settings as-is.
        let features = core_config::PanelFeatures {
            tun_enabled: compat::panel_features_tun_enabled(
                cfg!(target_os = "android"),
                self.config.tun_enabled,
            ),
            tun_stack: self.config.tun_stack.clone(),
            tun_auto_route: compat::panel_features_tun_auto_route(
                cfg!(target_os = "android"),
                self.config.tun_auto_route,
            ),
            clash_api_enabled: self.config.clash_api_enabled,
            clash_api_port: self.config.clash_api_port,
            clash_api_secret: self.config.clash_api_secret.clone(),
            clash_api_ui: self.config.clash_api_ui.clone(),
            rule_mode: self.config.normalized_rule_mode().to_string(),
            ipv6_enabled: self.config.ipv6_enabled,
            dns_fakeip_enabled: self.config.dns_fakeip_enabled,
            // FakeIP cache file pinned to the client data dir (explicit persistent path).
            data_dir: self.config.data_dir.to_string_lossy().into_owned(),
            // ADR-0005 D1: Android DNS takeover only when the DNS slice is enabled and set to
            // takeover; otherwise follow system (keep the forced injection).
            dns_mode: core_config::dns_mode_from_slices(&slices),
            github_proxy_prefix: self.config.github_proxy_prefix.clone(),
        };
        let mut config_json = match core_config::compose_singbox_config(
            &profile_cfg,
            self.config.mixed_port,
            chain,
        ) {
            Ok(cfg) => cfg,
            Err(e) => {
                self.rollback_mitm_started().await;
                return Err(e);
            }
        };
        // 启动时补齐「被引用但尚未下载」的社区规则集（best-effort）：用户在市场/规则集
        // 页添加 remote 规则集后可能从未点过「立即更新」，backing 文件缺失会导致引用它的
        // 规则在物化阶段被剥离（静默失效）；启动时补下载一次，失败仅告警（规则仍被剥
        // 离，核心照常启动）。
        crate::local_override::download_missing_rule_sets_warn_only(
            &self.config.data_dir,
            &config_json,
            progress,
        )
        .await;

        // [ADR-0002] Inject local override after compose, before panel features.
        crate::local_override::inject_local_override_warn_only(
            &self.config.data_dir,
            &mut config_json,
        );
        core_config::apply_panel_features(&mut config_json, &features);

        // 内置 CN 规则集本地化物化（ruleset_manager）：App 侧按镜像回退下载（jsDelivr →
        // GitHub raw → GitHub 代理前缀）并改写为 type:local；下载全部失败的 tag 连同引用
        // 它的路由 / DNS 规则降级移除——优先保证核心可启动，缺失 tag 暴露到运行状态供
        // 命令层后台重试与自动重载。
        progress("准备内置分流规则集（缺失时联网下载）");
        let available = crate::ruleset_manager::ensure_builtin_rule_sets(
            &self.config.data_dir,
            &self.config.github_proxy_prefix,
        )
        .await;
        let report = crate::ruleset_manager::materialize_rule_sets(
            &mut config_json,
            &self.config.data_dir,
            &available,
        );
        if !report.missing_tags.is_empty() {
            tracing::warn!(
                missing = ?report.missing_tags,
                stripped_route_rules = report.stripped_route_rules,
                stripped_dns_rules = report.stripped_dns_rules,
                "内置规则集未全部就绪，降级启动（CN 分流由 route.final 兜底）"
            );
        }
        self.missing_rule_sets = report.missing_tags;

        progress("启动代理核心与网络服务");
        self.start_services(&config_json).await?;

        // 运行状态扩展：记录本次合成配置的规则条数（未运行时为 0）。
        self.rule_count = compat::config_json_rule_count(&config_json);

        progress("同步代理策略");
        self.post_core_clash_setup().await;

        Ok(())
    }
}
