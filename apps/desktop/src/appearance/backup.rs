use super::*;
use lanpath_client::backup::Appearance;

impl AppearancePreferences {
    pub(crate) fn backup_appearance(self) -> Appearance {
        Appearance {
            theme: match self.theme {
                ThemePreference::System => "system",
                ThemePreference::Light => "light",
                ThemePreference::Dark => "dark",
            }
            .into(),
            accent: match self.accent {
                AccentPreference::Iris => "iris",
                AccentPreference::Teal => "teal",
                AccentPreference::Rose => "rose",
                AccentPreference::Blue => "blue",
            }
            .into(),
            pure_black: self.pure_black,
        }
    }
}

impl AppearanceState {
    fn restore_portable(&mut self, appearance: &Appearance) -> LanpathResult<Option<String>> {
        let theme = match appearance.theme.as_str() {
            "system" => ThemePreference::System,
            "light" => ThemePreference::Light,
            "dark" => ThemePreference::Dark,
            _ => return Err(LanpathError::Config("备份中的主题不受支持".into())),
        };
        let (accent, warning) = match appearance.accent.as_str() {
            "iris" => (AccentPreference::Iris, None),
            "teal" => (AccentPreference::Teal, None),
            "rose" => (AccentPreference::Rose, None),
            "blue" => (AccentPreference::Blue, None),
            other => (
                self.preferences.accent,
                Some(format!(
                    "备份强调色 {other} 不受 GPUI 实验版支持；已保留本机强调色，其他外观已恢复。"
                )),
            ),
        };
        // The shared v1/v2 Appearance contract has no interface scale. Keep the
        // local zoom; do not add a second archive format for this shell.
        let preferences = self
            .preferences
            .with_theme(theme)
            .with_accent(accent)
            .with_pure_black(appearance.pure_black);
        if self.load_error.is_some() || self.preferences != preferences {
            self.save(preferences)?;
        }
        Ok(warning)
    }
}

pub(super) fn restore_at_startup(core: &DesktopRuntime, state: &mut AppearanceState) {
    // BackupView owns reporting and acknowledgement of the startup transaction.
    // Leave its marker intact until the user has seen the restored summary.
    let Some(appearance) = core
        .backup()
        .ok()
        .and_then(|service| service.restored().ok().flatten())
        .and_then(|summary| summary.appearance)
    else {
        return;
    };
    state.restore_warning = match state.restore_portable(&appearance) {
        Ok(warning) => warning,
        Err(error) => Some(format!("配置已恢复，但外观未能恢复：{error}")),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_restore_preserves_zoom_and_unsupported_accent_without_losing_other_changes() {
        let dir = tempfile::tempdir().unwrap();
        let store = AppearanceStore::new(dir.path());
        let preferences = AppearancePreferences::default()
            .with_accent(AccentPreference::Iris)
            .with_material(super::super::MaterialPreference::Solid)
            .with_scale_percent(117);
        store.save(preferences).unwrap();
        let mut state = AppearanceState {
            preferences,
            store: Some(store.clone()),
            load_error: None,
            restore_warning: None,
        };
        assert!(
            state
                .restore_portable(&Appearance {
                    theme: "dark".into(),
                    accent: "amber".into(),
                    pure_black: true
                })
                .unwrap()
                .is_some()
        );
        let restored = store.load().unwrap();
        assert_eq!(restored.accent(), AccentPreference::Iris);
        assert_eq!(restored.theme(), ThemePreference::Dark);
        assert!(restored.pure_black());
        assert_eq!(restored.scale_percent(), 117);
        assert_eq!(restored.material(), super::super::MaterialPreference::Solid);
        state
            .restore_portable(&Appearance {
                theme: "light".into(),
                accent: "rose".into(),
                pure_black: false,
            })
            .unwrap();
        assert_eq!(store.load().unwrap().accent(), AccentPreference::Rose);
        assert_eq!(store.load().unwrap().scale_percent(), 117);
    }

    #[test]
    fn damaged_local_appearance_is_not_overwritten_by_startup_restore() {
        let dir = tempfile::tempdir().unwrap();
        let store = AppearanceStore::new(dir.path());
        std::fs::write(dir.path().join("appearance.json"), "damaged").unwrap();
        let mut state = AppearanceState {
            preferences: AppearancePreferences::default(),
            store: Some(store),
            load_error: Some("damaged".into()),
            restore_warning: None,
        };
        assert!(
            state
                .restore_portable(&Appearance {
                    theme: "dark".into(),
                    accent: "rose".into(),
                    pure_black: true
                })
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("appearance.json")).unwrap(),
            "damaged"
        );
        assert_eq!(state.preferences(), AppearancePreferences::default());
    }
}
