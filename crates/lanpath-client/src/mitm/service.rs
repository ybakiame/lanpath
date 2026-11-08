use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use lanpath_common::{LanpathError, LanpathResult};
use lanpath_mitm::{
    CaMaterial, CaStore, FileCaStore, MemoryRecorder, RewriteEngine, RunningProxy,
    ScriptHookEngine, TrafficRecorder,
};
pub use lanpath_mitm::{
    CaOwnedTrustView, CaOwnershipStatus, CaRemovalReceipt, CaTrustScope, CaTrustStatus,
    TrafficRecord,
};
use lanpath_script::{FilePersistentStore, HttpExecutor, Notifier, ScriptHost, ScriptLimits};
use serde::Serialize;

use super::{MitmBuildOptions, build_mitm_proxy_with_ca};
use crate::{
    ClientConfig,
    core_config::MitmChain,
    remote::{RemoteManager, collect_hostname_sources},
};

const TRAFFIC_CAPACITY: usize = 2048;

#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct MitmCaView {
    pub path: String,
    pub pem: String,
    pub sha256: String,
    pub scope: CaTrustScope,
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
        self.manual
            .iter()
            .map(String::as_str)
            .chain(self.derived.iter().map(|entry| entry.hostname.as_str()))
            .any(|hostname| !hostname.starts_with('-') && !hostname.starts_with('!'))
    }

    pub fn load(data_dir: &Path, config: &ClientConfig) -> Self {
        let (excluded, manual) = config
            .mitm
            .hostnames
            .iter()
            .cloned()
            .partition(|hostname| hostname.starts_with('-') || hostname.starts_with('!'));
        let derived = collect_hostname_sources(data_dir)
            .into_iter()
            .map(|entry| DerivedHostnameView {
                hostname: entry.hostname,
                sources: entry.sources,
            })
            .collect();
        Self {
            derived,
            manual,
            excluded,
        }
    }
}

#[derive(Clone)]
pub struct MitmService {
    data_dir: PathBuf,
    recorder: Arc<MemoryRecorder>,
    http: Arc<dyn HttpExecutor>,
    active_ca: Arc<Mutex<Option<String>>>,
}

impl MitmService {
    pub fn new(data_dir: impl Into<PathBuf>, http: Arc<dyn HttpExecutor>) -> Self {
        Self {
            data_dir: data_dir.into(),
            recorder: Arc::new(MemoryRecorder::new(TRAFFIC_CAPACITY)),
            http,
            active_ca: Arc::new(Mutex::new(None)),
        }
    }

    pub fn traffic(&self) -> Vec<TrafficRecord> {
        self.recorder.list()
    }

    pub fn ca(&self) -> LanpathResult<MitmCaView> {
        let material = self.ca_material()?;
        Ok(MitmCaView {
            path: self.ca_path().to_string_lossy().into_owned(),
            sha256: fingerprint(&material)?,
            pem: material.cert_pem,
            scope: if cfg!(windows) {
                CaTrustScope::CurrentUserRoot
            } else {
                CaTrustScope::System
            },
        })
    }

    pub fn trust_status(&self) -> LanpathResult<CaOwnedTrustView> {
        let material = FileCaStore::new(self.data_dir.join("certs")).load_existing();
        let mut view = self.trust_store().query()?;
        match material {
            Ok(material) => {
                let expected = fingerprint(&material)?;
                if view.sha256.as_deref() != Some(expected.as_str()) {
                    return Err(LanpathError::Mitm(
                        "CA source changed during trust query; refresh the displayed certificate"
                            .into(),
                    ));
                }
                self.require_active_ca(&expected)?;
            }
            Err(error) if !self.trust_store().has_receipt()? => return Err(error),
            Err(error) => {
                view.status = CaTrustStatus::Unknown;
                view.detail
                    .push_str(&format!("; signing identity unavailable: {error}"));
            }
        }
        Ok(view)
    }

    pub fn install_ca(&self, expected_sha256: &str) -> LanpathResult<CaOwnedTrustView> {
        self.require_active_ca(expected_sha256)?;
        let material = FileCaStore::new(self.data_dir.join("certs")).load_existing()?;
        self.require_active_ca(&fingerprint(&material)?)?;
        self.trust_store().install(expected_sha256)
    }

    pub fn remove_ca(&self, expected_sha256: &str) -> LanpathResult<CaRemovalReceipt> {
        let receipt = self.trust_store().remove(expected_sha256)?;
        if receipt.disposition == lanpath_mitm::CaRemovalDisposition::OwnershipChanged {
            return Err(LanpathError::Mitm(
                "CA trust ownership changed; certificate and receipt preserved".into(),
            ));
        }
        Ok(receipt)
    }

    fn trust_store(&self) -> lanpath_mitm::OwnedCaTrustStore {
        lanpath_mitm::OwnedCaTrustStore::new(self.data_dir.join("certs"))
    }

    fn require_active_ca(&self, fingerprint: &str) -> LanpathResult<()> {
        let active = self
            .active_ca
            .lock()
            .map_err(|_| LanpathError::Mitm("CA identity lock poisoned".into()))?;
        if active
            .as_ref()
            .is_some_and(|expected| expected != fingerprint)
        {
            return Err(LanpathError::Mitm("CA files differ from the running MITM identity; restore the original CA or stop MITM before authorizing another identity".into()));
        }
        Ok(())
    }

    pub async fn start(
        &self,
        config: &ClientConfig,
        notifier: Arc<dyn Notifier>,
    ) -> LanpathResult<Option<MitmSession>> {
        if !config.mitm_enabled {
            return Ok(None);
        }
        if config.data_dir != self.data_dir || config.mitm.ca_dir != self.data_dir.join("certs") {
            return Err(LanpathError::Config(
                "MITM 服务数据目录或 CA 目录与客户端配置不一致；未生成证书".into(),
            ));
        }

        let remote = RemoteManager::new(self.data_dir.clone());
        let merged = remote.load_cached()?;
        for warning in &merged.warnings {
            tracing::warn!(%warning, "MITM cache activation warning");
        }
        let hook_rules = merged.scripts;
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
            self.http.clone(),
            Arc::new(FilePersistentStore::new(self.data_dir.join("script_store"))),
            notifier,
        ));
        let hooks = ScriptHookEngine::new(host, ScriptLimits::default(), hook_rules);
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
        let material = self.ca_material()?;
        let identity = CaIdentityLease::acquire(self.active_ca.clone(), fingerprint(&material)?)?;
        let running = build_mitm_proxy_with_ca(config, options, recorder, material)?
            .start()
            .await?;
        let chain = MitmChain {
            proxy_addr: running.addr,
            return_port,
            hostnames,
        };
        Ok(Some(MitmSession {
            running: Some(running),
            chain,
            identity: Some(identity),
        }))
    }

    fn ca_material(&self) -> LanpathResult<CaMaterial> {
        let store = FileCaStore::new(self.data_dir.join("certs"));
        let active = self
            .active_ca
            .lock()
            .map_err(|_| LanpathError::Mitm("CA identity lock poisoned".into()))?
            .is_some()
            || self.trust_store().has_receipt()?;
        let material = if active {
            store.load_existing()?
        } else {
            store.load_or_generate()?
        };
        self.require_active_ca(&fingerprint(&material)?)?;
        Ok(material)
    }

    fn ca_path(&self) -> PathBuf {
        self.data_dir.join("certs").join("ca.crt")
    }
}

pub struct MitmSession {
    running: Option<RunningProxy>,
    chain: MitmChain,
    identity: Option<CaIdentityLease>,
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

    pub async fn stop(&mut self) -> LanpathResult<()> {
        if let Some(running) = self.running.as_mut() {
            running.shutdown().await?;
        }
        self.running = None;
        self.identity = None;
        Ok(())
    }
}

struct CaIdentityLease(Arc<Mutex<Option<String>>>);
impl CaIdentityLease {
    fn acquire(active: Arc<Mutex<Option<String>>>, fingerprint: String) -> LanpathResult<Self> {
        {
            let mut current = active
                .lock()
                .map_err(|_| LanpathError::Mitm("CA identity lock poisoned".into()))?;
            if current.is_some() {
                return Err(LanpathError::Mitm(
                    "MITM session already uses this CA service".into(),
                ));
            }
            *current = Some(fingerprint);
        }
        Ok(Self(active))
    }
}
impl Drop for CaIdentityLease {
    fn drop(&mut self) {
        if let Ok(mut active) = self.0.lock() {
            *active = None;
        }
    }
}

fn fingerprint(material: &CaMaterial) -> LanpathResult<String> {
    Ok(lanpath_mitm::sha256_hex(&lanpath_mitm::pem_to_der(
        &material.cert_pem,
    )?))
}

pub fn has_effective_whitelist(hostnames: &[String]) -> bool {
    hostnames
        .iter()
        .any(|hostname| !hostname.starts_with('-') && !hostname.starts_with('!'))
}

#[cfg(test)]
#[path = "service_ca_tests.rs"]
mod ca_tests;

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
        let mut view = MitmWhitelistView::default();
        view.derived.push(DerivedHostnameView {
            hostname: "-excluded.example".into(),
            sources: vec!["module".into()],
        });
        assert!(!view.has_inclusions());
        view.derived.push(DerivedHostnameView {
            hostname: "api.example".into(),
            sources: vec!["module".into()],
        });
        assert!(view.has_inclusions());
    }

    #[tokio::test]
    async fn starts_only_for_an_enabled_effective_whitelist() {
        let root = tempfile::tempdir().unwrap();
        let service = MitmService::new(
            root.path(),
            Arc::new(lanpath_script::MockHttpExecutor::with_responses(vec![])),
        );
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
        let mut session = service
            .start(&config, notifier)
            .await
            .unwrap()
            .expect("effective whitelist should start MITM");
        use tokio::io::AsyncReadExt as _;
        let addr = session.addr();
        let mut socket = tokio::net::TcpStream::connect(addr).await.unwrap();
        session.stop().await.unwrap();
        let mut byte = [0];
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(2), socket.read(&mut byte))
                .await
                .unwrap();
        assert!(
            matches!(result, Ok(0))
                || result.is_err_and(|error| matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ))
        );
        assert!(tokio::net::TcpStream::connect(addr).await.is_err());
    }
}
