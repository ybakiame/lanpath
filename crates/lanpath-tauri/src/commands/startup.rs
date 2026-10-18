//! Startup feedback is independent of the client mutex held during composition.
use crate::state::AppState;
use std::sync::Mutex;
use tauri::State;

#[derive(Default)]
pub struct StartupProgress(Mutex<Option<String>>);

impl StartupProgress {
    pub fn set(&self, message: Option<&str>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = message.map(str::to_owned);
    }

    pub fn begin(&self) -> StartupGuard<'_> {
        self.set(Some("准备启动"));
        StartupGuard(self)
    }
}

pub struct StartupGuard<'a>(&'a StartupProgress);

impl Drop for StartupGuard<'_> {
    fn drop(&mut self) {
        self.0.set(None);
    }
}

#[tauri::command]
pub fn proxy_startup_progress(state: State<'_, AppState>) -> Option<String> {
    state
        .startup_progress
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_is_readable_and_cleared_on_error_or_completion() {
        let progress = StartupProgress::default();
        {
            let _guard = progress.begin();
            progress.set(Some("合成配置"));
            assert_eq!(progress.0.lock().unwrap().as_deref(), Some("合成配置"));
        }
        assert!(progress.0.lock().unwrap().is_none());
    }
}
