mod backup;
#[cfg(test)]
mod tests;
mod theme;

use std::path::{Path, PathBuf};

use gpui_kit::{App, Global, WindowAppearance};
use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};

use crate::{configuration::resources, runtime::DesktopRuntime};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AccentPreference {
    Iris,
    Teal,
    Rose,
    #[default]
    Blue,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppearancePreferences {
    theme: ThemePreference,
    accent: AccentPreference,
    pure_black: bool,
    scale_percent: u16,
}

impl Default for AppearancePreferences {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            accent: AccentPreference::Blue,
            pure_black: false,
            scale_percent: 100,
        }
    }
}

impl AppearancePreferences {
    pub(crate) fn theme(self) -> ThemePreference {
        self.theme
    }
    pub(crate) fn accent(self) -> AccentPreference {
        self.accent
    }
    pub(crate) fn pure_black(self) -> bool {
        self.pure_black
    }
    pub(crate) fn scale_percent(self) -> u16 {
        self.scale_percent
    }

    pub(crate) fn with_theme(mut self, theme: ThemePreference) -> Self {
        self.theme = theme;
        self
    }

    pub(crate) fn with_accent(mut self, accent: AccentPreference) -> Self {
        self.accent = accent;
        self
    }

    pub(crate) fn with_pure_black(mut self, pure_black: bool) -> Self {
        self.pure_black = pure_black;
        self
    }

    pub(crate) fn with_scale_percent(mut self, scale_percent: u16) -> Self {
        self.scale_percent = scale_percent;
        self
    }

    fn validate(self) -> LanpathResult<Self> {
        if !(75..=150).contains(&self.scale_percent) {
            return Err(LanpathError::Config(
                "appearance.json: scale_percent 必须是 75 到 150 之间的整数".into(),
            ));
        }
        Ok(self)
    }

    pub(crate) fn is_dark(self, appearance: WindowAppearance) -> bool {
        match self.theme {
            ThemePreference::System => matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
            ThemePreference::Light => false,
            ThemePreference::Dark => true,
        }
    }

    pub(crate) fn effective_pure_black(self, appearance: WindowAppearance) -> bool {
        self.pure_black && self.is_dark(appearance)
    }
}

#[derive(Clone)]
pub(crate) struct AppearanceStore {
    root: PathBuf,
}

impl AppearanceStore {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }

    pub(crate) fn load(&self) -> LanpathResult<AppearancePreferences> {
        let app = lanpath_client::settings::app_settings_path(&self.root);
        if app.exists() {
            let preferences: AppearancePreferences =
                lanpath_client::settings::load_app_section(&self.root, "appearance")?
                    .unwrap_or_default();
            return preferences.validate();
        }
        let path = resources::confined(&self.root, "appearance.json", "appearance.json")?;
        match std::fs::read(&path) {
            Ok(bytes) => {
                let preferences: AppearancePreferences = serde_json::from_slice(&bytes)
                    .map_err(|error| LanpathError::Config(format!("appearance.json: {error}")))?;
                preferences.validate()
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(AppearancePreferences::default())
            }
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn save(&self, preferences: AppearancePreferences) -> LanpathResult<()> {
        let preferences = preferences.validate()?;
        // A damaged/replaced file must not be silently erased by an already-open view.
        self.load()?;
        lanpath_client::settings::ensure_app_settings(&self.root)?;
        lanpath_client::settings::persist_app_section(&self.root, "appearance", &preferences)
    }
}

pub(crate) struct AppearanceState {
    preferences: AppearancePreferences,
    store: Option<AppearanceStore>,
    load_error: Option<String>,
    restore_warning: Option<String>,
}

impl Global for AppearanceState {}

impl AppearanceState {
    pub(crate) fn preferences(&self) -> AppearancePreferences {
        self.preferences
    }
    pub(crate) fn load_error(&self) -> Option<&str> {
        self.load_error.as_deref()
    }
    pub(crate) fn restore_warning(&self) -> Option<&str> {
        self.restore_warning.as_deref()
    }

    pub(crate) fn save(&mut self, preferences: AppearancePreferences) -> LanpathResult<()> {
        if self.load_error.is_some() {
            return Err(LanpathError::Config("请先重新载入有效的外观偏好".into()));
        }
        self.store
            .as_ref()
            .ok_or_else(|| LanpathError::Config("实验数据目录不可用，不能保存外观偏好".into()))?
            .save(preferences)?;
        self.preferences = preferences;
        Ok(())
    }

    pub(crate) fn reload(&mut self) -> LanpathResult<()> {
        let result = self
            .store
            .as_ref()
            .ok_or_else(|| LanpathError::Config("实验数据目录不可用，不能载入外观偏好".into()))
            .and_then(AppearanceStore::load);
        match result {
            Ok(preferences) => {
                self.preferences = preferences;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }
}

/// Call after GPUI Kit initialization and before opening the first window.
/// On a load error the default is display-only: writes remain blocked until a
/// successful explicit reload, and the error is presented by AppearanceView.
pub(crate) fn initialize(core: &DesktopRuntime, cx: &mut App) {
    let store = core
        .business()
        .ok()
        .map(|business| AppearanceStore::new(business.data_dir()));
    let mut state = AppearanceState {
        preferences: AppearancePreferences::default(),
        store,
        load_error: None,
        restore_warning: None,
    };
    let _ = state.reload();
    let preferences = state.preferences;
    cx.set_global(state);
    apply(preferences, cx.window_appearance(), cx);
}

pub(crate) fn restore_backup(core: &DesktopRuntime, cx: &mut App) {
    let appearance = cx.window_appearance();
    let preferences = {
        let state = cx.global_mut::<AppearanceState>();
        let _ = state.reload();
        backup::restore_at_startup(core, state);
        state.preferences
    };
    apply(preferences, appearance, cx);
}

pub(crate) fn apply(
    preferences: AppearancePreferences,
    appearance: WindowAppearance,
    cx: &mut App,
) {
    theme::apply(preferences, appearance, cx);
}
