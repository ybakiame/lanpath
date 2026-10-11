use lanpath_common::{LanpathError, LanpathResult};

/// `LanpathError::Script` 的辅助构造函数。
pub fn script_err(msg: impl Into<String>) -> LanpathError {
    LanpathError::Script(msg.into())
}

/// `LanpathResult` 的脚本错误快捷别名。
pub type ScriptResult<T> = LanpathResult<T>;
