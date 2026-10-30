use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::Button,
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement as _,
    searchable_list::SearchableListItem,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Role, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    div,
};

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
struct ScaleOption {
    percent: u16,
    title: SharedString,
}

impl SearchableListItem for ScaleOption {
    type Value = u16;
    fn title(&self) -> SharedString {
        self.title.clone()
    }
    fn value(&self) -> &Self::Value {
        &self.percent
    }
}

fn scale_options(current: u16) -> Vec<ScaleOption> {
    let mut values = vec![75, 90, 100, 110, 125, 150];
    if !values.contains(&current) {
        values.push(current);
        values.sort_unstable();
    }
    values
        .into_iter()
        .map(|percent| ScaleOption {
            percent,
            title: format!("{percent}%").into(),
        })
        .collect()
}

pub(crate) struct AppearanceView {
    preferences: AppearancePreferences,
    scale: Entity<SelectState<Vec<ScaleOption>>>,
    closing: bool,
    error: Option<String>,
    _scale_subscription: Subscription,
    _appearance_subscription: Subscription,
}

impl AppearanceView {
    /// The application initializes AppearanceState before opening this window.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppearanceState>();
        let preferences = state.preferences();
        let error = state.load_error().map(str::to_owned);
        // Use the actual window appearance once available (notably on Linux).
        appearance::apply(preferences, window.appearance(), cx);
        let scale = cx.new(|cx| {
            SelectState::new(scale_options(preferences.scale_percent()), None, window, cx)
        });
        scale.update(cx, |state, cx| {
            state.set_selected_value(&preferences.scale_percent(), window, cx);
        });
        let scale_subscription = cx.subscribe_in(
            &scale,
            window,
            |this, _, event: &SelectEvent<Vec<ScaleOption>>, window, cx| {
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
        let appearance_subscription = cx.observe_window_appearance(window, |this, window, cx| {
            if this.preferences.theme() == ThemePreference::System {
                appearance::apply(this.preferences, window.appearance(), cx);
            }
            cx.notify();
        });
        Self {
            preferences,
            scale,
            closing: false,
            error,
            _scale_subscription: scale_subscription,
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
                appearance::apply(preferences, window.appearance(), cx);
            }
            Err(error) => self.error = Some(format!("无法保存外观偏好：{error}")),
        }
        cx.notify();
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        match cx.global_mut::<AppearanceState>().reload() {
            Ok(()) => {
                self.preferences = cx.global::<AppearanceState>().preferences();
                self.error = None;
                self.sync_scale(window, cx);
                appearance::apply(self.preferences, window.appearance(), cx);
            }
            Err(error) => self.error = Some(format!("无法载入外观偏好：{error}")),
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

impl Render for AppearanceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.closing || cx.global::<AppearanceState>().load_error().is_some();
        let dark = self.preferences.is_dark(window.appearance());
        let status = self.error.clone().unwrap_or_else(|| {
            format!(
                "更改立即保存到实验目录 · 当前{} · 缩放 {}%",
                if dark { "深色" } else { "浅色" },
                self.preferences.scale_percent()
            )
        });
        div()
            .id("appearance-page")
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .id("appearance-scroll")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_shrink_0()
                            .min_w_0()
                            .w_full()
                            .max_w(gpui_kit::rems(64.0))
                            .gap_6()
                            .p_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_wrap()
                                    .gap_3()
                                    .child(div().text_lg().font_semibold().child("外观"))
                                    .child(
                                        Button::new("appearance-reload")
                                            .label("重新载入外观")
                                            .disabled(self.closing)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.reload(window, cx);
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("appearance-status")
                                    .role(Role::Status)
                                    .aria_label(status.clone())
                                    .text_sm()
                                    .text_color(if self.error.is_some() {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(status),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(div().font_semibold().child("主题"))
                                    .child(
                                        RadioGroup::horizontal("appearance-theme")
                                            .selected_index(THEMES.iter().position(
                                                |(value, _, _)| *value == self.preferences.theme(),
                                            ))
                                            .disabled(disabled)
                                            .children(THEMES.iter().map(|(_, id, label)| {
                                                Radio::new(*id)
                                                    .label(*label)
                                                    .accessibility_label(*label)
                                            }))
                                            .on_change(cx.listener(
                                                |this, ix: &usize, window, cx| {
                                                    if let Some((theme, _, _)) = THEMES.get(*ix) {
                                                        this.change(
                                                            this.preferences.with_theme(*theme),
                                                            window,
                                                            cx,
                                                        );
                                                    }
                                                },
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(div().font_semibold().child("强调色"))
                                    .child(
                                        RadioGroup::horizontal("appearance-accent")
                                            .selected_index(ACCENTS.iter().position(
                                                |(value, _, _)| *value == self.preferences.accent(),
                                            ))
                                            .disabled(disabled)
                                            .children(ACCENTS.iter().map(|(_, id, label)| {
                                                Radio::new(*id)
                                                    .label(*label)
                                                    .accessibility_label(*label)
                                            }))
                                            .on_change(cx.listener(
                                                |this, ix: &usize, window, cx| {
                                                    if let Some((accent, _, _)) = ACCENTS.get(*ix) {
                                                        this.change(
                                                            this.preferences.with_accent(*accent),
                                                            window,
                                                            cx,
                                                        );
                                                    }
                                                },
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        Switch::new("appearance-pure-black")
                                            .label("纯黑背景")
                                            .checked(self.preferences.pure_black())
                                            .disabled(disabled || !dark)
                                            .on_change(cx.listener(
                                                |this, checked: &bool, window, cx| {
                                                    if this.preferences.is_dark(window.appearance())
                                                    {
                                                        this.change(
                                                            this.preferences
                                                                .with_pure_black(*checked),
                                                            window,
                                                            cx,
                                                        );
                                                    }
                                                },
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("仅在深色主题生效；切换到浅色时保留选择。"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().font_semibold().child("界面缩放"))
                                    .child(
                                        Select::new(&self.scale)
                                            .id("appearance-scale")
                                            .accessibility_label("界面缩放百分比")
                                            .disabled(disabled)
                                            .w_40(),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("同时缩放文字、控件和间距。"),
                                    ),
                            ),
                    ),
            )
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
