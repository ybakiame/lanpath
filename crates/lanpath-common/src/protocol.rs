use serde::{Deserialize, Serialize};

/// Runtime engines supported by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CoreType {
    #[default]
    #[serde(rename = "sing-box", alias = "singbox")]
    SingBox,
    #[serde(rename = "mihomo")]
    Mihomo,
}

impl CoreType {
    pub fn as_str(self) -> &'static str {
        self.binary_name()
    }

    pub fn binary_name(self) -> &'static str {
        match self {
            Self::SingBox => "sing-box",
            Self::Mihomo => "mihomo",
        }
    }

    pub fn github_repo(self) -> (&'static str, &'static str) {
        match self {
            Self::SingBox => ("SagerNet", "sing-box"),
            Self::Mihomo => ("MetaCubeX", "mihomo"),
        }
    }
}

impl std::fmt::Display for CoreType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::CoreType;

    #[test]
    fn core_type_mappings_and_serde_names_are_stable() {
        assert_eq!(CoreType::SingBox.as_str(), "sing-box");
        assert_eq!(CoreType::SingBox.github_repo(), ("SagerNet", "sing-box"));
        assert_eq!(CoreType::Mihomo.as_str(), "mihomo");
        assert_eq!(CoreType::Mihomo.binary_name(), "mihomo");
        assert_eq!(CoreType::Mihomo.github_repo(), ("MetaCubeX", "mihomo"));
        assert_eq!(CoreType::Mihomo.to_string(), "mihomo");
        assert_eq!(
            serde_json::to_string(&CoreType::SingBox).unwrap(),
            "\"sing-box\""
        );
        assert_eq!(
            serde_json::from_str::<CoreType>("\"sing-box\"").unwrap(),
            CoreType::SingBox
        );
        assert_eq!(
            serde_json::from_str::<CoreType>("\"mihomo\"").unwrap(),
            CoreType::Mihomo
        );
    }
}
