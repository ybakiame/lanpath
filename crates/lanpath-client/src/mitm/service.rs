use std::{path::PathBuf, sync::Arc};

use lanpath_common::{LanpathError, LanpathResult};
use lanpath_mitm::{
    CaMaterial, CaStore, FileCaStore, MemoryRecorder, RewriteEngine, RunningProxy,
    ScriptHookEngine, TrafficRecorder,
};
pub use lanpath_mitm::{CaTrustStatus, CaTrustView, TrafficRecord};
use lanpath_script::{FilePersistentStore, Notifier, ScriptHost, ScriptLimits};
use serde::Serialize;

use super::{MitmBuildOptions, build_mitm_proxy};
use crate::{
    ClientConfig,
    core_config::MitmChain,
    http_exec::ReqwestHttpExecutor,
    remote::{RemoteManager, apply_argument_templates, collect_hostname_sources},
};

const TRAFFIC_CAPACITY: usize = 2048;

#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct MitmCaView {
    pub path: String,
    pub pem: String,
}

#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct DerivedHostnameView {
    pub hostname: String,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[non_exhaustive]
pub struct MitmWhitelistView {
    pub derived: Vec<DerivedHostnameView>,
    pub manual: Vec<String>,
    pub excluded: Vec<String>,
}

impl MitmWhitelistView {
    pub fn has_inclusions(&self) -> bool {
        !self.manual.is_empty() || !self.derived.is_empty()
    }
}

#[derive(Clone)]
pub struct MitmService {
    data_dir: PathBuf,
    recorder: Arc<MemoryRecorder>,
}

impl MitmService {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            recorder: Arc::new(MemoryRecorder::new(TRAFFIC_CAPACITY)),
        }
    }

    pub fn traffic(&self) -> Vec<TrafficRecord> {
        self.recorder.list()
    }

    pub fn whitelist(&self, config: &ClientConfig) -> MitmWhitelistView {
        let (excluded, manual) = config
            .mitm
            .hostnames
            .iter()
            .cloned()
            .partition(|hostname| hostname.starts_with('-') || hostname.starts_with('!'));
        let derived = collect_hostname_sources(&self.data_dir)
            .into_iter()
            .map(|entry| DerivedHostnameView {
                hostname: entry.hostname,
                sources: entry.sources,
            })
            .collect();
        MitmWhitelistView {
            derived,
            manual,
            excluded,
        }
    }

    pub fn ca(&self) -> LanpathResult<MitmCaView> {
        let material = self.ca_material()?;
        Ok(MitmCaView {
            path: self.ca_path().to_string_lossy().into_owned(),
            pem: material.cert_pem,
        })
    }

    pub fn trust_status(&self) -> LanpathResult<CaTrustView> {
        self.ca_material()?;
        Ok(lanpath_mitm::ca_trust_status(&self.ca_path()))
    }

    pub fn install_ca(&self) -> LanpathResult<CaTrustView> {
        self.ca_material()?;
        lanpath_mitm::install_ca(&self.ca_path())?;
        Ok(lanpath_mitm::ca_trust_status(&self.ca_path()))
    }

    pub async fn start(
        &self,
        config: &ClientConfig,
        notifier: Arc<dyn Notifier>,
    ) -> LanpathResult<Option<MitmSession>> {
        if !config.mitm_enabled {
            return Ok(None);
        }
        if config.data_dir != self.data_dir {
            return Err(LanpathError::Config(
                "MITM 服务数据目录与客户端配置不一致".into(),
            ));
        }

        let remote = RemoteManager::new(self.data_dir.clone());
        let merged = remote.load_cached()?;
        let remotes = remote.load().unwrap_or_default();
        let hook_rules = apply_argument_templates(merged.scripts, &merged.metas, &remotes);
        let mut hostnames = config.mitm.hostnames.clone();
        for hostname in merged.hostnames {
            if !hostnames.contains(&hostname) {
                hostnames.push(hostname);
            }
        }
        if !has_effective_whitelist(&hostnames) {
            tracing::info!("MITM enabled but whitelist is empty, skipping MITM chain");
            return Ok(None);
        }

        let host = Arc::new(ScriptHost::new(
            Arc::new(ReqwestHttpExecutor::new()),
            Arc::new(FilePersistentStore::new(self.data_dir.join("script_store"))),
            notifier,
        ));
        let hooks = ScriptHookEngine::new(
            host,
            config.mitm.script_dialect,
            ScriptLimits::default(),
            hook_rules,
        );
        let return_port = config
            .mixed_port
            .checked_add(1)
            .ok_or_else(|| LanpathError::Config("MITM 回流端口超出有效范围".into()))?;
        let options = MitmBuildOptions {
            extra_hostnames: hostnames.clone(),
            upstream_port: Some(return_port),
            rewrite: RewriteEngine {
                rules: merged.rewrites,
            },
            hooks: Some(hooks),
        };
        let recorder: Arc<dyn TrafficRecorder> = self.recorder.clone();
        let running = build_mitm_proxy(config, options, recorder)?.start().await?;
        let chain = MitmChain {
            proxy_addr: running.addr,
            return_port,
            hostnames,
        };
        Ok(Some(MitmSession {
            running: Some(running),
            chain,
        }))
    }

    fn ca_material(&self) -> LanpathResult<CaMaterial> {
        FileCaStore::new(self.data_dir.join("certs")).load_or_generate()
    }

    fn ca_path(&self) -> PathBuf {
        self.data_dir.join("certs").join("ca.crt")
    }
}

pub struct MitmSession {
    running: Option<RunningProxy>,
    chain: MitmChain,
}

impl MitmSession {
    pub fn chain(&self) -> MitmChain {
        MitmChain {
            proxy_addr: self.chain.proxy_addr,
            return_port: self.chain.return_port,
            hostnames: self.chain.hostnames.clone(),
        }
    }

    pub fn addr(&self) -> std::net::SocketAddr {
        self.chain.proxy_addr
    }

    pub fn stop(mut self) {
        if let Some(running) = self.running.take() {
            running.shutdown();
        }
    }
}

impl Drop for MitmSession {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.shutdown();
        }
    }
}

pub fn has_effective_whitelist(hostnames: &[String]) -> bool {
    hostnames
        .iter()
        .any(|hostname| !hostname.starts_with('-') && !hostname.starts_with('!'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_requires_an_inclusion() {
        assert!(!has_effective_whitelist(&[]));
        assert!(!has_effective_whitelist(&[
            "-example.com".into(),
            "!example.net".into(),
        ]));
        assert!(has_effective_whitelist(&[
            "-example.com".into(),
            "*.example.net".into(),
        ]));
    }

    #[test]
    fn ca_and_whitelist_use_only_the_selected_data_root() {
        let root = tempfile::tempdir().unwrap();
        let service = MitmService::new(root.path());
        let mut config = ClientConfig::new(
            root.path().to_path_buf(),
            String::new(),
            String::new(),
            PathBuf::new(),
        );
        config.mitm.hostnames = vec!["example.com".into(), "-excluded.example".into()];

        let ca = service.ca().unwrap();
        assert!(PathBuf::from(ca.path).starts_with(root.path()));
        let whitelist = service.whitelist(&config);
        assert_eq!(whitelist.manual, ["example.com"]);
        assert_eq!(whitelist.excluded, ["-excluded.example"]);
        assert!(whitelist.derived.is_empty());
        assert!(whitelist.has_inclusions());
    }

    #[tokio::test]
    async fn starts_only_for_an_enabled_effective_whitelist() {
        let root = tempfile::tempdir().unwrap();
        let service = MitmService::new(root.path());
        let mut config = ClientConfig::new(
            root.path().to_path_buf(),
            String::new(),
            String::new(),
            PathBuf::new(),
        );
        let notifier = Arc::new(lanpath_script::RecordingNotifier::new());

        assert!(
            service
                .start(&config, notifier.clone())
                .await
                .unwrap()
                .is_none()
        );
        config.mitm_enabled = true;
        config.mitm.hostnames = vec!["-excluded.example".into()];
        assert!(
            service
                .start(&config, notifier.clone())
                .await
                .unwrap()
                .is_none()
        );

        config.mitm.hostnames.push("api.example.com".into());
        let session = service
            .start(&config, notifier)
            .await
            .unwrap()
            .expect("effective whitelist should start MITM");
        assert!(session.addr().ip().is_loopback());
        assert_eq!(session.chain().return_port, config.mixed_port + 1);
        assert_eq!(
            session.chain().hostnames,
            ["-excluded.example", "api.example.com"]
        );
        session.stop();
    }
}
