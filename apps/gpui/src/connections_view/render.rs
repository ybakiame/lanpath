use super::ConnectionsView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup},
    input::Editor,
    list::List,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

impl Render for ConnectionsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_selection = self.list.read(cx).delegate().selected_id().is_some();
        let count = self.list.read(cx).delegate().len();
        let compact = window.viewport_size().width < window.rem_size() * 64.0;
        let status = if !self.running {
            "启动核心后显示当前会话连接".into()
        } else if self.closing {
            "正在关闭所选连接…".into()
        } else if let Some(error) = &self.load_error {
            format!("读取失败，保留上次快照；10 秒后重试：{error}")
        } else if !self.loaded {
            "正在读取连接…".into()
        } else if self.show_closed {
            format!("{count} 条当前会话关闭记录 · 最多保留 500 条")
        } else {
            format!("{count} 个活动连接 · 每 2 秒刷新")
        };
        let shown_count = if self.loaded {
            count.to_string()
        } else {
            "—".into()
        };
        let upload = if self.loaded {
            crate::design::format_bytes(self.upload_total)
        } else {
            "—".into()
        };
        let download = if self.loaded {
            crate::design::format_bytes(self.download_total)
        } else {
            "—".into()
        };
        let summary = format!("连接 {shown_count} · 累计上传 {upload} · 累计下载 {download}");
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(crate::design::page_header(
                "连接",
                "当前会话与流量",
                crate::design::toolbar()
                    .child(
                        Button::new("connections-refresh")
                            .label("刷新连接")
                            .disabled(!self.running || self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.request(None, cx))),
                    )
                    .child(
                        Button::new("connection-close")
                            .label("关闭所选连接")
                            .disabled(
                                self.show_closed || !self.running || self.busy || !has_selection,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                let id = this
                                    .list
                                    .read(cx)
                                    .delegate()
                                    .selected_id()
                                    .map(str::to_owned);
                                if let Some(id) = id {
                                    this.request(Some(id), cx);
                                }
                            })),
                    ),
                cx,
            ))
            .child(
                ButtonGroup::new("connections-kind")
                    .small()
                    .compact()
                    .outline()
                    .child(
                        Button::new("connections-active")
                            .label("活动连接")
                            .selected(!self.show_closed),
                    )
                    .child(
                        Button::new("connections-closed")
                            .label("会话已关闭")
                            .selected(self.show_closed),
                    )
                    .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                        if let Some(index) = indices.first() {
                            this.select_kind(*index == 1, cx);
                        }
                    })),
            )
            .child(
                div()
                    .id("connections-summary")
                    .accessibility_id("connections-summary")
                    .role(Role::Group)
                    .aria_label(summary.clone())
                    .flex_shrink_0()
                    .min_w_0()
                    .child(if compact {
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(summary)
                    } else {
                        crate::design::toolbar()
                            .child(crate::design::metric(
                                if self.show_closed {
                                    "会话关闭记录"
                                } else {
                                    "活动连接"
                                },
                                shown_count,
                                cx,
                            ))
                            .child(crate::design::metric("累计上传", upload, cx))
                            .child(crate::design::metric("累计下载", download, cx))
                    }),
            )
            .child(
                crate::design::surface(cx)
                    .id("connections-list")
                    .accessibility_id("connections-list")
                    .role(Role::Group)
                    .aria_label("连接列表")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(List::new(&self.list).size_full()),
            )
            .children((!self.detail_text.is_empty()).then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .h_48()
                    .max_h(gpui_kit::relative(0.35))
                    .min_h_0()
                    .min_w_0()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("连接详情 · 可选择复制"),
                    )
                    .child(Editor::new(&self.details).readonly(true).flex_1().min_h_0())
            }))
            .children(self.action_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(format!("关闭失败：{error}"))
            }))
            .children(self.notice.map(|notice| div().text_sm().child(notice)))
            .child(
                div()
                    .text_xs()
                    .text_color(if self.load_error.is_some() {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(status),
            )
    }
}
