//! Per-hook metadata is persisted without resource-wide dialect guesses.
use lanpath_common::{LanpathError, LanpathResult};
#[cfg(feature = "mitm")]
use lanpath_mitm::{HookMode, Pattern, ScriptRule};
use lanpath_script::{ScriptDialect, ScriptKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedScriptRule {
    pub name: String,
    pub kind: ScriptKind,
    pub pattern: String,
    pub requires_body: bool,
    pub max_size: usize,
    pub source: String,
    pub argument: Option<String>,
    #[serde(default)]
    pub dialect: Option<ScriptDialect>,
    #[cfg(feature = "mitm")]
    #[serde(default)]
    pub mode: Option<HookMode>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub binary: bool,
    #[serde(default)]
    pub full_headers: bool,
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub module_id: Option<String>,
}
#[cfg(feature = "mitm")]
impl From<&ScriptRule> for CachedScriptRule {
    fn from(rule: &ScriptRule) -> Self {
        Self {
            name: rule.name.clone(),
            kind: rule.kind,
            pattern: rule.pattern.as_str().into(),
            requires_body: rule.requires_body,
            max_size: rule.max_size,
            source: rule.source.clone(),
            argument: rule.argument.clone(),
            dialect: Some(rule.dialect),
            mode: Some(rule.mode),
            enabled: rule.enabled,
            timeout_ms: rule.timeout_ms,
            binary: rule.binary,
            full_headers: rule.full_headers,
            source_id: rule.source_id.clone(),
            module_id: rule.module_id.clone(),
        }
    }
}
#[cfg(feature = "mitm")]
impl TryFrom<CachedScriptRule> for ScriptRule {
    type Error = LanpathError;
    fn try_from(rule: CachedScriptRule) -> LanpathResult<Self> {
        let invalid = || {
            LanpathError::Client(
                "hook cache lacks attributable dialect/source/module metadata; refetch or reimport"
                    .into(),
            )
        };
        if rule.source_id.is_empty()
            || rule.source.is_empty()
            || rule.module_id.as_deref().is_none_or(str::is_empty)
            || !matches!(
                rule.kind,
                ScriptKind::HttpRequest | ScriptKind::HttpResponse
            )
        {
            return Err(invalid());
        }
        if rule.dialect == Some(ScriptDialect::Surge)
            && rule.max_size > lanpath_mitm::SURGE_BODY_ENVELOPE
            && !(rule.kind == ScriptKind::HttpResponse && rule.max_size == usize::MAX)
        {
            return Err(LanpathError::Client(
                "cached Surge body limit exceeds the supported 32 MiB inspection envelope; refetch or reimport".into(),
            ));
        }
        Ok(Self {
            name: rule.name,
            kind: rule.kind,
            pattern: Pattern::new(&rule.pattern).map_err(|error| {
                LanpathError::Client(format!("cached hook pattern not activated: {error}"))
            })?,
            requires_body: rule.requires_body,
            max_size: rule.max_size,
            source: rule.source,
            argument: rule.argument,
            dialect: rule.dialect.ok_or_else(invalid)?,
            mode: rule.mode.ok_or_else(invalid)?,
            enabled: rule.enabled,
            timeout_ms: rule.timeout_ms,
            binary: rule.binary,
            full_headers: rule.full_headers,
            source_id: rule.source_id,
            module_id: rule.module_id,
        })
    }
}

#[cfg(all(test, feature = "mitm"))]
mod tests {
    use super::*;

    #[test]
    fn cached_surge_numeric_limits_above_envelope_cannot_reactivate() {
        let rule = ScriptRule {
            name: "cached".into(),
            kind: ScriptKind::HttpRequest,
            pattern: Pattern::new(".*").unwrap(),
            requires_body: true,
            max_size: lanpath_mitm::SURGE_BODY_ENVELOPE,
            source: "$done({})".into(),
            argument: None,
            dialect: ScriptDialect::Surge,
            mode: HookMode::Modify,
            enabled: true,
            timeout_ms: None,
            binary: false,
            full_headers: false,
            source_id: "https://assets.example/hook.js".into(),
            module_id: Some("module".into()),
        };
        assert!(ScriptRule::try_from(CachedScriptRule::from(&rule)).is_ok());
        let mut oversized = CachedScriptRule::from(&rule);
        oversized.max_size += 1;
        assert!(ScriptRule::try_from(oversized).is_err());
        let mut response_unlimited = CachedScriptRule::from(&rule);
        response_unlimited.kind = ScriptKind::HttpResponse;
        response_unlimited.max_size = usize::MAX;
        assert!(ScriptRule::try_from(response_unlimited).is_ok());
    }
}
