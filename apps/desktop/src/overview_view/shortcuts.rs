//! 概览页工作区快捷入口卡片：聚合策略与节点、连接监控、规则管理与诊断排查。

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::Page;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{SurfaceLevel, section_title, surface};

pub(super) fn quick_shortcuts_card(page: Signal<Page>) -> AnyElement {
    surface(SurfaceLevel::Card)
        .test_id("overview-shortcuts")
        .p_5()
        .flex_col()
        .gap_3()
        .flex_1()
        .min_w(px(0.0))
        .child(section_title(|| t!("overview.next")))
        .child(shortcut_item(
            Icon::Proxies,
            "nav.proxies",
            "overview.task.proxies_hint",
            Page::Proxies,
            page,
        ))
        .child(shortcut_item(
            Icon::Connections,
            "nav.connections",
            "overview.task.connections_hint",
            Page::Connections,
            page,
        ))
        .child(shortcut_item(
            Icon::Rules,
            "overview.task.rules",
            "overview.task.rules_hint",
            Page::Rules,
            page,
        ))
        .child(shortcut_item(
            Icon::Activity,
            "overview.task.diagnose",
            "overview.task.diagnose_hint",
            Page::Diagnostics,
            page,
        ))
        .into_element()
}

fn shortcut_item(
    kind: Icon,
    title_key: &'static str,
    hint_key: &'static str,
    target: Page,
    page: Signal<Page>,
) -> AnyElement {
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .p_2()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .bg(move || theme().muted.opacity(0.35))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .flex_row()
                .items_center()
                .gap_3()
                .child(icon_sized(kind, Tone::Primary, 16.0))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex_col()
                        .child(
                            text(move || t!(title_key))
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM),
                        )
                        .child(
                            text(move || t!(hint_key))
                                .text_size(px(11.5))
                                .text_color(move || theme().muted_foreground)
                                .truncate(),
                        ),
                ),
        )
        .child(
            button(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .label(|| t!("common.open"))
                .on_press(move || page.set(target)),
        )
        .into_element()
}
