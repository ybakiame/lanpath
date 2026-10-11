//! Client error types, sing-box identity and operating-system lifecycle utilities.
pub mod error;
pub mod process;
pub mod protocol;
pub use error::*;
pub use process::*;
pub use protocol::*;
