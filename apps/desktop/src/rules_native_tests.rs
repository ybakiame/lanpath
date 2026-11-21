//! 可选的 Windows 真实核心校验（显式传入测试二进制，普通测试跳过）。
use super::*;
use serde_json::json;
use std::process::Command;

#[tokio::test]
#[ignore = "set LANPATH_TEST_SINGBOX and LANPATH_TEST_MIHOMO to official test binaries"]
async fn quick_rules_and_typescript_pass_both_real_core_validators() {
    let singbox = std::env::var_os("LANPATH_TEST_SINGBOX").expect("test sing-box path");
    let mihomo = std::env::var_os("LANPATH_TEST_MIHOMO").expect("test Mihomo path");
    let root = tempfile::tempdir().unwrap();
    let rules=parse_lines("domain example.com -> direct\nsuffix example.org -> proxy\nkeyword ads -> reject\ncidr 10.0.0.0/8 -> direct\ncidr 2001:db8::/32 -> direct\nport 443 -> proxy\nport 8000-9000 -> reject\nprocess not-running-test.exe -> direct").unwrap();
    let js = crate::ts_override::compile("function main(config: any) { return config; }").unwrap();
    let mut sb = json!({"inbounds":[],"outbounds":[{"tag":"direct","type":"direct"},{"tag":"proxy","type":"direct"}],"route":{"rules":[],"final":"direct"}});
    lanpath_client::apply_singbox_rules(&mut sb, &rules);
    normalize_singbox_ports(&mut sb);
    sb = lanpath_client::profile::apply_js_override(sb, &js)
        .await
        .unwrap();
    let path = root.path().join("sing-box.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&sb).unwrap()).unwrap();
    let output = Command::new(singbox)
        .arg("check")
        .arg("-c")
        .arg(&path)
        .arg("-D")
        .arg(root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut mh = json!({"mixed-port":0,"allow-lan":false,"proxies":[],"proxy-groups":[{"name":"proxy","type":"select","proxies":["DIRECT"]}],"rules":["MATCH,DIRECT"]});
    assert!(apply_mihomo_rules(&mut mh, &rules).is_empty());
    mh = lanpath_client::profile::apply_js_override(mh, &js)
        .await
        .unwrap();
    let path = root.path().join("mihomo.yaml");
    std::fs::write(&path, serde_yaml::to_string(&mh).unwrap()).unwrap();
    let output = Command::new(mihomo)
        .arg("-t")
        .arg("-d")
        .arg(root.path())
        .arg("-f")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
