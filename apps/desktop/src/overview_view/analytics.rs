//! 概览页数据可视化面板：使用 NSH 原生 `Plot` 与 `Canvas` 展示实时流量曲线、
//! 代理/直连比例环形图、近 7 日流量柱状图以及高频快捷入口。

use nsh_base::plot::{BandScale, BarCornerRadii, Curve, Scale, area, bar, grid, line, plot};
use nsh_base::theme;
use nsh_core::element::{RowAccessor, Rows};
use nsh_core::prelude::*;

use super::{shortcuts, visuals};
use crate::Page;
use crate::app_state::{AppState, TrafficSnapshot};
use crate::connections::{format_bytes, format_rate};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{SurfaceLevel, section_title, surface};

pub(super) fn dashboard(state: AppState, page: Signal<Page>) -> AnyElement {
    let traffic = state.traffic;
    view()
        .flex_col()
        .gap_5()
        .child(
            view()
                .gap_5()
                .style_with(move |s| {
                    if window().width.0 < 1040.0 || font_scale() > 1.3 {
                        s.flex_col()
                    } else {
                        s.flex_row()
                    }
                })
                .child(realtime_traffic_card(traffic, page))
                .child(proxy_ratio_card(traffic)),
        )
        .child(
            view()
                .gap_5()
                .style_with(move |s| {
                    if window().width.0 < 1040.0 || font_scale() > 1.3 {
                        s.flex_col()
                    } else {
                        s.flex_row()
                    }
                })
                .child(daily_usage_card(traffic))
                .child(shortcuts::quick_shortcuts_card(page)),
        )
        .into_element()
}

fn realtime_traffic_card(traffic: Signal<TrafficSnapshot>, page: Signal<Page>) -> AnyElement {
    surface(SurfaceLevel::Card)
        .test_id("overview-traffic-chart")
        .p_5()
        .flex_col()
        .gap_3()
        .flex_1()
        .min_w(px(0.0))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    view()
                        .flex_col()
                        .gap_1()
                        .child(section_title(|| t!("overview.analytics.traffic_title")))
                        .child(
                            text(move || {
                                let snap = traffic.get();
                                t!(
                                    "overview.analytics.traffic_live",
                                    down = format_rate(snap.down_bps),
                                    up = format_rate(snap.up_bps)
                                )
                            })
                            .text_size(px(12.5))
                            .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(
                    button(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .test_id("overview-goto-connections")
                        .prefix(|| icon_sized(Icon::Connections, Tone::Primary, 13.0))
                        .label(|| t!("overview.analytics.view_connections"))
                        .on_press(move || page.set(Page::Connections)),
                ),
        )
        .child(
            view().h(px(152.0)).w_full().child(
                plot()
                    .w_full()
                    .h_full()
                    .interactive(false)
                    .data(move || {
                        let history = traffic.get().history;
                        let n = history.len().max(2);
                        let downs: Vec<f64> = history.iter().map(|p| p.down_bps).collect();
                        let ups: Vec<f64> = history.iter().map(|p| p.up_bps).collect();
                        Rows::from_columns(n, [("down".into(), downs), ("up".into(), ups)])
                    })
                    .x_scale(Some(Scale::Point { reverse: false }))
                    .y_scale(move || {
                        let max_val = traffic
                            .get()
                            .history
                            .iter()
                            .fold(0.0_f64, |acc, p| acc.max(p.down_bps).max(p.up_bps));
                        Some(Scale::Linear {
                            domain: Some([0.0, (max_val * 1.15).max(1024.0)]),
                            reverse: false,
                        })
                    })
                    .child(
                        grid()
                            .ticks(Some(3))
                            .stroke_color(move || theme().border.opacity(0.6))
                            .stroke_width(px(1.0)),
                    )
                    .child(
                        area()
                            .y1(Some(RowAccessor::from("down")))
                            .curve(Curve::Natural)
                            .fill_color(move || theme().primary.opacity(0.16))
                            .stroke_none(),
                    )
                    .child(
                        line()
                            .y(Some(RowAccessor::from("down")))
                            .curve(Curve::Natural)
                            .stroke_color(move || theme().primary)
                            .stroke_width(px(2.0)),
                    )
                    .child(
                        area()
                            .y1(Some(RowAccessor::from("up")))
                            .curve(Curve::Natural)
                            .fill_color(move || {
                                theme()
                                    .color("success")
                                    .unwrap_or(theme().accent_foreground)
                                    .opacity(0.14)
                            })
                            .stroke_none(),
                    )
                    .child(
                        line()
                            .y(Some(RowAccessor::from("up")))
                            .curve(Curve::Natural)
                            .stroke_color(move || {
                                theme()
                                    .color("success")
                                    .unwrap_or(theme().accent_foreground)
                            })
                            .stroke_width(px(1.75)),
                    ),
            ),
        )
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
                        .gap_4()
                        .child(legend_dot(
                            move || theme().primary,
                            || t!("overview.analytics.download"),
                        ))
                        .child(legend_dot(
                            move || {
                                theme()
                                    .color("success")
                                    .unwrap_or(theme().accent_foreground)
                            },
                            || t!("overview.analytics.upload"),
                        )),
                )
                .child(
                    text(move || {
                        let snap = traffic.get();
                        t!(
                            "overview.analytics.active_conns_count",
                            count = snap.active_count
                        )
                    })
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .into_element()
}

fn proxy_ratio_card(traffic: Signal<TrafficSnapshot>) -> AnyElement {
    surface(SurfaceLevel::Card)
        .test_id("overview-ratio-card")
        .p_5()
        .flex_col()
        .gap_3()
        .flex_1()
        .min_w(px(0.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(section_title(|| t!("overview.analytics.ratio_title")))
                .child(
                    text(move || {
                        let d = traffic.get().today.date;
                        if d.is_empty() {
                            t!("overview.analytics.today")
                        } else {
                            d
                        }
                    })
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_5()
                .child(visuals::proxy_ratio_canvas(traffic))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(150.0))
                        .flex_col()
                        .gap_2()
                        .child(ratio_stat_row(
                            move || theme().primary,
                            || t!("overview.analytics.proxied"),
                            move || {
                                let t_sum = traffic.get().today;
                                let bytes = t_sum
                                    .proxied_upload_bytes
                                    .saturating_add(t_sum.proxied_download_bytes)
                                    .max(0) as u64;
                                format_bytes(bytes)
                            },
                        ))
                        .child(ratio_stat_row(
                            move || {
                                theme()
                                    .color("success")
                                    .unwrap_or(theme().secondary_foreground)
                            },
                            || t!("overview.analytics.direct"),
                            move || {
                                let t_sum = traffic.get().today;
                                let total = t_sum
                                    .upload_bytes
                                    .saturating_add(t_sum.download_bytes)
                                    .max(0) as u64;
                                let proxied = t_sum
                                    .proxied_upload_bytes
                                    .saturating_add(t_sum.proxied_download_bytes)
                                    .max(0) as u64;
                                format_bytes(total.saturating_sub(proxied))
                            },
                        ))
                        .child(ratio_stat_row(
                            move || theme().muted_foreground,
                            || t!("overview.analytics.today_conns"),
                            move || traffic.get().today.connection_count.to_string(),
                        ))
                        .child(ratio_stat_row(
                            move || theme().muted_foreground,
                            || t!("overview.analytics.today_targets"),
                            move || traffic.get().today.target_count.to_string(),
                        )),
                ),
        )
        .into_element()
}

fn daily_usage_card(traffic: Signal<TrafficSnapshot>) -> AnyElement {
    let mut band = BandScale::default();
    band.padding_inner = Percentage(0.32);
    band.padding_outer = Percentage(0.14);
    let band_scale = Scale::Band(band);

    let mut top_radii = BarCornerRadii::default();
    top_radii.top_left = px(4.0);
    top_radii.top_right = px(4.0);

    surface(SurfaceLevel::Card)
        .test_id("overview-daily-chart")
        .p_5()
        .flex_col()
        .gap_3()
        .flex_1()
        .min_w(px(0.0))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(section_title(|| t!("overview.analytics.daily_title")))
                .child(
                    text(move || {
                        let sum: u64 = traffic
                            .get()
                            .recent_days
                            .iter()
                            .map(|d| d.upload_bytes.saturating_add(d.download_bytes).max(0) as u64)
                            .sum();
                        t!("overview.analytics.daily_total", size = format_bytes(sum))
                    })
                    .text_size(px(12.5))
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            view().h(px(144.0)).w_full().child(
                plot()
                    .w_full()
                    .h_full()
                    .interactive(false)
                    .data(move || {
                        let days = traffic.get().recent_days;
                        if days.is_empty() {
                            return Rows::from_columns(
                                7,
                                [
                                    ("proxied".into(), vec![0.0; 7]),
                                    ("direct".into(), vec![0.0; 7]),
                                ],
                            );
                        }
                        let proxied: Vec<f64> =
                            days.iter().map(|d| d.proxied_bytes.max(0) as f64).collect();
                        let direct: Vec<f64> =
                            days.iter().map(|d| d.direct_bytes.max(0) as f64).collect();
                        Rows::from_columns(
                            days.len(),
                            [("proxied".into(), proxied), ("direct".into(), direct)],
                        )
                    })
                    .x_scale(Some(band_scale))
                    .y_scale(move || {
                        let max_day = traffic.get().recent_days.iter().fold(0.0_f64, |acc, d| {
                            let total =
                                d.upload_bytes.saturating_add(d.download_bytes).max(0) as f64;
                            acc.max(total)
                        });
                        Some(Scale::Linear {
                            domain: Some([0.0, (max_day * 1.15).max(1024.0)]),
                            reverse: false,
                        })
                    })
                    .child(
                        grid()
                            .ticks(Some(3))
                            .stroke_color(move || theme().border.opacity(0.55))
                            .stroke_width(px(1.0)),
                    )
                    .child(
                        bar()
                            .value(Some(RowAccessor::from("proxied")))
                            .stack(Some("daily".into()))
                            .corner_radii(top_radii)
                            .fill_color(move || theme().primary)
                            .stroke_none(),
                    )
                    .child(
                        bar()
                            .value(Some(RowAccessor::from("direct")))
                            .stack(Some("daily".into()))
                            .corner_radii(top_radii)
                            .fill_color(move || {
                                theme()
                                    .color("success")
                                    .unwrap_or(theme().secondary_foreground)
                                    .opacity(0.75)
                            })
                            .stroke_none(),
                    ),
            ),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .px_2()
                .child(each(
                    move || {
                        let days = traffic.get().recent_days;
                        if days.is_empty() {
                            (1..=7).map(|i| format!("D-{i}")).collect::<Vec<_>>()
                        } else {
                            days.into_iter()
                                .map(|d| d.date.get(5..).unwrap_or(&d.date).to_owned())
                                .collect()
                        }
                    },
                    |label| label.clone(),
                    |item, _| {
                        text(move || item.get())
                            .text_size(px(11.0))
                            .text_color(move || theme().muted_foreground)
                    },
                )),
        )
        .into_element()
}

fn legend_dot(
    dot_color: impl Fn() -> AdaptiveColor + 'static,
    label: impl Fn() -> String + 'static,
) -> AnyElement {
    view()
        .flex_row()
        .items_center()
        .gap_1()
        .child(view().size(px(8.0)).rounded_full().bg(dot_color))
        .child(
            text(label)
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .into_element()
}

fn ratio_stat_row(
    dot_color: impl Fn() -> AdaptiveColor + 'static,
    label: impl Fn() -> String + 'static,
    value: impl Fn() -> String + 'static,
) -> AnyElement {
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .child(legend_dot(dot_color, label))
        .child(
            text(value)
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD),
        )
        .into_element()
}
