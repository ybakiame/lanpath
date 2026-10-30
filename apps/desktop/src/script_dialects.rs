use lanpath_client::ScriptDialect;
use lanpath_common::{LanpathError, LanpathResult};

pub(crate) fn is_selectable(dialect: ScriptDialect) -> bool {
    matches!(dialect, ScriptDialect::Surge | ScriptDialect::QuantumultX)
}

pub(crate) fn selection_allowed(dialect: ScriptDialect, previous: Option<ScriptDialect>) -> bool {
    is_selectable(dialect) || previous == Some(dialect)
}

pub(crate) fn validate_selection(
    dialect: ScriptDialect,
    previous: Option<ScriptDialect>,
) -> LanpathResult<()> {
    if selection_allowed(dialect, previous) {
        return Ok(());
    }
    Err(LanpathError::Script(
        "GPUI 当前仅开放 Surge 与 Quantumult X；Loon、Stash 仅保留历史数据，不开放新增或切换"
            .into(),
    ))
}
