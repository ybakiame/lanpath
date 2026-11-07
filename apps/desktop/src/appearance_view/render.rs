use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    scroll::ScrollableElement as _,
    select::Select,
    switch::Switch,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use super::{ACCENTS, AppearanceView, THEMES};
use crate::{
    appearance::{self, AppearanceState, MaterialPreference},
    design,
};

impl AppearanceView {
    fn theme_choices(
        &self,
        disabled: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .gap_3()
            .children(THEMES.map(|(theme, id, label)| {
                let preview_dark = self
                    .preferences
                    .with_theme(theme)
                    .is_dark(window.appearance());
                let (background, surface, border) = appearance::preview_colors(preview_dark);
                let accent = appearance::accent_color(self.preferences.accent(), preview_dark);
                Button::new(id)
                    .ghost()
                    .h_auto()
                    .p_2()
                    .flex_1()
                    .min_w(gpui_kit::rems(9.))
                    .selected(self.preferences.theme() == theme)
                    .disabled(disabled)
                    .accessibility_label(format!("{label}主题"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.change(this.preferences.with_theme(theme), window, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .h_20()
                                    .w_full()
                                    .p_2()
                                    .gap_2()
                                    .rounded(cx.theme().radius)
                                    .border_1()
                                    .border_color(border)
                                    .bg(background)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .w_8()
                                            .gap_2()
                                            .pt_1()
                                            .child(div().h_1().w_5().rounded_sm().bg(border))
                                            .child(div().h_2().w_full().rounded_sm().bg(accent))
                                            .child(div().h_1().w_5().rounded_sm().bg(border)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .gap_2()
                                            .p_2()
                                            .rounded(cx.theme().radius)
                                            .bg(surface)
                                            .child(div().h_1().w_8().rounded_sm().bg(border))
                                            .child(div().h_1().w_full().rounded_sm().bg(border))
                                            .child(div().h_2().w_6().rounded_sm().bg(accent)),
                                    ),
                            )
                            .child(div().text_sm().font_medium().child(label)),
                    )
            }))
    }
}

impl Render for AppearanceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.closing || cx.global::<AppearanceState>().load_error().is_some();
        let dark = self.preferences.is_dark(window.appearance());
        let status = self.error.clone().unwrap_or_else(|| {
            format!(
                "已保存 · 当前为{}主题 · {}%",
                if dark { "深色" } else { "浅色" },
                self.preferences.scale_percent()
            )
        });
        let material_description = if !cfg!(windows) {
            "此平台使用实色背景；材质偏好会保留。"
        } else if self.preferences.effective_pure_black(window.appearance()) {
            "纯黑模式使用不透明背景；关闭纯黑后恢复所选材质。"
        } else {
            "亚克力使用 Windows 原生背景模糊；实色提供稳定、不透明的背景。"
        };
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
                            .min_w_0()
                            .w_full()
                            .max_w(gpui_kit::rems(60.))
                            .gap_4()
                            .pb_4()
                            .child(design::page_header(
                                "外观",
                                "让岚径适合你的工作环境",
                                Button::new("appearance-reload")
                                    .ghost()
                                    .small()
                                    .icon(IconName::RefreshCw)
                                    .label("重新载入")
                                    .disabled(self.closing)
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.reload(window, cx)),
                                    ),
                                cx,
                            ))
                            .child(design::settings_section(
                                "主题",
                                "选择明暗外观，或随系统自动切换。",
                                self.theme_choices(disabled, window, cx),
                                cx,
                            ))
                            .child(design::settings_section(
                                "强调色",
                                "用于当前选择、焦点和主要操作。",
                                div().flex().flex_wrap().gap_2().children(ACCENTS.map(
                                    |(accent, id, label)| {
                                        Button::new(id)
                                            .ghost()
                                            .selected(self.preferences.accent() == accent)
                                            .disabled(disabled)
                                            .accessibility_label(label)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.change(
                                                    this.preferences.with_accent(accent),
                                                    window,
                                                    cx,
                                                );
                                            }))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        div().size_3().rounded_full().bg(
                                                            appearance::accent_color(accent, dark),
                                                        ),
                                                    )
                                                    .child(label),
                                            )
                                    },
                                )),
                                cx,
                            ))
                            .child(design::settings_section(
                                "窗口材质",
                                material_description,
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .child(
                                        ButtonGroup::new("appearance-material")
                                            .compact()
                                            .children(
                                                [
                                                    (
                                                        MaterialPreference::Acrylic,
                                                        "亚克力",
                                                        IconName::Layers,
                                                    ),
                                                    (
                                                        MaterialPreference::Solid,
                                                        "实色",
                                                        IconName::Square,
                                                    ),
                                                ]
                                                .map(|(material, label, icon)| {
                                                    Button::new(format!(
                                                        "appearance-material-{material:?}"
                                                    ))
                                                    .label(label)
                                                    .icon(icon)
                                                    .selected(
                                                        self.preferences.material() == material,
                                                    )
                                                    .disabled(disabled || !cfg!(windows))
                                                }),
                                            )
                                            .on_click(cx.listener(
                                                |this, selected: &Vec<usize>, window, cx| {
                                                    let material = match selected.first() {
                                                        Some(0) => MaterialPreference::Acrylic,
                                                        Some(1) => MaterialPreference::Solid,
                                                        _ => return,
                                                    };
                                                    this.change(
                                                        this.preferences.with_material(material),
                                                        window,
                                                        cx,
                                                    );
                                                },
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_4()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child("纯黑背景")
                                                    .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(
                                                            "仅在深色主题生效，浅色时保留偏好。",
                                                        ),
                                                ),
                                            )
                                            .child(
                                                Switch::new("appearance-pure-black")
                                                    .accessibility_label("纯黑背景")
                                                    .checked(self.preferences.pure_black())
                                                    .disabled(disabled || !dark)
                                                    .on_change(cx.listener(
                                                        |this, checked: &bool, window, cx| {
                                                            if this
                                                                .preferences
                                                                .is_dark(window.appearance())
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
                                            ),
                                    ),
                                cx,
                            ))
                            .child(design::settings_section(
                                "界面缩放",
                                "文字、控件与间距一起缩放。",
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_wrap()
                                    .gap_3()
                                    .child(
                                        div().w_40().flex_shrink_0().child(
                                            Select::new(&self.scale)
                                                .id("appearance-scale")
                                                .accessibility_label("界面缩放百分比")
                                                .disabled(disabled)
                                                .w_full(),
                                        ),
                                    )
                                    .child(
                                        Button::new("appearance-scale-reset")
                                            .ghost()
                                            .small()
                                            .label("恢复 100%")
                                            .disabled(
                                                disabled || self.preferences.scale_percent() == 100,
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.change(
                                                    this.preferences.with_scale_percent(100),
                                                    window,
                                                    cx,
                                                );
                                                this.sync_scale(window, cx);
                                            })),
                                    ),
                                cx,
                            ))
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
                            ),
                    ),
            )
    }
}
