use serde::{Deserialize, Serialize};
/// The client supports one runtime engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CoreType {
    #[default]
    SingBox,
}
impl CoreType {
    pub fn github_repo(&self) -> (&'static str, &'static str) {
        ("SagerNet", "sing-box")
    }
}
impl std::fmt::Display for CoreType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sing-box")
    }
}
