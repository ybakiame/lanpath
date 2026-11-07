use chrono::DateTime;
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    input::Editor,
    progress::Progress,
    tag::Tag,
};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _,
};
use lanpath_client::{SubFormat, Subscription, SubscriptionInfo};

use super::{Operation, SubscriptionsView};

impl SubscriptionsView {
    fn open_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = self.selected else {
            return;
        };
        let title = self
            .subscriptions
            .iter()
            .find(|item| item.id == selected)
            .map(|item| format!("订阅原文 · {}", item.name))
            .unwrap_or_else(|| "订阅原文".into());
        let source = self.source.clone();
        window.open_dialog(cx, move |dialog, window, _| {
            dialog
                .title(title.clone())
                .w(window.rem_size() * 64.0)
                .h(window.viewport_size().height * 0.75)
                .margin_top(window.viewport_size().height / 8.0)
                .shadow_lg()
                .child(
                    Editor::new(&source)
                        .aria_label("订阅原文")
                        .readonly(true)
                        .size_full(),
                )
        });
    }

    pub(super) fn render_card(
        &self,
        item: &Subscription,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = item.id;
        let active = Some(id) == self.active;
        let selected = Some(id) == self.selected;
        let format = item.format.map(format_label).unwrap_or("尚未拉取");
        let expire = item
            .userinfo
            .as_ref()
            .and_then(|info| info.expire)
            .map(format_expire)
            .unwrap_or_else(|| "-".into());
        let host = reqwest::Url::parse(&item.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| "订阅源".into());

        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_3()
            .p_4()
            .border_1()
            .rounded_lg()
            .border_color(if selected {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .bg(cx.theme().group_box)
            .child(
                Button::new(format!("subscription-{id}"))
                    .accessibility_label(format!("查看订阅 {}", item.name))
                    .tooltip(item.name.clone())
                    .ghost()
                    .disabled(disabled)
                    .w_full()
                    .h_auto()
                    .min_w_0()
                    .p_0()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.request_select(Some(id), window, cx, false)
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .min_w_0()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .min_w_0()
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::Globe)
                                            .small()
                                            .flex_shrink_0()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .font_semibold()
                                            .child(item.name.clone()),
                                    )
                                    .when(active, |element| {
                                        element.child(Tag::primary().small().child("生效中"))
                                    })
                                    .when(!item.enabled, |element| {
                                        element.child(Tag::secondary().small().child("已停用"))
                                    }),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(host),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} 个节点 · {format} · 到期 {expire}",
                                        item.node_count
                                    )),
                            )
                            .children(item.userinfo.as_ref().map(|info| {
                                let (usage, percent) = usage(info);
                                let color = if percent >= 100.0 {
                                    cx.theme().danger
                                } else if percent >= 80.0 {
                                    cx.theme().warning
                                } else {
                                    cx.theme().primary
                                };
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex()
                                            .justify_between()
                                            .gap_2()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(usage)
                                            .child(format!("{percent:.0}%")),
                                    )
                                    .child(
                                        Progress::new(format!("subscription-usage-{id}"))
                                            .small()
                                            .value(percent)
                                            .color(color),
                                    )
                            }))
                            .children(item.error.as_ref().map(|error| {
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().warning)
                                    .child(error.clone())
                            })),
                    ),
            )
            .when(selected, |element| {
                element.child(self.render_card_actions(item, disabled, cx))
            })
            .into_any_element()
    }

    fn render_card_actions(
        &self,
        item: &Subscription,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = Some(item.id) == self.active;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("subscription-edit")
                    .small()
                    .icon(IconName::SquarePen)
                    .label("编辑…")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let selected = this.selected;
                        this.request_select(selected, window, cx, true)
                    })),
            )
            .child(
                Button::new("subscription-source-toggle")
                    .small()
                    .ghost()
                    .icon(IconName::FileText)
                    .label("原文…")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| this.open_source(window, cx))),
            )
            .child(
                Button::new("subscription-refresh")
                    .small()
                    .ghost()
                    .icon(IconName::RefreshCw)
                    .label("刷新")
                    .loading(matches!(self.busy, Some(Operation::Refresh)))
                    .tooltip("刷新已保存的订阅 URL；保留未保存的表单修改")
                    .disabled(disabled)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.run(Operation::Refresh, window, cx)),
                    ),
            )
            .child(
                Button::new("subscription-activate")
                    .small()
                    .ghost()
                    .icon(IconName::CircleCheck)
                    .label(if active { "生效中" } else { "设为当前" })
                    .loading(matches!(self.busy, Some(Operation::Activate)))
                    .disabled(disabled || active || !item.enabled)
                    .on_click(
                        cx.listener(|this, _, window, cx| {
                            this.run(Operation::Activate, window, cx)
                        }),
                    ),
            )
            .child(self.more_actions(item, disabled, cx))
            .into_any_element()
    }
}

fn format_label(format: SubFormat) -> &'static str {
    match format {
        SubFormat::ShareLinks => "Share Links",
        SubFormat::ClashYaml => "Clash",
        SubFormat::SingBoxJson => "sing-box",
    }
}

fn format_expire(timestamp: u64) -> String {
    i64::try_from(timestamp)
        .ok()
        .and_then(|timestamp| DateTime::from_timestamp(timestamp, 0))
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|| "-".into())
}

fn usage(info: &SubscriptionInfo) -> (String, f32) {
    let used = info
        .download
        .unwrap_or(0)
        .saturating_add(info.upload.unwrap_or(0));
    let percent = info.total.filter(|total| *total > 0).map_or(0.0, |total| {
        ((used as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32
    });
    let usage = info.total.map_or_else(
        || format_bytes(used),
        |total| format!("{} / {}", format_bytes(used), format_bytes(total)),
    );
    (usage, percent)
}

fn format_bytes(bytes: u64) -> String {
    const BYTES_PER_GB: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.2} GB", bytes as f64 / BYTES_PER_GB)
}
