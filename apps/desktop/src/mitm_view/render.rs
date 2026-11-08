use super::{MitmView, Operation, traffic_list::traffic_columns};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::Editor,
    list::List,
    scroll::ScrollableElement as _,
    switch::Switch,
    tab::{Tab, TabBar},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

impl Render for MitmView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let compact = window.viewport_size().width < window.rem_size() * 64.;
        let status = self
            .error
            .clone()
            .or_else(|| self.sync_error.clone())
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| {
                if self.operation == Some(Operation::Save) {
                    "正在保存设置…".into()
                } else if self.dirty {
                    "有未保存修改 · 保存后下次启动生效".into()
                } else if self.operation == Some(Operation::InstallCa) {
                    "正在安装 CA…".into()
                } else if self.operation == Some(Operation::RemoveCa) {
                    "正在移除拥有的 CA 信任…".into()
                } else if self.mitm_addr.is_some() {
                    "MITM 运行中 · 资源变更在下次启动生效".into()
                } else if self.running && self.enabled {
                    "MITM 未启动 · 请检查是否有启用的主机规则".into()
                } else if self.enabled {
                    "已启用 · 下次启动生效".into()
                } else {
                    "MITM 已关闭".into()
                }
            });
        let traffic_count = self.traffic.read(cx).delegate().len();
        let enabled = Switch::new("mitm-enabled")
            .label("启用 MITM")
            .checked(self.enabled)
            .disabled(disabled)
            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                this.enabled = *checked;
                this.dirty = true;
                this.notice = None;
                cx.notify();
            }));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_2()
            .child(if compact {
                crate::design::toolbar()
                    .child(div().flex_1().font_semibold().text_lg().child("MITM"))
                    .child(enabled)
                    .into_any_element()
            } else {
                crate::design::page_header("MITM", "HTTPS 检查与请求记录", enabled, cx)
                    .into_any_element()
            })
            .child(self.render_certificate(cx))
            .child(
                TabBar::new("mitm-sections")
                    .segmented()
                    .selected_index(self.section)
                    .child(Tab::new().label("主机规则"))
                    .child(Tab::new().label(format!("请求记录 · {traffic_count}")))
                    .on_click(cx.listener(|this, section: &usize, _, cx| {
                        this.section = *section;
                        if *section == 1 {
                            this.refresh_traffic(cx);
                        }
                        cx.notify();
                    })),
            )
            .child(if self.section == 0 {
                self.render_hosts(compact, cx).into_any_element()
            } else {
                self.render_traffic(cx).into_any_element()
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        div()
                            .id("mitm-status")
                            .accessibility_id("mitm-status")
                            .role(Role::Status)
                            .aria_label(status.clone())
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .max_h_16()
                            .overflow_y_scroll()
                            .text_color(if self.error.is_some() || self.sync_error.is_some() {
                                cx.theme().danger
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(status),
                    )
                    .child(
                        Button::new("mitm-save")
                            .accessibility_id("mitm-save")
                            .primary()
                            .label("保存设置")
                            .disabled(disabled || !self.dirty())
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}

impl MitmView {
    fn render_hosts(&self, compact: bool, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .font_semibold()
                            .child(format!("资源主机 · {}", self.derived.len())),
                    )
                    .children((!compact).then(|| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("来自已启用的片段与本地资源")
                    })),
            )
            .child(
                crate::design::surface(cx)
                    .id("mitm-derived")
                    .accessibility_id("mitm-derived")
                    .role(Role::Group)
                    .aria_label(format!("资源主机 · {}", self.derived.len()))
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(
                        div()
                            .size_full()
                            .overflow_y_scrollbar()
                            .p_3()
                            .children(self.derived.is_empty().then(|| {
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if compact {
                                        "暂无主机，请在脚本页启用片段或手动补充。"
                                    } else {
                                        "暂无主机声明。请在脚本页启用并更新片段，或在下方手动添加。"
                                    })
                            }))
                            .children(self.derived.iter().map(|entry| {
                                let label =
                                    format!("{} · {}", entry.hostname, entry.sources.join("、"));
                                div()
                                    .id(gpui_kit::SharedString::from(format!(
                                        "mitm-host:{}",
                                        entry.hostname
                                    )))
                                    .accessibility_id(format!("mitm-host:{}", entry.hostname))
                                    .role(Role::Group)
                                    .aria_label(label)
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .min_w_0()
                                    .gap_2()
                                    .py_1()
                                    .text_sm()
                                    .child(entry.hostname.clone())
                                    .child(
                                        div()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(entry.sources.join("、")),
                                    )
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        Button::new("mitm-manual-toggle")
                            .accessibility_id("mitm-manual-toggle")
                            .small()
                            .ghost()
                            .label(format!(
                                "{}手动补充 · {}",
                                if self.manual_open { "收起" } else { "展开" },
                                self.hostnames
                                    .read(cx)
                                    .value()
                                    .lines()
                                    .filter(|line| !line.trim().is_empty())
                                    .count()
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.manual_open = !this.manual_open;
                                cx.notify();
                            })),
                    )
                    .children(self.manual_open.then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("每行一个域名，支持通配符；- 或 ! 前缀表示排除"),
                            )
                            .child(
                                div().h_24().child(
                                    Editor::new(&self.hostnames)
                                        .aria_label("手动 MITM 主机")
                                        .size_full(),
                                ),
                            )
                    })),
            )
            .children((!compact).then(|| {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("进入页面自动同步资源主机；没有包含项时不启动 MITM。")
            }))
    }

    fn render_traffic(&self, cx: &Context<Self>) -> impl IntoElement {
        crate::design::surface(cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .px_3()
                    .pt_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.running {
                        "当前应用会话 · 最新在前 · 每 2 秒自动更新"
                    } else {
                        "当前应用会话 · 最新在前 · 核心已停止"
                    }),
            )
            .child(
                div()
                    .px_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(traffic_columns("时间", "方法", "URL", "状态", "耗时")),
            )
            .child(List::new(&self.traffic).flex_1().min_h_0())
    }
}
