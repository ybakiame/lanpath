//! 「连接」监控视图：实时活跃/历史连接记录、多维筛选与排序、命中规则排查、
//! 一键写入直连/代理/拦截/自定义规则，以及断开单条或全部连接。

mod rows;
mod state;

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::Page;
use crate::app_state;
use crate::connections::{format_bytes, format_rate};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::feedback::empty_with_icon;
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::input;
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, page_header, surface};
use crate::ui::select::{segmented_group, segmented_radio};
use state::{ConnSort, ConnTab, ConnectionsState};

pub(crate) fn connections_page(page: Signal<Page>, rule_seed: Signal<String>) -> impl IntoElement {
    let state = ConnectionsState::new();
    state.start_polling();

    let app = app_state::current();
    let status = app.status;
    let traffic = app.traffic;

    let paused = state.paused;
    let tab = state.tab;
    let sort = state.sort;
    let search = state.search;
    let outbound_filter = state.outbound_filter;
    let error = state.error;

    let pause_btn_state = state.clone();
    let close_all_state = state.clone();
    let clear_state = state.clone();
    let filter_state = state.clone();
    let list_state = state.clone();
    let count_active = state.active;
    let count_closed = state.closed;

    scroll().flex_1().min_h(px(0.0)).child(
        view()
            .flex_col()
            .gap_5()
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_end()
                    .justify_between()
                    .gap_4()
                    .child(page_header(
                        || t!("connections.title"),
                        || t!("connections.subtitle"),
                    ))
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                button(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .test_id("connections-pause")
                                    .prefix(move || {
                                        icon_sized(
                                            if paused.get() { Icon::Play } else { Icon::Stop },
                                            Tone::Primary,
                                            13.0,
                                        )
                                    })
                                    .label(move || {
                                        if paused.get() {
                                            t!("connections.resume")
                                        } else {
                                            t!("connections.pause")
                                        }
                                    })
                                    .on_press(move || {
                                        let next = !pause_btn_state.paused.get();
                                        pause_btn_state.paused.set(next);
                                        if !next {
                                            pause_btn_state.refresh_now();
                                        }
                                    }),
                            )
                            .child(
                                button(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .test_id("connections-close-all")
                                    .disabled(move || {
                                        !status.get().running || count_active.with(Vec::is_empty)
                                    })
                                    .prefix(|| icon_sized(Icon::Close, Tone::Danger, 13.0))
                                    .label(|| t!("connections.close_all"))
                                    .on_press(move || close_all_state.close_all()),
                            )
                            .child(
                                button(ButtonVariant::Ghost)
                                    .size(ButtonSize::Sm)
                                    .test_id("connections-clear-history")
                                    .prefix(|| icon_sized(Icon::Trash, Tone::Muted, 13.0))
                                    .label(|| t!("connections.clear_history"))
                                    .on_press(move || clear_state.clear_history()),
                            ),
                    ),
            )
            .child(kpi_strip(
                traffic,
                state.active,
                state.upload_total,
                state.download_total,
            ))
            .child(when(
                move || error.get().is_some(),
                move || {
                    alert_banner(
                        AlertTone::Danger,
                        text(move || error.get().unwrap_or_default())
                            .text_size(px(12.5))
                            .text_color(move || theme().destructive),
                    )
                },
            ))
            .child(filter_bar(
                tab,
                sort,
                search,
                outbound_filter,
                count_active,
                count_closed,
                filter_state,
            ))
            .child(connections_list(list_state, status, page, rule_seed)),
    )
}

fn kpi_strip(
    traffic: Signal<app_state::TrafficSnapshot>,
    active: Signal<Vec<lanpath_client::ConnectionView>>,
    upload_total: Signal<u64>,
    download_total: Signal<u64>,
) -> AnyElement {
    view()
        .gap_3()
        .style_with(move |s| {
            if window().width.0 < 920.0 || font_scale() > 1.3 {
                s.flex_col()
            } else {
                s.flex_row()
            }
        })
        .child(kpi_card(
            "connections.kpi.active",
            move || active.with(|v| v.len().to_string()),
            move || {
                let t_sum = traffic.get().today;
                t!(
                    "connections.kpi.today_conns",
                    count = t_sum.connection_count
                )
            },
        ))
        .child(kpi_card(
            "connections.kpi.speed",
            move || {
                let snap = traffic.get();
                format!("↓ {}", format_rate(snap.down_bps))
            },
            move || {
                let snap = traffic.get();
                format!("↑ {}", format_rate(snap.up_bps))
            },
        ))
        .child(kpi_card(
            "connections.kpi.session_traffic",
            move || format_bytes(upload_total.get().saturating_add(download_total.get())),
            move || {
                format!(
                    "↑ {} · ↓ {}",
                    format_bytes(upload_total.get()),
                    format_bytes(download_total.get())
                )
            },
        ))
        .child(kpi_card(
            "connections.kpi.proxy_ratio",
            move || {
                let today = traffic.get().today;
                let total = today
                    .upload_bytes
                    .saturating_add(today.download_bytes)
                    .max(0) as u64;
                let proxied = today
                    .proxied_upload_bytes
                    .saturating_add(today.proxied_download_bytes)
                    .max(0) as u64;
                match proxied.saturating_mul(100).checked_div(total) {
                    Some(pct) => format!("{pct}%"),
                    None => "—".to_owned(),
                }
            },
            move || {
                let today = traffic.get().today;
                let total = today
                    .upload_bytes
                    .saturating_add(today.download_bytes)
                    .max(0) as u64;
                t!("connections.kpi.today_total", size = format_bytes(total))
            },
        ))
        .into_element()
}

fn kpi_card(
    title_key: &'static str,
    primary: impl Fn() -> String + 'static,
    secondary: impl Fn() -> String + 'static,
) -> AnyElement {
    surface(SurfaceLevel::Card)
        .flex_1()
        .min_w(px(0.0))
        .p_4()
        .flex_col()
        .gap_1()
        .child(
            text(move || t!(title_key))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            text(primary)
                .text_size(px(18.0))
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(
            text(secondary)
                .text_size(px(11.5))
                .text_color(move || theme().muted_foreground),
        )
        .into_element()
}

fn filter_bar(
    tab: Signal<ConnTab>,
    sort: Signal<ConnSort>,
    search: Signal<String>,
    outbound_filter: Signal<String>,
    active: Signal<Vec<lanpath_client::ConnectionView>>,
    closed: Signal<Vec<lanpath_client::ConnectionView>>,
    state: ConnectionsState,
) -> AnyElement {
    let outbounds_state = state.clone();
    view()
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(segmented_group(
                    "connections-tabs",
                    vec![
                        segmented_radio(
                            move || tab.get() == ConnTab::Active,
                            move || t!("connections.tab.active", count = active.with(Vec::len)),
                            "connections-tab-active",
                            move || {
                                tab.set(ConnTab::Active);
                                outbound_filter.set(String::new());
                            },
                        ),
                        segmented_radio(
                            move || tab.get() == ConnTab::Closed,
                            move || t!("connections.tab.closed", count = closed.with(Vec::len)),
                            "connections-tab-closed",
                            move || {
                                tab.set(ConnTab::Closed);
                                outbound_filter.set(String::new());
                            },
                        ),
                    ],
                ))
                .child(segmented_group(
                    "connections-sort",
                    vec![
                        segmented_radio(
                            move || sort.get() == ConnSort::Time,
                            || t!("connections.sort.time"),
                            "connections-sort-time",
                            move || sort.set(ConnSort::Time),
                        ),
                        segmented_radio(
                            move || sort.get() == ConnSort::Total,
                            || t!("connections.sort.total"),
                            "connections-sort-total",
                            move || sort.set(ConnSort::Total),
                        ),
                        segmented_radio(
                            move || sort.get() == ConnSort::Download,
                            || t!("connections.sort.download"),
                            "connections-sort-download",
                            move || sort.set(ConnSort::Download),
                        ),
                        segmented_radio(
                            move || sort.get() == ConnSort::Upload,
                            || t!("connections.sort.upload"),
                            "connections-sort-upload",
                            move || sort.set(ConnSort::Upload),
                        ),
                    ],
                )),
        )
        .child(
            view().flex_row().flex_wrap().items_center().gap_2().child(
                view().flex_1().min_w(px(220.0)).child(
                    input(search)
                        .test_id("connections-search")
                        .placeholder(|| t!("connections.search_placeholder")),
                ),
            ),
        )
        .child(when(
            move || !outbounds_state.distinct_outbounds().is_empty(),
            move || {
                let chip_state = state.clone();
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        button(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .test_id("connections-outbound-all")
                            .label(|| t!("connections.outbound_all"))
                            .on_press(move || outbound_filter.set(String::new())),
                    )
                    .child(each(
                        move || chip_state.distinct_outbounds(),
                        |tag| tag.clone(),
                        move |tag_item, _| {
                            let tag = tag_item.get();
                            let active_tag = tag.clone();
                            let click_tag = tag.clone();
                            let label_tag = tag.clone();
                            let variant = if outbound_filter.get() == active_tag {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            };
                            button(variant)
                                .size(ButtonSize::Sm)
                                .test_id(format!("connections-outbound-{tag}"))
                                .label(move || label_tag.clone())
                                .on_press(move || outbound_filter.set(click_tag.clone()))
                        },
                    ))
            },
        ))
        .into_element()
}

fn connections_list(
    state: ConnectionsState,
    status: Signal<app_state::RuntimeView>,
    page: Signal<Page>,
    rule_seed: Signal<String>,
) -> AnyElement {
    let is_empty_state = state.clone();
    let empty_state = state.clone();
    let list_state = state.clone();
    let row_state = state;
    view()
        .flex_col()
        .gap_2()
        .child(when(
            move || is_empty_state.filtered_list().is_empty(),
            move || {
                let running = status.get().running;
                let is_active_tab = empty_state.tab.get() == ConnTab::Active;
                surface(SurfaceLevel::Card).child(empty_with_icon(
                    Some(Icon::Connections),
                    move || {
                        if !running && is_active_tab {
                            t!("connections.empty_stopped_title")
                        } else {
                            t!("connections.empty_title")
                        }
                    },
                    move || {
                        if !running && is_active_tab {
                            t!("connections.empty_stopped_hint")
                        } else {
                            t!("connections.empty_hint")
                        }
                    },
                ))
            },
        ))
        .child(each(
            move || list_state.filtered_list(),
            |conn| conn.id.clone(),
            move |item, _| rows::connection_row(item.get(), row_state.clone(), page, rule_seed),
        ))
        .into_element()
}
