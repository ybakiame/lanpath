//! Native services for the independent GPUI product.
pub(crate) mod instance;
pub(crate) mod migration;
pub(crate) mod native;
#[cfg(any(windows, test))]
pub(crate) mod payload;
#[cfg(any(windows, test))]
mod payload_files;
pub(crate) mod signature;
pub(crate) mod updater;

use lanpath_common::LanpathError;
pub(super) fn invalid(message: impl Into<String>) -> LanpathError {
    LanpathError::Config(message.into())
}
