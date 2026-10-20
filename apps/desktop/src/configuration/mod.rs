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

#[derive(Clone, Copy, Debug)]
pub(crate) struct Bindings {
    mixed_port: u16,
    api_port: u16,
}

impl Bindings {
    pub(crate) fn new(mixed_port: u16, api_port: u16) -> Self {
        Self {
            mixed_port,
            api_port,
        }
    }

    pub(crate) fn mixed_port(self) -> u16 {
        self.mixed_port
    }

    pub(crate) fn api_port(self) -> u16 {
        self.api_port
    }
}

#[cfg(test)]
mod tests;
