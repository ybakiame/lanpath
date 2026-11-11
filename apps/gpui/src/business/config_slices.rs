use lanpath_client::{
    config_slices::{ConfigSlices, ConfigSlicesStore, DnsServer, DnsSlice},
    core_config::builtin_dns_slice,
    dns_probe::{DnsProbeInput, probe_dns_server},
};
use lanpath_common::{LanpathError, LanpathResult};

use super::BusinessService;

#[derive(Debug, Clone)]
pub(crate) struct ConfigSlicesDocument {
    pub(crate) slices: ConfigSlices,
    pub(crate) builtin_dns: DnsSlice,
}

impl BusinessService {
    pub(crate) fn config_slices(&self) -> LanpathResult<ConfigSlicesDocument> {
        let slices = ConfigSlicesStore::new(self.root.clone()).load()?;
        Ok(ConfigSlicesDocument {
            slices,
            // Experimental networking always disables IPv6 in client.json.
            builtin_dns: builtin_dns_slice(false),
        })
    }

    pub(crate) fn save_config_slices(
        &self,
        slices: &ConfigSlices,
    ) -> LanpathResult<ConfigSlicesDocument> {
        ConfigSlicesStore::new(self.root.clone()).save(slices)?;
        self.config_slices()
    }

    pub(crate) async fn probe_dns_server(&self, server: DnsServer) -> LanpathResult<u64> {
        probe_dns_server(&DnsProbeInput {
            server_type: server.server_type.as_str().to_string(),
            server: server.server,
            server_port: server.server_port,
            domain: None,
        })
        .await
        .map_err(LanpathError::Client)
    }
}
