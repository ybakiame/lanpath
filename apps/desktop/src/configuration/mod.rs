//! Persisted, user-authored sing-box documents for the experimental desktop.
//!
//! This is a deliberately restricted capability policy, not an unrestricted
//! sing-box editor or an OS sandbox. Schema correctness belongs to the selected
//! real core. Accepted source text and JSON values are never rewritten.
mod outbound;
mod policy;
mod resources;
mod store;
mod unique_json;

pub(crate) use store::ConfigStore;

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
}

#[cfg(test)]
mod tests;
