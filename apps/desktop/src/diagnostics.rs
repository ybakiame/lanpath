//! 只读诊断：固定运行上下文，不启动/修改核心与系统代理。
use crate::{
    core_runtime::CoreRuntime,
    data_root::{open_root, resolve_data_location},
    request_settings::load_request_config,
};
use lanpath_client::{
    ClientConfig,
    config_slices::ConfigSlicesStore,
    diagnose::{DiagReport, DiagnoseInput, diagnose_connectivity},
};
use lanpath_common::{LanpathError, LanpathResult};
use std::path::Path;

pub(crate) fn normalize_target(value: &str) -> LanpathResult<String> {
    crate::rules::syntax::normalize_domain(value).map_err(LanpathError::Validation)
}
pub(crate) async fn run(target: String) -> LanpathResult<DiagReport> {
    let target = normalize_target(&target)?;
    let root = open_root(&resolve_data_location()?.path)?;
    let _activity = root.activity().read().await;
    let config = {
        let _guard = root.business_guard();
        load_request_config(root.path())?
    };
    let runtime = CoreRuntime::for_root(root.path())?;
    let status = runtime.status().await;
    let mut input = input(root.path(), &config, target)?;
    // 本轮只探测运行会话实际端口。已保存但未重启的端口不冒充运行值。
    if status.running
        && let Some((running, missing)) = runtime.diagnostic_context()
    {
        input.core_running = true;
        input.mixed_port = running.mixed_port;
        input.missing_rule_sets = missing;
        input.rule_mode = running.normalized_rule_mode().into();
        input.clash_api = running
            .clash_api_enabled
            .then_some((running.clash_api_port, running.clash_api_secret));
    }
    let mut report = diagnose_connectivity(input).await;
    report.steps.retain(|step| step.key != "tun-note");
    // DNS 服务器来自已保存切片；不是对运行中核心配置的读取。
    if let Some(step) = report
        .steps
        .iter_mut()
        .find(|step| step.key == "dns-servers")
    {
        step.name = "已保存 DNS 上游探测".into();
        step.detail = format!(
            "已保存设置的独立探测，不代表运行核心当前 DNS 配置。\n{}",
            step.detail
        );
    }
    report.failed_steps = report
        .steps
        .iter()
        .filter(|s| s.status == lanpath_client::diagnose::DiagStatus::Fail)
        .count();
    Ok(report)
}
fn input(root: &Path, config: &ClientConfig, target: String) -> LanpathResult<DiagnoseInput> {
    let slices = ConfigSlicesStore::new(root.to_path_buf()).load()?;
    Ok(DiagnoseInput {
        domain: target,
        mixed_port: config.mixed_port,
        core_running: false,
        missing_rule_sets: vec![],
        rule_mode: "unknown".into(),
        clash_api: None,
        dns_servers: lanpath_client::diagnose::effective_dns_servers(&slices, config.ipv6_enabled),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_is_not_an_arbitrary_url() {
        assert_eq!(normalize_target(" Example.COM. ").unwrap(), "example.com");
        for bad in [
            "https://example.com/a",
            "example.com:443",
            "a b",
            "",
            "a..com",
        ] {
            assert!(normalize_target(bad).is_err(), "{bad}");
        }
    }
}
