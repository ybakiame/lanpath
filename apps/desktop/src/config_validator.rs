//! 核心配置校验（启动前置条件）：真实核心二进制 schema 校验 + 本切片能力边界。
//!
//! 真实校验以选中的库存核心执行——sing-box `check -c <临时文件> -D <根>`、
//! Mihomo `-t -d <根> -f <临时文件>`；临时文件建在本数据根内（核心 `-D`/`-d`
//! 同样固定为本根），`no_window` 抑制 Windows 控制台窗口，stdout/stderr 并行
//! drain（各保留最多 32 KiB 诊断），20 秒超时强杀并回收（测试可注入更短超时）。
//!
//! 能力边界（不搬 GPUI 全量字段白名单，只守本切片安全语义；未知字段交给
//! 核心 schema 判定）：
//!
//! - sing-box：有且仅有一个 `mixed` inbound，`listen` 必须为 `127.0.0.1`、
//!   `listen_port` 必须等于已保存 mixed 端口；拒绝任何 tun inbound 与
//!   `set_system_proxy: true`；`experimental.clash_api` 在启用时必须匹配已保存
//!   loopback API 端口与密钥，禁用时必须缺席，且不得携带 `external_ui` /
//!   `external_ui_download_url` 面板下载字段；`log.output`、
//!   `experimental.cache_file.path` 写路径分别限制在根内 `logs/`、`cache/`，
//!   `route.rule_set` 本地资源与 `certificate.certificate_path` 读路径限制在
//!   根内（拒绝链接/junction/硬链接与 `$` 环境变量展开）。
//! - Mihomo：`mixed-port` 必须等于已保存 mixed 端口；`allow-lan` 只允许
//!   false/缺席；`tun.enable` 只允许 false/缺席；`external-controller` 只允许
//!   loopback（启用时必须匹配已保存端口，禁用时必须缺席）；顶层
//!   `external-ui` / `external-ui-url` 面板下载字段必须不存在。
//!
//! 日志不记录配置正文；校验失败只把核心 stderr/stdout 诊断（有界）带回错误。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use lanpath_common::{CoreType, LanpathError, LanpathResult};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

/// 校验子进程默认超时：核心的 `check`/`-t` 只做本地解析，20 秒足够覆盖
/// 冷启动；超时按校验失败处理并强杀回收。
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
/// stdout/stderr 各保留的有界诊断上限（超出部分仍 drain，只是不保留）。
const DIAGNOSTIC_LIMIT: usize = 32 * 1024;

/// 本次启动固定的端口/API 绑定（与合成配置同一份 ClientConfig 快照）。
#[derive(Debug, Clone)]
pub(crate) struct ValidatorBindings {
    pub(crate) mixed_port: u16,
    pub(crate) clash_api_enabled: bool,
    pub(crate) clash_api_port: u16,
    pub(crate) clash_api_secret: String,
}

/// 配置校验器：数据根 + 选中的库存核心 + 快照绑定。
pub(crate) struct ConfigValidator {
    root: PathBuf,
    binary: PathBuf,
    core_type: CoreType,
    bindings: ValidatorBindings,
    #[cfg(test)]
    check_timeout: Option<Duration>,
}

impl ConfigValidator {
    pub(crate) fn new(
        root: &Path,
        binary: &Path,
        core_type: CoreType,
        bindings: ValidatorBindings,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            binary: binary.to_path_buf(),
            core_type,
            bindings,
            #[cfg(test)]
            check_timeout: None,
        }
    }

    /// 测试注入更短的校验超时（超时用例不真的等 20 秒）。
    #[cfg(test)]
    pub(crate) fn with_check_timeout(self, timeout: Duration) -> Self {
        Self {
            check_timeout: Some(timeout),
            ..self
        }
    }

    fn timeout(&self) -> Duration {
        #[cfg(test)]
        if let Some(timeout) = self.check_timeout {
            return timeout;
        }
        CHECK_TIMEOUT
    }

    /// 能力边界 + 真实核心二进制校验；全部通过才允许启动。
    pub(crate) async fn validate_value(&self, value: &Value) -> LanpathResult<()> {
        match self.core_type {
            CoreType::SingBox => check_singbox(&self.root, value, &self.bindings)?,
            CoreType::Mihomo => check_mihomo(value, &self.bindings)?,
        }
        if !self.binary.is_absolute() || !self.root.is_absolute() {
            return Err(denied(
                "core",
                "校验与启动都要求绝对路径的核心二进制与数据根",
            ));
        }
        let canonical_binary = std::fs::canonicalize(&self.binary).map_err(|error| {
            LanpathError::Core(format!(
                "无法访问选中的 {} 核心 {}:{error}",
                self.core_type,
                self.binary.display()
            ))
        })?;
        if self.core_type == CoreType::SingBox {
            // 输出/缓存路径不得覆盖选中的核心可执行文件本身。
            for (pointer, location) in [
                ("/log/output", "log.output"),
                (
                    "/experimental/cache_file/path",
                    "experimental.cache_file.path",
                ),
            ] {
                if let Some(path) = value
                    .pointer(pointer)
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
                    && confined(&self.root, path, location)? == canonical_binary
                {
                    return Err(denied(location, "输出路径不得覆盖选中的核心可执行文件"));
                }
            }
        }
        let text = match self.core_type {
            CoreType::SingBox => serde_json::to_string_pretty(value)?,
            CoreType::Mihomo => serde_yaml::to_string(value)?,
        };
        self.check_core(&text).await
    }

    /// 真实核心二进制校验：临时文件落在本根内，核心工作目录固定为本根。
    async fn check_core(&self, text: &str) -> LanpathResult<()> {
        let suffix = match self.core_type {
            CoreType::SingBox => ".json",
            CoreType::Mihomo => ".yaml",
        };
        let mut candidate = tempfile::Builder::new()
            .prefix(".core-check-")
            .suffix(suffix)
            .tempfile_in(&self.root)?;
        candidate.write_all(text.as_bytes())?;
        candidate.as_file().sync_all()?;
        let mut command = Command::new(&self.binary);
        match self.core_type {
            CoreType::SingBox => {
                command
                    .arg("check")
                    .arg("-c")
                    .arg(candidate.path())
                    .arg("-D")
                    .arg(&self.root);
            }
            CoreType::Mihomo => {
                command
                    .arg("-t")
                    .arg("-d")
                    .arg(&self.root)
                    .arg("-f")
                    .arg(candidate.path());
            }
        }
        command
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = lanpath_common::no_window_tokio(&mut command)
            .spawn()
            .map_err(|error| {
                LanpathError::Core(format!(
                    "无法启动选中的 {} 校验进程:{error}",
                    self.core_type
                ))
            })?;
        let (Some(output), Some(errors)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(LanpathError::Core("校验进程诊断管道不可用".into()));
        };
        let mut stdout = tokio::spawn(drain(output));
        let mut stderr = tokio::spawn(drain(errors));
        let result = tokio::time::timeout(self.timeout(), async {
            let status = child.wait().await?;
            let out = (&mut stdout).await.map_err(std::io::Error::other)??;
            let err = (&mut stderr).await.map_err(std::io::Error::other)??;
            Ok::<_, std::io::Error>((status, out, err))
        })
        .await;
        match result {
            Ok(Ok((status, _, _))) if status.success() => Ok(()),
            Ok(Ok((status, out, err))) => Err(LanpathError::Core(format!(
                "{} 校验失败（{status}）:\n{}\n{}",
                self.core_type,
                String::from_utf8_lossy(&err),
                String::from_utf8_lossy(&out)
            ))),
            Ok(Err(error)) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                stdout.abort();
                stderr.abort();
                Err(LanpathError::Core(format!(
                    "{} 校验未能完成:{error}",
                    self.core_type
                )))
            }
            Err(_) => {
                let killed = child.kill().await;
                let reaped = child.wait().await;
                stdout.abort();
                stderr.abort();
                Err(LanpathError::Core(format!(
                    "{} 校验超时（{} 秒，kill: {killed:?}, reap: {reaped:?}）",
                    self.core_type,
                    self.timeout().as_secs()
                )))
            }
        }
    }
}

/// drain 两个管道避免死锁，但只保留有界诊断。
async fn drain(mut stream: impl AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut diagnostic = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Ok(diagnostic);
        }
        let retained = count.min(DIAGNOSTIC_LIMIT.saturating_sub(diagnostic.len()));
        diagnostic.extend_from_slice(&buffer[..retained]);
    }
}

fn denied(location: &str, reason: &str) -> LanpathError {
    LanpathError::Config(format!("{location}: {reason}"))
}

// ---------- 能力边界 ----------

fn check_singbox(root: &Path, value: &Value, bindings: &ValidatorBindings) -> LanpathResult<()> {
    let obj = value
        .as_object()
        .ok_or_else(|| denied("$", "sing-box 配置必须是 JSON 对象"))?;
    let inbounds = obj
        .get("inbounds")
        .and_then(Value::as_array)
        .ok_or_else(|| denied("inbounds", "必须恰有一个受管 mixed inbound"))?;
    if inbounds.len() != 1 {
        return Err(denied(
            "inbounds",
            "本切片只允许一个已保存端口的 loopback mixed inbound",
        ));
    }
    let inbound = &inbounds[0];
    if inbound.get("type").and_then(Value::as_str) != Some("mixed") {
        return Err(denied(
            "inbounds[0].type",
            "只允许 mixed inbound（拒绝 tun 等）",
        ));
    }
    if inbound.get("listen").and_then(Value::as_str) != Some("127.0.0.1") {
        return Err(denied("inbounds[0].listen", "必须显式监听 127.0.0.1"));
    }
    if inbound.get("listen_port").and_then(Value::as_u64) != Some(u64::from(bindings.mixed_port)) {
        return Err(denied(
            "inbounds[0].listen_port",
            "必须等于已保存的 mixed 端口",
        ));
    }
    if inbound
        .get("set_system_proxy")
        .is_some_and(|value| value != &Value::Bool(false))
    {
        return Err(denied(
            "inbounds[0].set_system_proxy",
            "本切片未授权系统代理副作用，只允许 false 或缺席",
        ));
    }
    check_singbox_clash_api(obj.get("experimental"), bindings)?;
    if let Some(log) = obj.get("log") {
        let log = log
            .as_object()
            .ok_or_else(|| denied("log", "log 必须是对象"))?;
        if let Some(output) = log.get("output").and_then(Value::as_str)
            && !output.is_empty()
        {
            write_confined(root, output, "log.output", "logs")?;
        }
    }
    if let Some(cache) = value
        .pointer("/experimental/cache_file")
        .and_then(Value::as_object)
        && let Some(path) = cache.get("path").and_then(Value::as_str)
        && !path.is_empty()
    {
        write_confined(root, path, "experimental.cache_file.path", "cache")?;
    }
    if let Some(sets) = value.pointer("/route/rule_set").and_then(Value::as_array) {
        for (index, set) in sets.iter().enumerate() {
            let location = format!("route.rule_set[{index}]");
            let kind = set.get("type").and_then(Value::as_str).unwrap_or("inline");
            if kind == "remote" {
                continue;
            }
            for field in ["path", "initial_path"] {
                if let Some(path) = set.get(field).and_then(Value::as_str) {
                    confined(root, path, &format!("{location}.{field}"))?;
                }
            }
        }
    }
    if let Some(paths) = value
        .pointer("/certificate/certificate_path")
        .and_then(Value::as_array)
    {
        for (index, path) in paths.iter().enumerate() {
            if let Some(path) = path.as_str() {
                confined(
                    root,
                    path,
                    &format!("certificate.certificate_path[{index}]"),
                )?;
            }
        }
    }
    Ok(())
}

fn check_singbox_clash_api(
    experimental: Option<&Value>,
    bindings: &ValidatorBindings,
) -> LanpathResult<()> {
    let clash = experimental.and_then(|exp| exp.get("clash_api"));
    match (bindings.clash_api_enabled, clash) {
        (true, None) => {
            return Err(denied(
                "experimental.clash_api",
                "已保存设置启用了 Clash API，配置必须携带匹配的 loopback 端点",
            ));
        }
        (false, Some(_)) => {
            return Err(denied(
                "experimental.clash_api",
                "已保存设置关闭了 Clash API，配置不得携带该端点",
            ));
        }
        (true, Some(clash)) => {
            let clash = clash
                .as_object()
                .ok_or_else(|| denied("experimental.clash_api", "必须是对象"))?;
            let expected = format!("127.0.0.1:{}", bindings.clash_api_port);
            if clash.get("external_controller").and_then(Value::as_str) != Some(&expected) {
                return Err(denied(
                    "experimental.clash_api.external_controller",
                    "必须等于已保存的 loopback Clash API 端口",
                ));
            }
            let secret = clash
                .get("secret")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if secret != bindings.clash_api_secret {
                return Err(denied(
                    "experimental.clash_api.secret",
                    "必须与已保存的 Clash API 密钥一致",
                ));
            }
            for field in ["external_ui", "external_ui_download_url"] {
                if clash.contains_key(field) {
                    return Err(denied(
                        &format!("experimental.clash_api.{field}"),
                        "面板目录/下载字段不允许（避免核心联网下载面板）",
                    ));
                }
            }
        }
        (false, None) => {}
    }
    Ok(())
}

fn check_mihomo(value: &Value, bindings: &ValidatorBindings) -> LanpathResult<()> {
    let obj = value
        .as_object()
        .ok_or_else(|| denied("$", "Mihomo 配置必须是对象"))?;
    if obj.get("mixed-port").and_then(Value::as_u64) != Some(u64::from(bindings.mixed_port)) {
        return Err(denied("mixed-port", "必须等于已保存的 mixed 端口"));
    }
    if obj
        .get("allow-lan")
        .is_some_and(|v| v != &Value::Bool(false))
    {
        return Err(denied("allow-lan", "本切片只允许 false 或缺席"));
    }
    if let Some(tun) = obj.get("tun") {
        let enabled = tun
            .as_object()
            .and_then(|tun| tun.get("enable"))
            .is_some_and(|v| v != &Value::Bool(false));
        if enabled {
            return Err(denied("tun.enable", "本切片未授权 TUN"));
        }
    }
    for field in ["external-ui", "external-ui-url"] {
        if obj.contains_key(field) {
            return Err(denied(
                field,
                "面板目录/下载字段不允许（避免核心联网下载面板）",
            ));
        }
    }
    match (bindings.clash_api_enabled, obj.get("external-controller")) {
        (true, None) => {
            return Err(denied(
                "external-controller",
                "已保存设置启用了 Clash API，配置必须携带匹配的 loopback 端点",
            ));
        }
        (false, Some(_)) => {
            return Err(denied(
                "external-controller",
                "已保存设置关闭了 Clash API，配置不得携带该端点",
            ));
        }
        (true, Some(controller)) => {
            let expected = format!("127.0.0.1:{}", bindings.clash_api_port);
            if controller.as_str() != Some(&expected) {
                return Err(denied(
                    "external-controller",
                    "必须等于已保存的 loopback Clash API 端口",
                ));
            }
        }
        (false, None) => {}
    }
    Ok(())
}

// ---------- 资源路径收敛（限制在数据根内） ----------

mod paths;

pub(crate) use paths::{confined, write_confined};

#[cfg(test)]
#[path = "config_validator_tests.rs"]
mod tests;
