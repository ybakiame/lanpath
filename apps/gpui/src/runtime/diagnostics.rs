use lanpath_client::{
    config_slices::ConfigSlicesStore,
    diagnose::{
        DiagReport, DiagStatus, DiagnoseInput, diagnose_connectivity, effective_dns_servers,
    },
};

use super::DesktopRuntime;

impl DesktopRuntime {
    /// Called on the background executor, never from inside `block_on`.
    pub(crate) fn diagnose(&self, domain: Option<String>) -> Result<DiagReport, String> {
        let business = self.business()?;
        let config = business
            .client_config()
            .map_err(|error| error.to_string())?;
        let slices = ConfigSlicesStore::new(business.data_dir().to_path_buf())
            .load()
            .map_err(|error| format!("读取配置切片失败：{error}"))?;
        // Both accessors may enter Tokio themselves. Collect them before the
        // diagnostic future so there is no nested Runtime::block_on.
        let core_running = self.running()?;
        let missing_rule_sets = self.missing_rule_sets()?;
        let missing = missing_rule_sets.join("、");
        let domain = domain
            .as_deref()
            .map(str::trim)
            .filter(|domain| !domain.is_empty())
            .unwrap_or("google.com")
            .to_owned();
        let mut report = self.block_on(diagnose_connectivity(DiagnoseInput {
            domain,
            mixed_port: config.mixed_port,
            core_running,
            missing_rule_sets,
            rule_mode: config.normalized_rule_mode().to_owned(),
            clash_api: config
                .clash_api_enabled
                .then_some((config.clash_api_port, config.clash_api_secret)),
            dns_servers: effective_dns_servers(&slices, config.ipv6_enabled),
        }))?;
        for step in &mut report.steps {
            match step.key.as_str() {
                "tun-note" => {
                    step.summary = "实验版未启用或探测 TUN".into();
                    step.detail = "本次诊断仅检查系统 DNS、直连与实验核心的显式 mixed/API 路径；不会启用 TUN、设置系统代理或修改系统网络。本报告不能证明系统隧道或其他应用流量正常。".into();
                }
                "core-status" if step.status == DiagStatus::Fail => {
                    if core_running {
                        step.summary = "核心运行中，但引用的本地规则集文件缺失".into();
                        step.detail = format!(
                            "缺失：{missing}。核心可能仍持有已加载到内存的规则，不能据此断定当前全部路由已降级。本页不会自动补齐或热应用规则集；请停止核心，检查规则并重新生成预览后再启动。"
                        );
                    } else {
                        step.detail = "实验核心未运行；核心 mixed 入站与出站延迟步骤会跳过。系统 DNS 和直连基线仍会真实执行，不代表系统隧道状态。".into();
                    }
                }
                _ => {}
            }
        }
        Ok(report)
    }
}
