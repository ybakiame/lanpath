//! 精选社区规则集市场面板：镜像源切换、分类筛选与一键添加下载。

use lanpath_client::rulesets::{MarketMirror, MarketRuleSetSpec, market_catalog};
use nsh_base::theme;
use nsh_core::prelude::*;

use super::managed::filter_toolbar;
use super::state::{RulesetsState, category_label};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{SurfaceLevel, surface};
use crate::ui::select::{segmented_group, segmented_radio};
use crate::ui::{BadgeVariant, badge};

pub(super) fn market_panel(state: RulesetsState) -> AnyElement {
    let search = state.search;
    let category = state.category;
    let mirror = state.mirror;

    let filtered = computed(move || {
        let query = search.get().trim().to_lowercase();
        let cat = category.get();
        market_catalog()
            .iter()
            .copied()
            .filter(|spec| {
                let cat_ok = cat == "all" || spec.category.eq_ignore_ascii_case(&cat);
                let query_ok = query.is_empty()
                    || spec.tag.to_lowercase().contains(&query)
                    || spec.name_zh.to_lowercase().contains(&query)
                    || spec.name_en.to_lowercase().contains(&query)
                    || spec.description_zh.to_lowercase().contains(&query)
                    || spec.description_en.to_lowercase().contains(&query);
                cat_ok && query_ok
            })
            .collect::<Vec<_>>()
    });

    let card_state = state;

    view()
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
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            text(|| t!("rulesets.market.mirror"))
                                .text_size(px(12.5))
                                .text_color(move || theme().muted_foreground),
                        )
                        .child(segmented_group(
                            "rulesets-mirror",
                            vec![
                                segmented_radio(
                                    move || mirror.get() == MarketMirror::JsDelivr,
                                    || "jsDelivr CDN".to_owned(),
                                    "rulesets-mirror-jsdelivr",
                                    move || mirror.set(MarketMirror::JsDelivr),
                                ),
                                segmented_radio(
                                    move || mirror.get() == MarketMirror::GitHubRaw,
                                    || "GitHub Raw".to_owned(),
                                    "rulesets-mirror-github",
                                    move || mirror.set(MarketMirror::GitHubRaw),
                                ),
                            ],
                        )),
                )
                .child(
                    text(|| t!("rulesets.market.source_note"))
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(filter_toolbar(search, category))
        .child(view().flex_row().flex_wrap().gap_3().child(each(
            move || filtered.get(),
            |spec: &MarketRuleSetSpec| spec.tag.to_owned(),
            move |spec, _| market_card(card_state.clone(), spec.get_untracked()),
        )))
        .into_element()
}

fn market_card(state: RulesetsState, spec: MarketRuleSetSpec) -> AnyElement {
    let items = state.items;
    let added = move || {
        items.with(|list| {
            list.iter()
                .any(|row| row.entry.tag.eq_ignore_ascii_case(spec.tag))
        })
    };
    let zh = move || t!("nav.rulesets") == "规则集";
    let busy_state = state.clone();
    let add_state = state;

    surface(SurfaceLevel::Card)
        .w(px(340.0))
        .flex_grow(1.0)
        .p_4()
        .flex_col()
        .justify_between()
        .gap_3()
        .child(
            view()
                .flex_col()
                .gap_2()
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            text(move || {
                                if zh() {
                                    spec.name_zh.to_owned()
                                } else {
                                    spec.name_en.to_owned()
                                }
                            })
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || theme().foreground),
                        )
                        .child(badge(BadgeVariant::Default, move || {
                            category_label(spec.category)
                        })),
                )
                .child(
                    view()
                        .flex_row()
                        .child(badge(BadgeVariant::Outline, move || spec.tag.to_owned())),
                )
                .child(
                    text(move || {
                        if zh() {
                            spec.description_zh.to_owned()
                        } else {
                            spec.description_en.to_owned()
                        }
                    })
                    .text_size(px(12.5))
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            view().flex_row().items_center().justify_end().child(
                when(added, || {
                    badge(BadgeVariant::Success, || t!("rulesets.market.added"))
                })
                .otherwise(move || {
                    let st = add_state.clone();
                    let b = busy_state.clone();
                    button(ButtonVariant::Primary)
                        .test_id(format!("market-add-{}", spec.tag))
                        .size(ButtonSize::Sm)
                        .disabled(move || b.is_busy())
                        .on_press(move || st.add_from_market(spec))
                        .prefix(|| icon_sized(Icon::Download, Tone::OnPrimary, 13.0).into_element())
                        .label(|| t!("rulesets.market.add_download"))
                }),
            ),
        )
        .into_element()
}
