use crate::types::ScriptDialect;

/// 方言处理：同一 host 能力按方言注入不同全局名。
///
/// Lists vendor-specific globals; shared execution globals are installed separately.
/// Loon and Stash are distinct callback APIs, not Quantumult X supersets.
pub fn dialect_globals(dialect: ScriptDialect) -> &'static [&'static str] {
    match dialect {
        ScriptDialect::QuantumultX => &["$task", "$prefs", "$notify"],
        ScriptDialect::Surge | ScriptDialect::Stash => {
            &["$httpClient", "$persistentStore", "$notification"]
        }
        ScriptDialect::Loon => &["$httpClient", "$persistentStore", "$notification", "$loon"],
    }
}
