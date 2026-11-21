//! 配置校验器测试：能力边界正/负用例（纯同步，直接驱动检查函数）+ 真实
//! 假核心二进制的 `check`/`-t` 校验（成功/失败/超时）。不依赖互联网。

use std::time::Duration;

use serde_json::{Value, json};

use super::*;
use crate::core_fixture;

fn bindings() -> ValidatorBindings {
    ValidatorBindings {
        mixed_port: 17899,
        clash_api_enabled: true,
        clash_api_port: 19090,
        clash_api_secret: String::new(),
    }
}

/// 带 logs/、runtime/sing-box 与 .cache/ 子目录的数据根（写路径收敛的正例需要父目录存在）。
fn root_with_dirs() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("logs")).unwrap();
    std::fs::create_dir_all(dir.path().join("runtime/sing-box")).unwrap();
    std::fs::create_dir_all(dir.path().join(".cache")).unwrap();
    dir
}

fn valid_singbox() -> Value {
    json!({
        "inbounds": [{
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": 17899
        }],
        "outbounds": [{"type": "direct"}],
        "experimental": {
            "clash_api": {
                "external_controller": "127.0.0.1:19090",
                "default_mode": "rule"
            }
        }
    })
}

fn valid_mihomo() -> Value {
    json!({
        "mixed-port": 17899,
        "allow-lan": false,
        "bind-address": "127.0.0.1",
        "mode": "rule",
        "external-controller": "127.0.0.1:19090",
        "proxies": []
    })
}

fn expect_denied<T: std::fmt::Debug>(result: LanpathResult<T>, needle: &str) {
    let error = result.expect_err("must be denied");
    let message = error.to_string();
    assert!(message.contains(needle), "期望含 {needle:?}：{message}");
}

// ---------- sing-box 能力边界 ----------

#[test]
fn singbox_valid_minimal_config_passes() {
    let dir = root_with_dirs();
    check_singbox(dir.path(), &valid_singbox(), &bindings()).expect("valid config passes");
}

#[test]
fn singbox_rejects_tun_and_extra_inbounds() {
    let dir = root_with_dirs();
    let mut config = valid_singbox();
    config["inbounds"] = json!([
        {"type": "mixed", "listen": "127.0.0.1", "listen_port": 17899},
        {"type": "tun", "interface_name": "tun0"}
    ]);
    expect_denied(check_singbox(dir.path(), &config, &bindings()), "inbounds");

    let mut config = valid_singbox();
    config["inbounds"] = json!([
        {"type": "mixed", "listen": "127.0.0.1", "listen_port": 17899},
        {"type": "mixed", "listen": "127.0.0.1", "listen_port": 17898}
    ]);
    expect_denied(check_singbox(dir.path(), &config, &bindings()), "inbounds");
}

#[test]
fn singbox_rejects_non_loopback_and_wrong_port() {
    let dir = root_with_dirs();
    let mut config = valid_singbox();
    config["inbounds"][0]["listen"] = json!("0.0.0.0");
    expect_denied(check_singbox(dir.path(), &config, &bindings()), "listen");
    let mut config = valid_singbox();
    config["inbounds"][0]["listen_port"] = json!(17890);
    expect_denied(
        check_singbox(dir.path(), &config, &bindings()),
        "listen_port",
    );
}

#[test]
fn singbox_rejects_set_system_proxy_true() {
    let dir = root_with_dirs();
    let mut config = valid_singbox();
    config["inbounds"][0]["set_system_proxy"] = json!(true);
    expect_denied(
        check_singbox(dir.path(), &config, &bindings()),
        "set_system_proxy",
    );
    config["inbounds"][0]["set_system_proxy"] = json!(false);
    check_singbox(dir.path(), &config, &bindings()).expect("false passes");
}

#[test]
fn singbox_clash_api_must_match_saved_loopback_endpoint() {
    let dir = root_with_dirs();
    // 启用但缺失
    let mut config = valid_singbox();
    config["experimental"] = json!({});
    expect_denied(check_singbox(dir.path(), &config, &bindings()), "clash_api");
    // 端口不匹配
    let mut config = valid_singbox();
    config["experimental"]["clash_api"]["external_controller"] = json!("127.0.0.1:9090");
    expect_denied(
        check_singbox(dir.path(), &config, &bindings()),
        "external_controller",
    );
    // 非 loopback
    let mut config = valid_singbox();
    config["experimental"]["clash_api"]["external_controller"] = json!("0.0.0.0:19090");
    expect_denied(
        check_singbox(dir.path(), &config, &bindings()),
        "external_controller",
    );
    // 密钥不匹配
    let mut config = valid_singbox();
    config["experimental"]["clash_api"]["secret"] = json!("wrong");
    expect_denied(check_singbox(dir.path(), &config, &bindings()), "secret");
    // 面板下载字段
    for field in ["external_ui", "external_ui_download_url"] {
        let mut config = valid_singbox();
        config["experimental"]["clash_api"][field] = json!("x");
        expect_denied(check_singbox(dir.path(), &config, &bindings()), field);
    }
    // 已保存密钥匹配时通过
    let mut with_secret = bindings();
    with_secret.clash_api_secret = "s3cret".into();
    let mut config = valid_singbox();
    config["experimental"]["clash_api"]["secret"] = json!("s3cret");
    check_singbox(dir.path(), &config, &with_secret).expect("matching secret passes");
    // 禁用时必须缺席
    let mut disabled = bindings();
    disabled.clash_api_enabled = false;
    let mut config = valid_singbox();
    config["experimental"] = json!({});
    check_singbox(dir.path(), &config, &disabled).expect("disabled without api passes");
    config["experimental"]["clash_api"] = json!({"external_controller": "127.0.0.1:19090"});
    expect_denied(check_singbox(dir.path(), &config, &disabled), "clash_api");
}

#[test]
fn singbox_resource_paths_are_confined_to_root() {
    let dir = root_with_dirs();
    let root = dir.path();
    // log.output 越出根
    let mut config = valid_singbox();
    config["log"] = json!({"output": "/etc/passwd"});
    expect_denied(check_singbox(root, &config, &bindings()), "log.output");
    // log.output 必须在 logs/ 内（不是根内任意位置）
    let mut config = valid_singbox();
    config["log"] = json!({"output": ".cache/evil.log"});
    expect_denied(check_singbox(root, &config, &bindings()), "log.output");
    // logs/ 内通过（文件尚不存在允许——核心自行创建）
    let mut config = valid_singbox();
    config["log"] = json!({"output": "logs/core.log"});
    check_singbox(root, &config, &bindings()).expect("logs output passes");
    // 绝对路径在根内 logs/ 下也通过
    let mut config = valid_singbox();
    config["log"] = json!({"output": root.join("logs/core.log").to_string_lossy()});
    check_singbox(root, &config, &bindings()).expect("absolute logs output passes");
    // cache_file 越界/越目录
    let mut config = valid_singbox();
    config["experimental"]["cache_file"] = json!({"enabled": true, "path": "logs/cache.db"});
    expect_denied(check_singbox(root, &config, &bindings()), "cache_file.path");
    let mut config = valid_singbox();
    config["experimental"]["cache_file"] =
        json!({"enabled": true, "path": "runtime/sing-box/cache.db"});
    check_singbox(root, &config, &bindings()).expect("runtime cache path passes");
    let mut config = valid_singbox();
    config["experimental"]["cache_file"] = json!({"enabled": true, "path": ".cache/core.db"});
    check_singbox(root, &config, &bindings()).expect("dot-cache path passes");
    // 本地 rule_set 越界与 `..`
    let mut config = valid_singbox();
    config["route"] = json!({"rule_set": [{"type": "local", "tag": "t", "format": "source", "path": "/etc/hosts"}]});
    expect_denied(
        check_singbox(root, &config, &bindings()),
        "rule_set[0].path",
    );
    let mut config = valid_singbox();
    config["route"] = json!({"rule_set": [{"type": "local", "tag": "t", "format": "source", "path": "../escape.srs"}]});
    expect_denied(
        check_singbox(root, &config, &bindings()),
        "rule_set[0].path",
    );
    let mut config = valid_singbox();
    config["route"] = json!({"rule_set": [{"type": "local", "tag": "t", "format": "source", "path": "rulesets/files/t.srs"}]});
    std::fs::create_dir_all(root.join("rulesets/files")).unwrap();
    check_singbox(root, &config, &bindings()).expect("confined rule_set passes");
    // certificate_path 越界
    let mut config = valid_singbox();
    config["certificate"] = json!({"certificate_path": ["/etc/ssl/cert.pem"]});
    expect_denied(
        check_singbox(root, &config, &bindings()),
        "certificate_path",
    );
    // 环境变量展开拒绝
    let mut config = valid_singbox();
    config["log"] = json!({"output": "logs/$USER.log"});
    expect_denied(check_singbox(root, &config, &bindings()), "log.output");
}

// ---------- Mihomo 能力边界 ----------

#[test]
fn mihomo_valid_subscription_config_passes() {
    check_mihomo(&valid_mihomo(), &bindings()).expect("valid mihomo passes");
}

#[test]
fn mihomo_rejects_mismatched_mixed_port() {
    let mut config = valid_mihomo();
    config["mixed-port"] = json!(17890);
    expect_denied(check_mihomo(&config, &bindings()), "mixed-port");
}

#[test]
fn mihomo_rejects_allow_lan_and_tun() {
    let mut config = valid_mihomo();
    config["allow-lan"] = json!(true);
    expect_denied(check_mihomo(&config, &bindings()), "allow-lan");
    let mut config = valid_mihomo();
    config["tun"] = json!({"enable": true, "stack": "mixed"});
    expect_denied(check_mihomo(&config, &bindings()), "tun.enable");
    let mut config = valid_mihomo();
    config["tun"] = json!({"enable": false});
    check_mihomo(&config, &bindings()).expect("disabled tun passes");
}

#[test]
fn mihomo_external_controller_must_be_saved_loopback() {
    let mut config = valid_mihomo();
    config["external-controller"] = json!("0.0.0.0:19090");
    expect_denied(check_mihomo(&config, &bindings()), "external-controller");
    let mut config = valid_mihomo();
    config["external-controller"] = json!("127.0.0.1:9090");
    expect_denied(check_mihomo(&config, &bindings()), "external-controller");
    let mut config = valid_mihomo();
    config
        .as_object_mut()
        .unwrap()
        .remove("external-controller");
    expect_denied(check_mihomo(&config, &bindings()), "external-controller");
    // 禁用时必须缺席
    let mut disabled = bindings();
    disabled.clash_api_enabled = false;
    let mut config = valid_mihomo();
    config
        .as_object_mut()
        .unwrap()
        .remove("external-controller");
    check_mihomo(&config, &disabled).expect("disabled without controller passes");
    config["external-controller"] = json!("127.0.0.1:19090");
    expect_denied(check_mihomo(&config, &disabled), "external-controller");
}

#[test]
fn mihomo_rejects_dashboard_download_fields() {
    for field in ["external-ui", "external-ui-url"] {
        let mut config = valid_mihomo();
        config[field] = json!("x");
        expect_denied(check_mihomo(&config, &bindings()), field);
    }
}

// ---------- 真实二进制校验（fixture 核心） ----------

/// 正例：fixture `check` 退出 0，validate_value 通过。
#[tokio::test]
async fn real_binary_check_passes_for_valid_config() {
    let dir = root_with_dirs();
    let validator = ConfigValidator::new(
        dir.path(),
        core_fixture::compiled_core(),
        CoreType::SingBox,
        bindings(),
    );
    validator
        .validate_value(&valid_singbox())
        .await
        .expect("fixture check passes");
}

/// 反例：fixture `check` 以 stderr + 退出 1 报告，错误带真实诊断。
#[tokio::test]
async fn real_binary_check_failure_carries_core_diagnostics() {
    let dir = root_with_dirs();
    let validator = ConfigValidator::new(
        dir.path(),
        core_fixture::compiled_core(),
        CoreType::SingBox,
        bindings(),
    );
    let mut config = valid_singbox();
    config["fixture_fail"] = json!("sing-box says no");
    let error = validator
        .validate_value(&config)
        .await
        .expect_err("fixture check fails");
    assert!(error.to_string().contains("sing-box says no"), "{error}");
}

/// 超时：fixture 停滞，校验 1 秒超时强杀并回收。
#[tokio::test]
async fn real_binary_check_timeout_kills_and_reaps() {
    let dir = root_with_dirs();
    let validator = ConfigValidator::new(
        dir.path(),
        core_fixture::compiled_core(),
        CoreType::SingBox,
        bindings(),
    )
    .with_check_timeout(Duration::from_secs(1));
    let mut config = valid_singbox();
    config["fixture_sleep_secs"] = json!(30);
    let started = std::time::Instant::now();
    let error = validator
        .validate_value(&config)
        .await
        .expect_err("timeout");
    assert!(error.to_string().contains("超时"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

/// Mihomo `-t` 校验真实执行（YAML 序列化后由 fixture 解析）。
#[tokio::test]
async fn mihomo_real_binary_check_runs_yaml() {
    let dir = root_with_dirs();
    let validator = ConfigValidator::new(
        dir.path(),
        core_fixture::compiled_core(),
        CoreType::Mihomo,
        bindings(),
    );
    validator
        .validate_value(&valid_mihomo())
        .await
        .expect("fixture -t passes");
    let mut config = valid_mihomo();
    config["fixture_fail"] = json!("mihomo says no");
    let error = validator
        .validate_value(&config)
        .await
        .expect_err("fixture -t fails");
    assert!(error.to_string().contains("mihomo says no"), "{error}");
}

/// 输出路径不得覆盖选中的核心可执行文件本身。
#[tokio::test]
async fn output_path_must_not_overwrite_the_selected_binary() {
    let dir = root_with_dirs();
    let binary = dir.path().join("logs").join("core-under-test");
    std::fs::copy(core_fixture::compiled_core(), &binary).expect("copy fixture");
    let validator = ConfigValidator::new(dir.path(), &binary, CoreType::SingBox, bindings());
    let mut config = valid_singbox();
    config["log"] = json!({"output": binary.to_string_lossy()});
    let error = validator
        .validate_value(&config)
        .await
        .expect_err("overwrite denied");
    assert!(error.to_string().contains("不得覆盖"), "{error}");
}

/// 相对路径分隔/冒号拒绝（Windows 设备与 ADS 形态）。
#[test]
fn confined_rejects_windows_style_relative_paths() {
    let dir = root_with_dirs();
    expect_denied(
        write_confined(dir.path(), "logs\\evil.log", "log.output", "logs"),
        "log.output",
    );
    expect_denied(
        write_confined(dir.path(), "logs/evil.log:ads", "log.output", "logs"),
        "log.output",
    );
    expect_denied(
        write_confined(dir.path(), "CON", "log.output", "logs"),
        "log.output",
    );
}

/// 编译期保证：校验器可跨线程移动到 NSH background。
#[allow(dead_code)]
fn _assert_validator_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<ConfigValidator>();
}
