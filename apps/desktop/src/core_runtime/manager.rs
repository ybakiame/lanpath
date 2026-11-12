//! 核心进程管理器选择（`ActiveManager`）与 spawn/就绪辅助：复用共享
//! `lanpath-engine` 的两个进程管理器（pid 身份记录、Windows Job、Unix
//! 生命周期绑定），本模块只做 enum 分派与就绪等待/身份读回。

use std::path::Path;
use std::time::Duration;

use lanpath_common::{CoreType, LanpathError, LanpathResult, ProcessIdentity, ProcessStopKind};
use lanpath_engine::{CoreManager, MihomoProcessManager, SingBoxProcessManager};
use serde_json::Value;

use crate::config_composer::{ConfigPreview, PreviewFormat};

/// 本次启动持有的核心进程管理器（复用共享引擎的进程/pid 身份/Job/父子绑定）。
pub(super) enum ActiveManager {
    SingBox(SingBoxProcessManager),
    Mihomo(MihomoProcessManager),
}

impl ActiveManager {
    pub(super) fn new(
        core_type: CoreType,
        binary: &Path,
        config_dir: &Path,
    ) -> LanpathResult<Self> {
        match core_type {
            CoreType::SingBox => Ok(Self::SingBox(SingBoxProcessManager::new(
                binary, config_dir,
            )?)),
            CoreType::Mihomo => Ok(Self::Mihomo(MihomoProcessManager::new(binary, config_dir)?)),
        }
    }

    pub(super) async fn start(&self, config: &Value) -> LanpathResult<()> {
        match self {
            Self::SingBox(manager) => manager.start(config).await,
            Self::Mihomo(manager) => manager.start(config).await,
        }
    }

    pub(super) async fn is_running(&self) -> bool {
        match self {
            Self::SingBox(manager) => manager.is_running().await,
            Self::Mihomo(manager) => manager.is_running().await,
        }
    }

    pub(super) async fn stop_process(&self) -> LanpathResult<ProcessStopKind> {
        match self {
            Self::SingBox(manager) => manager.stop_process().await,
            Self::Mihomo(manager) => manager.stop_process().await,
        }
    }

    pub(super) async fn last_error(&self) -> String {
        let result = match self {
            Self::SingBox(manager) => manager.last_error().await,
            Self::Mihomo(manager) => manager.last_error().await,
        };
        result.unwrap_or_default()
    }
}

/// 从引擎写的 pid 文件读回本次子进程的 OS 身份（字段与引擎 lifecycle 的
/// PidFile 一致）。
pub(super) fn read_pid_identity(
    root: &Path,
    core_type: CoreType,
) -> LanpathResult<ProcessIdentity> {
    #[derive(serde::Deserialize)]
    struct PidRecord {
        identity: ProcessIdentity,
    }
    let name = match core_type {
        CoreType::SingBox => "sing-box.pid",
        CoreType::Mihomo => "mihomo.pid",
    };
    let path = root.join(name);
    let bytes = std::fs::read(&path).map_err(|error| {
        LanpathError::Core(format!("无法读取进程身份记录 {}:{error}", path.display()))
    })?;
    let record: PidRecord = serde_json::from_slice(&bytes).map_err(|error| {
        LanpathError::Core(format!("进程身份记录损坏 {}:{error}", path.display()))
    })?;
    Ok(record.identity)
}

/// 把合成预览解析回 `Value` 供引擎：sing-box 为 pretty JSON，Mihomo 为
/// YAML（serde_yaml 解析回 Value；合成已按同一快照完成，不重复联网）。
pub(super) fn parse_preview(preview: &ConfigPreview) -> LanpathResult<Value> {
    match preview.format() {
        PreviewFormat::Json => serde_json::from_str(preview.content())
            .map_err(|error| LanpathError::Core(format!("合成配置 JSON 解析失败:{error}"))),
        PreviewFormat::Yaml => serde_yaml::from_str(preview.content())
            .map_err(|error| LanpathError::Core(format!("合成配置 YAML 解析失败:{error}"))),
    }
}

/// 等待 mixed 端口真实监听；核心中途退出立即报错（带核心输出尾行）。
pub(super) async fn wait_listener(
    manager: &ActiveManager,
    port: u16,
    timeout: Duration,
) -> LanpathResult<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .is_ok()
        {
            return Ok(());
        }
        if !manager.is_running().await {
            let detail = manager.last_error().await;
            return Err(LanpathError::Core(if detail.is_empty() {
                "核心在监听就绪前已退出".into()
            } else {
                format!("核心在监听就绪前已退出：{detail}")
            }));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(LanpathError::Core(format!(
                "等待代理监听 {port} 超时（{} 秒）",
                timeout.as_secs()
            )));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
