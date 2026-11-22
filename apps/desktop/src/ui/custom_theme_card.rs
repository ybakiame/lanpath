//! 外观页「自定义配色（OKLCH）」组件：
//! 开放 4 个核心种子色（主色 / 强调色 / 背景 / 文字），每个色槽点击弹出原生
//! `nsh_base::ColorPicker`（Popover 浮层，不拉高页面密度）：
//! - `ColorArea` 二维色域面板
//! - `ColorSlider` 原生 OKLCH 三通道渐变滑条（亮度 L / 彩度 C / 色相 H）
//! - `ColorField`（默认 OKLCH 格式，同时兼容 `#hex` / `rgb()` / `hsl()` 输入）
//! - `ColorSwatch` 精选快捷色点 + 底部「从预设导入微调」与「应用 / 重置」操作。

use nsh_base::{
    ColorChannel, ColorFormat, ColorPickerEvent, ColorSpace, PopoverEvent, color_area, color_field,
    color_picker, color_slider, color_swatch, popover as base_popover, theme,
};
use nsh_core::prelude::*;

use super::button::{ButtonSize, ButtonVariant, button};
use super::feedback::show_toast;
use super::icon::{Icon, Tone, icon_sized};
use super::layout::{SurfaceLevel, section_title, surface};
use super::prefs::{self, UiPrefs};
use super::select::{popup_ending, popup_starting, popup_surface};
use super::theme::{
    CustomPalette, ThemeId,
    oklch::{hsla_to_hex, hsla_to_oklch},
    preset_seed,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorSlot {
    Primary,
    Accent,
    Background,
    Foreground,
}

impl ColorSlot {
    const ALL: [ColorSlot; 4] = [
        ColorSlot::Primary,
        ColorSlot::Accent,
        ColorSlot::Background,
        ColorSlot::Foreground,
    ];

    fn label_key(self) -> &'static str {
        match self {
            ColorSlot::Primary => "appearance.custom.slot.primary",
            ColorSlot::Accent => "appearance.custom.slot.accent",
            ColorSlot::Background => "appearance.custom.slot.background",
            ColorSlot::Foreground => "appearance.custom.slot.foreground",
        }
    }

    fn trigger_id(self) -> &'static str {
        match self {
            ColorSlot::Primary => "custom-color-primary",
            ColorSlot::Accent => "custom-color-accent",
            ColorSlot::Background => "custom-color-background",
            ColorSlot::Foreground => "custom-color-foreground",
        }
    }

    fn popover_id(self) -> &'static str {
        match self {
            ColorSlot::Primary => "custom-picker-primary",
            ColorSlot::Accent => "custom-picker-accent",
            ColorSlot::Background => "custom-picker-background",
            ColorSlot::Foreground => "custom-picker-foreground",
        }
    }

    fn field_id(self) -> &'static str {
        match self {
            ColorSlot::Primary => "custom-field-primary",
            ColorSlot::Accent => "custom-field-accent",
            ColorSlot::Background => "custom-field-background",
            ColorSlot::Foreground => "custom-field-foreground",
        }
    }

    fn get(self, p: CustomPalette) -> Hsla {
        match self {
            ColorSlot::Primary => p.primary,
            ColorSlot::Accent => p.accent,
            ColorSlot::Background => p.background,
            ColorSlot::Foreground => p.foreground,
        }
    }

    fn set(self, mut p: CustomPalette, color: Hsla) -> CustomPalette {
        match self {
            ColorSlot::Primary => p.primary = color,
            ColorSlot::Accent => p.accent = color,
            ColorSlot::Background => p.background = color,
            ColorSlot::Foreground => p.foreground = color,
        }
        p
    }
}

fn format_oklch_short(color: Hsla) -> String {
    let ok = hsla_to_oklch(color);
    format!("oklch({:.0}% {:.2} {:.0})", ok.l * 100.0, ok.c, ok.h)
}

/// 外观页自定义配色卡片：4 个核心色槽（Popover 原生 OKLCH ColorPicker）+ 预设导入 + 应用/重置。
pub fn custom_theme_card() -> AnyElement {
    let ui_prefs = prefs::prefs();
    let draft = signal(ui_prefs.custom_palette.get_untracked());
    let active_error = signal(None::<String>);

    // 离开页面时恢复已持久化的生效主题，避免未保存草稿残留。
    cleanup(move || ui_prefs.restore_applied_theme());

    let preset_swatches: Vec<Hsla> = ThemeId::PRESETS
        .iter()
        .flat_map(|id| {
            let s = preset_seed(*id);
            [s.primary, s.accent]
        })
        .collect();

    surface(SurfaceLevel::Card)
        .test_id("custom-theme-card")
        .p_5()
        .flex_col()
        .gap_4()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    view()
                        .flex_col()
                        .gap_1()
                        .child(section_title(|| t!("appearance.custom.title")))
                        .child(
                            text(|| t!("appearance.custom.subtitle"))
                                .text_size(px(12.0))
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            button(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .test_id("custom-theme-reset")
                                .label(|| t!("appearance.custom.reset"))
                                .on_press(move || {
                                    let base = preset_seed(ThemeId::Lanpath);
                                    draft.set(base);
                                    ui_prefs.preview_custom_palette(base);
                                    active_error.set(None);
                                }),
                        )
                        .child(
                            button(ButtonVariant::Primary)
                                .size(ButtonSize::Sm)
                                .test_id("custom-theme-apply")
                                .prefix(|| icon_sized(Icon::Sparkles, Tone::OnPrimary, 13.0))
                                .label(|| t!("appearance.custom.apply"))
                                .on_press(move || {
                                    let next = draft.get_untracked();
                                    match ui_prefs.apply_custom_palette(next) {
                                        Ok(()) => {
                                            active_error.set(None);
                                            show_toast(t!("appearance.custom.applied"));
                                        }
                                        Err(_) => {
                                            ui_prefs.restore_applied_theme();
                                            active_error.set(Some(t!("appearance.save_failed")));
                                        }
                                    }
                                }),
                        ),
                ),
        )
        .child(
            view()
                .grid()
                .w_full()
                .grid_cols(move || if window().width.0 < 860.0 { 2_u16 } else { 4 })
                .gap_3()
                .children(
                    ColorSlot::ALL
                        .iter()
                        .map(|&slot| color_slot_popover(slot, draft, ui_prefs, &preset_swatches)),
                ),
        )
        .child(preset_import_row(draft, ui_prefs))
        .child(when(
            move || active_error.get().is_some(),
            move || {
                text(move || active_error.get().unwrap_or_default())
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive)
            },
        ))
        .into_element()
}

fn color_slot_popover(
    slot: ColorSlot,
    draft: Signal<CustomPalette>,
    ui_prefs: UiPrefs,
    swatches: &[Hsla],
) -> AnyElement {
    let open = signal(false);
    let swatch_list = swatches.to_vec();

    let trigger = nsh_base::button()
        .w_full()
        .p(px(10.0))
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || {
            if open.get() {
                theme().primary
            } else {
                theme().border
            }
        })
        .bg(move || theme().background)
        .hover(|s| s.border_color(move || theme().ring))
        .cursor(Cursor::Default)
        .child(
            view()
                .test_id(slot.trigger_id())
                .w_full()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .child(
                    view()
                        .size(px(28.0))
                        .flex_none()
                        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .bg(move || AdaptiveColor::from(slot.get(draft.get()))),
                )
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            text(move || t!(slot.label_key()))
                                .text_size(px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || theme().foreground)
                                .truncate(),
                        )
                        .child(
                            text(move || hsla_to_hex(slot.get(draft.get())))
                                .text_size(px(11.0))
                                .text_color(move || theme().muted_foreground)
                                .truncate(),
                        ),
                ),
        );

    let popup = view()
        .test_id(slot.popover_id())
        .w(px(292.0))
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    text(move || t!(slot.label_key()))
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().popover_foreground),
                )
                .child(
                    text(move || format_oklch_short(slot.get(draft.get())))
                        .text_size(px(11.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            color_picker()
                .value(move || Some(slot.get(draft.get())))
                .format(ColorFormat::Oklch)
                .on_change(move |ev: &ColorPickerEvent| {
                    let color = match ev {
                        ColorPickerEvent::Change(c, _) | ColorPickerEvent::ChangeEnd(c, _) => *c,
                    };
                    let next = slot.set(draft.get_untracked(), color);
                    draft.set(next);
                    ui_prefs.preview_custom_palette(next);
                })
                .child(
                    view()
                        .w_full()
                        .flex_col()
                        .gap_3()
                        .child(
                            color_area()
                                .w_full()
                                .h(px(132.0))
                                .panel_class("rounded-md")
                                .thumb_class("size-4 rounded-full border-2 border-white shadow")
                                .fill_thumb(true),
                        )
                        .child(oklch_channel_slider(
                            "appearance.custom.channel.l",
                            ColorChannel::Lightness,
                        ))
                        .child(oklch_channel_slider(
                            "appearance.custom.channel.c",
                            ColorChannel::Chroma,
                        ))
                        .child(oklch_channel_slider(
                            "appearance.custom.channel.h",
                            ColorChannel::Hue,
                        ))
                        .child(
                            view().test_id(slot.field_id()).w_full().child(
                                color_field()
                                    .format(ColorFormat::Oklch)
                                    .placeholder("oklch(55% 0.14 250) / #245db5")
                                    .w_full()
                                    .h(px(32.0))
                                    .px_2()
                                    .text_size(px(12.0))
                                    .rounded(move || {
                                        theme().number("radius-sm").map(px).unwrap_or(px(4.0))
                                    })
                                    .border(px(1.0))
                                    .border_color(move || theme().input)
                                    .bg(move || theme().background)
                                    .text_color(move || theme().foreground),
                            ),
                        )
                        .child(view().flex_row().flex_wrap().gap(px(6.0)).children(
                            swatch_list.into_iter().map(|chip| {
                                color_swatch()
                                    .color(chip)
                                    .size(px(18.0))
                                    .rounded_full()
                                    .border(px(1.0))
                                    .border_color(move || theme().border)
                            }),
                        )),
                ),
        );

    base_popover()
        .open(move || Some(open.get()))
        .placement(Placement::Bottom)
        .align(Align::Start)
        .offset(px(6.0))
        .margin(px(8.0))
        .on_open_change(move |ev: &PopoverEvent| {
            if let PopoverEvent::OpenChange(next, _) = ev {
                open.set(*next);
            }
        })
        .popup_style(|s| popup_surface(s).p_3())
        .popup_starting_style(popup_starting)
        .popup_ending_style(popup_ending)
        .trigger(trigger)
        .child(popup)
        .into_element()
}

fn oklch_channel_slider(label_key: &'static str, channel: ColorChannel) -> AnyElement {
    view()
        .w_full()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            text(move || t!(label_key))
                .w(px(42.0))
                .flex_none()
                .text_size(px(11.5))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            color_slider()
                .color_space(ColorSpace::Oklch)
                .channel(channel)
                .flex_1()
                .h(px(22.0))
                .justify_center()
                .px(px(6.0))
                .track_class("h-2.5 rounded-full")
                .thumb_class("size-4 rounded-full border-2 border-white shadow")
                .fill_thumb(true),
        )
        .into_element()
}

fn preset_import_row(draft: Signal<CustomPalette>, ui_prefs: UiPrefs) -> AnyElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(
            text(|| t!("appearance.custom.load_preset"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .children(ThemeId::PRESETS.iter().map(|&preset| {
            let seed = preset_seed(preset);
            let dot_color = super::theme::theme_tokens(preset)
                .color("primary")
                .unwrap_or_else(|| AdaptiveColor::from(seed.primary));
            button(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .test_id(format!("custom-import-{}", preset.id_name()))
                .prefix(move || {
                    view()
                        .size(px(10.0))
                        .rounded_full()
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .bg(dot_color)
                        .into_element()
                })
                .label(move || preset.id_name().to_owned())
                .on_press(move || {
                    draft.set(seed);
                    ui_prefs.preview_custom_palette(seed);
                })
        }))
        .into_element()
}
