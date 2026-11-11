//! Persisted, user-authored sing-box documents for the experimental desktop.
//!
//! This is a deliberately restricted capability policy, not an unrestricted
//! sing-box editor or an OS sandbox. Schema correctness belongs to the selected
//! real core. Accepted source text and JSON values are never rewritten.
mod outbound;
mod policy;
pub(crate) mod resources;
mod validator;

pub(crate) use validator::ConfigValidator;

#[derive(Clone)]
pub(crate) struct Bindings {
    mixed_port: u16,
    api_port: u16,
    api_secret: String,
    api_enabled: bool,
    tun_enabled: bool,
}

impl Bindings {
    pub(crate) fn new(mixed_port: u16, api_port: u16) -> Self {
        Self {
            mixed_port,
            api_port,
            api_secret: String::new(),
            api_enabled: true,
            tun_enabled: false,
        }
    }

    pub(crate) fn mixed_port(&self) -> u16 {
        self.mixed_port
    }

    pub(crate) fn api_port(&self) -> u16 {
        self.api_port
    }

    pub(crate) fn from_config(config: &lanpath_client::ClientConfig) -> Self {
        Self {
            mixed_port: config.mixed_port,
            api_port: config.clash_api_port,
            api_secret: config.clash_api_secret.clone(),
            api_enabled: config.clash_api_enabled,
            tun_enabled: config.tun_enabled,
        }
    }
}

/// The elevated helper derives bindings from its already bounded immutable request,
/// then applies the same complete capability schema as the unelevated editor.
#[cfg(any(windows, test))]
pub(crate) fn check_privileged_document(
    root: &std::path::Path,
    value: &serde_json::Value,
) -> lanpath_common::LanpathResult<()> {
    use resources::denied;
    use serde_json::Value;
    let mixed_port = value
        .get("inbounds")
        .and_then(Value::as_array)
        .and_then(|inbounds| {
            inbounds
                .iter()
                .find(|entry| entry.get("type").and_then(Value::as_str) == Some("mixed"))
        })
        .and_then(|inbound| inbound.get("listen_port"))
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port >= 1024)
        .ok_or_else(|| {
            denied(
                "inbounds",
                "a managed unprivileged mixed listener is required",
            )
        })?;
    let api = value.pointer("/experimental/clash_api");
    let api_port = match api {
        Some(api) => api
            .get("external_controller")
            .and_then(Value::as_str)
            .and_then(|address| address.parse::<std::net::SocketAddr>().ok())
            .filter(|address| {
                address.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                    && address.port() >= 1024
            })
            .map(|address| address.port())
            .ok_or_else(|| {
                denied(
                    "experimental.clash_api.external_controller",
                    "an unprivileged IPv4 loopback API endpoint is required",
                )
            })?,
        None => 0,
    };
    let bindings = Bindings {
        mixed_port,
        api_port,
        api_enabled: api.is_some(),
        tun_enabled: true,
        api_secret: api
            .and_then(|api| api.get("secret"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    };
    policy::check(root, value, &bindings)
}

#[cfg(test)]
mod tests;
