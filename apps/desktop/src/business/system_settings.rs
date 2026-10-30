use lanpath_client::ClientConfig;
use lanpath_common::{LanpathError, LanpathResult};

use super::{BusinessService, state::persist};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SystemSettingsDocument {
    pub system_proxy_enabled: bool,
    pub tun_enabled: bool,
    pub tun_stack: String,
    pub tun_auto_route: bool,
    pub ipv6_enabled: bool,
    pub dns_fakeip_enabled: bool,
    pub mixed_port: u16,
    pub clash_api_enabled: bool,
    pub clash_api_port: u16,
    pub clash_api_secret: String,
    pub clash_api_ui: String,
}

impl From<&ClientConfig> for SystemSettingsDocument {
    fn from(config: &ClientConfig) -> Self {
        Self {
            system_proxy_enabled: config.system_proxy_enabled,
            tun_enabled: config.tun_enabled,
            tun_stack: config.tun_stack.clone(),
            tun_auto_route: config.tun_auto_route,
            ipv6_enabled: config.ipv6_enabled,
            dns_fakeip_enabled: config.dns_fakeip_enabled,
            mixed_port: config.mixed_port,
            clash_api_enabled: config.clash_api_enabled,
            clash_api_port: config.clash_api_port,
            clash_api_secret: config.clash_api_secret.clone(),
            clash_api_ui: config.clash_api_ui.clone(),
        }
    }
}

impl SystemSettingsDocument {
    fn validate(&self, mitm_enabled: bool) -> LanpathResult<()> {
        if self.mixed_port == 0 {
            return Err(invalid("混合代理端口必须是 1–65535 之间的整数"));
        }
        if self.clash_api_enabled
            && (self.clash_api_port == 0 || self.clash_api_port == self.mixed_port)
        {
            return Err(invalid("Clash API 端口必须非零，且不能与混合代理端口相同"));
        }
        if self.tun_enabled
            && (self.mixed_port < 1024 || (self.clash_api_enabled && self.clash_api_port < 1024))
        {
            return Err(invalid(
                "TUN helper 只允许 1024–65535 的本地代理和 API 端口",
            ));
        }
        if mitm_enabled {
            let returned = self
                .mixed_port
                .checked_add(1)
                .ok_or_else(|| invalid("MITM 回流端口溢出，请降低混合代理端口"))?;
            if self.clash_api_enabled && returned == self.clash_api_port {
                return Err(invalid("Clash API 端口不能与 MITM 回流端口相同"));
            }
        }
        if !matches!(self.tun_stack.as_str(), "mixed" | "system" | "gvisor") {
            return Err(invalid("桌面 TUN 协议栈必须是 mixed、system 或 gvisor"));
        }
        if !matches!(
            self.clash_api_ui.as_str(),
            "yacd" | "zashboard" | "metacubexd"
        ) {
            return Err(invalid("请选择受支持的 Clash 面板"));
        }
        if self.clash_api_secret.contains(['\r', '\n', '\0']) {
            return Err(invalid("Clash API 密钥不能包含换行或空字符"));
        }
        Ok(())
    }

    fn apply(&self, config: &mut ClientConfig) {
        config.system_proxy_enabled = self.system_proxy_enabled;
        config.tun_enabled = self.tun_enabled;
        config.tun_stack.clone_from(&self.tun_stack);
        config.tun_auto_route = self.tun_auto_route;
        config.ipv6_enabled = self.ipv6_enabled;
        config.dns_fakeip_enabled = self.dns_fakeip_enabled;
        config.mixed_port = self.mixed_port;
        config.clash_api_enabled = self.clash_api_enabled;
        config.clash_api_port = self.clash_api_port;
        config.clash_api_secret.clone_from(&self.clash_api_secret);
        config.clash_api_ui.clone_from(&self.clash_api_ui);
    }
}

impl BusinessService {
    pub(crate) fn system_settings(&self) -> LanpathResult<SystemSettingsDocument> {
        Ok(SystemSettingsDocument::from(&self.client_config()?))
    }

    pub(crate) fn clash_api_endpoint(&self) -> LanpathResult<(u16, String)> {
        let config = self.client_config()?;
        if !config.clash_api_enabled {
            return Err(invalid("Clash API 未启用"));
        }
        Ok((config.clash_api_port, config.clash_api_secret))
    }

    pub(crate) fn save_system_settings(
        &self,
        settings: &SystemSettingsDocument,
    ) -> LanpathResult<()> {
        let mut config = self.client_config()?;
        settings.validate(config.mitm_enabled)?;
        settings.apply(&mut config);
        persist(&config.config_file(), &config)
    }
}

fn invalid(message: &str) -> LanpathError {
    LanpathError::Config(message.into())
}
