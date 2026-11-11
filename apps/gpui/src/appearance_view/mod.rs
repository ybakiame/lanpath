mod render;

use gpui_kit::component::{
    searchable_list::SearchableListItem,
    select::{SelectEvent, SelectState},
};
use gpui_kit::{AppContext as _, Context, Entity, SharedString, Subscription, Window};

use crate::appearance::{
    self, AccentPreference, AppearancePreferences, AppearanceState, ThemePreference,
};

const THEMES: [(ThemePreference, &str, &str); 3] = [
    (
        ThemePreference::System,
        "appearance-theme-system",
        "跟随系统",
    ),
    (ThemePreference::Light, "appearance-theme-light", "浅色"),
    (ThemePreference::Dark, "appearance-theme-dark", "深色"),
];
const ACCENTS: [(AccentPreference, &str, &str); 4] = [
    (AccentPreference::Iris, "appearance-accent-iris", "鸢尾紫"),
    (AccentPreference::Teal, "appearance-accent-teal", "湖水青"),
    (AccentPreference::Rose, "appearance-accent-rose", "玫瑰粉"),
    (AccentPreference::Blue, "appearance-accent-blue", "品牌蓝"),
];

#[derive(Clone)]
struct PercentOption {
    percent: u16,
    title: SharedString,
}

impl SearchableListItem for PercentOption {
    type Value = u16;
    fn title(&self) -> SharedString {
        self.title.clone()
    }
    fn value(&self) -> &Self::Value {
        &self.percent
    }
}

fn percent_options(current: u16, mut values: Vec<u16>) -> Vec<PercentOption> {
    if !values.contains(&current) {
        values.push(current);
        values.sort_unstable();
    }
    values
        .into_iter()
        .map(|percent| PercentOption {
            percent,
            title: format!("{percent}%").into(),
        })
        .collect()
}

fn scale_options(current: u16) -> Vec<PercentOption> {
    percent_options(current, vec![75, 90, 100, 110, 125, 150])
}

fn acrylic_opacity_options(current: u16) -> Vec<PercentOption> {
    percent_options(current, vec![20, 35, 50, 65, 80, 100])
}

pub(crate) struct AppearanceView {
    preferences: AppearancePreferences,
    scale: Entity<SelectState<Vec<PercentOption>>>,
    acrylic_opacity: Entity<SelectState<Vec<PercentOption>>>,
    closing: bool,
    error: Option<String>,
    _scale_subscription: Subscription,
    _acrylic_opacity_subscription: Subscription,
    _appearance_subscription: Subscription,
}

impl AppearanceView {
    /// The application initializes AppearanceState before opening this window.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppearanceState>();
        let preferences = state.preferences();
        let error = state.load_error().map(str::to_owned);
        // Use the actual window appearance once available (notably on Linux).
        appearance::apply_to_window(preferences, window, cx);
        let scale = cx.new(|cx| {
            SelectState::new(scale_options(preferences.scale_percent()), None, window, cx)
        });
        scale.update(cx, |state, cx| {
            state.set_selected_value(&preferences.scale_percent(), window, cx);
        });
        let scale_subscription = cx.subscribe_in(
            &scale,
            window,
            |this, _, event: &SelectEvent<Vec<PercentOption>>, window, cx| {
                if let SelectEvent::Confirm(Some(percent)) = event {
                    this.change(this.preferences.with_scale_percent(*percent), window, cx);
                    // Select commits before emitting. Roll back its presentation on I/O
                    // failure so it never claims an unsaved value took effect.
                    let saved = this.preferences.scale_percent();
                    if saved != *percent {
                        this.scale.update(cx, |state, cx| {
                            state.set_selected_value(&saved, window, cx);
                        });
                    }
                }
            },
        );
        let acrylic_opacity = cx.new(|cx| {
            SelectState::new(
                acrylic_opacity_options(preferences.acrylic_opacity_percent()),
                None,
                window,
                cx,
            )
        });
        acrylic_opacity.update(cx, |state, cx| {
            state.set_selected_value(&preferences.acrylic_opacity_percent(), window, cx);
        });
        let acrylic_opacity_subscription = cx.subscribe_in(
            &acrylic_opacity,
            window,
            |this, _, event: &SelectEvent<Vec<PercentOption>>, window, cx| {
                if let SelectEvent::Confirm(Some(percent)) = event {
                    this.change(
                        this.preferences.with_acrylic_opacity_percent(*percent),
                        window,
                        cx,
                    );
                    let saved = this.preferences.acrylic_opacity_percent();
                    if saved != *percent {
                        this.acrylic_opacity.update(cx, |state, cx| {
                            state.set_selected_value(&saved, window, cx);
                        });
                    }
                }
            },
        );
        let appearance_subscription = cx.observe_window_appearance(window, |this, window, cx| {
            if this.preferences.theme() == ThemePreference::System {
                appearance::apply_to_window(this.preferences, window, cx);
            }
            cx.notify();
        });
        Self {
            preferences,
            scale,
            acrylic_opacity,
            closing: false,
            error,
            _scale_subscription: scale_subscription,
            _acrylic_opacity_subscription: acrylic_opacity_subscription,
            _appearance_subscription: appearance_subscription,
        }
    }

    pub(crate) fn set_closing(&mut self, closing: bool, cx: &mut Context<Self>) {
        if self.closing == closing {
            return;
        }
        self.closing = closing;
        cx.notify();
    }

    fn change(
        &mut self,
        preferences: AppearancePreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closing || preferences == self.preferences {
            return;
        }
        // Tiny, synchronous atomic write: accepted work finishes before returning
        // to the event loop, so shutdown has no detached work to wait for.
        match cx.global_mut::<AppearanceState>().save(preferences) {
            Ok(()) => {
                self.preferences = preferences;
                self.error = None;
                appearance::apply_to_window(preferences, window, cx);
            }
            Err(error) => self.error = Some(format!("无法保存外观偏好：{error}")),
        }
        cx.notify();
    }

    fn sync_scale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let percent = self.preferences.scale_percent();
        self.scale.update(cx, |state, cx| {
            state.set_items(scale_options(percent), window, cx);
            state.set_selected_value(&percent, window, cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_custom_scale_is_an_exact_selectable_value_without_duplicates() {
        let custom = scale_options(117);
        assert_eq!(custom.iter().filter(|item| item.percent == 117).count(), 1);
        assert!(
            custom
                .windows(2)
                .all(|items| items[0].percent < items[1].percent)
        );
        let standard = scale_options(100);
        assert_eq!(
            standard.iter().filter(|item| item.percent == 100).count(),
            1
        );
    }
}
