use super::LogsView;
use crate::log_buffer::CoreLogState;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    input::Editor,
    list::List,
    menu::{DropdownMenu as _, PopupMenuItem},
    switch::Switch,
    tab::{Tab, TabBar},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

impl LogsView {
    fn request_clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("清空内存日志？")
                .description("仅清空本次进程的日志缓冲，不删除磁盘文件，不停止后续采集。")
                .confirm()
                .ok_text("清空内存日志")
                .cancel_text("取消")
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.core.logs().buffer().clear();
                        this.refresh(cx);
                    });
                    true
                })
        });
    }

    fn memory_menu(&self, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.weak_entity();
        Button::new("logs-more")
            .ghost()
            .icon(IconName::Ellipsis)
            .accessibility_label("日志显示选项")
            .tooltip("显示条数与清空内存日志")
            .dropdown_menu(move |mut menu, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return menu;
                };
                for limit in [200, 500, 1000] {
                    let selected = entity.read(cx).limit == limit;
                    let view = view.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!("最近 {limit} 条"))
                            .checked(selected)
                            .on_click(move |_, _, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    this.limit = limit;
                                    this.revision = None;
                                    this.refresh(cx);
                                });
                            }),
                    );
                }
                let disabled = entity.read(cx).total == 0;
                let view = view.clone();
                menu.separator().item(
                    PopupMenuItem::new("清空内存日志…")
                        .icon(IconName::Trash)
                        .disabled(disabled)
                        .on_click(move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| this.request_clear(window, cx));
                        }),
                )
            })
    }

    fn render_memory(&self, cx: &Context<Self>) -> impl IntoElement {
        let status = self.status();
        let error = matches!(
            self.state,
            CoreLogState::Disconnected(_) | CoreLogState::Unavailable(_)
        );
        div().flex().flex_col().flex_1().min_h_0().min_w_0().gap_3()
            .child(crate::design::surface(cx).p_3().flex().items_center().flex_wrap().gap_3()
                .child(ButtonGroup::new("logs-levels").small().compact().outline()
                    .children(["全部", "调试", "信息", "警告", "错误"].into_iter().enumerate().map(|(rank,label)| {
                        Button::new(("logs-level",rank)).label(label).selected(self.min_rank == rank as u8)
                    })).on_click(cx.listener(|this, indices: &Vec<usize>,_,cx| {
                        if let Some(rank) = indices.first() { this.min_rank = *rank as u8; this.revision = None; this.refresh(cx); }
                    })))
                .child(div().flex_1())
                .child(Switch::new("logs-auto-refresh").label("实时刷新").checked(self.auto_refresh)
                    .on_click(cx.listener(|this, checked: &bool,_,cx| { this.auto_refresh = *checked; if *checked { this.refresh(cx); } cx.notify(); })))
                .child(self.memory_menu(cx)))
            .child(crate::design::surface(cx).flex().flex_col().flex_1().min_h_0().min_w_0()
                .child(List::new(&self.list).flex_1().min_h_0().min_w_0()))
            .children(self.detail_id.map(|_| div().id("log-detail").role(Role::Group).aria_label("日志详情")
                .flex().flex_col().gap_2().h_40().max_h(gpui_kit::relative(0.35)).min_h_0().min_w_0().flex_shrink_0()
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child("日志详情 · 可选择复制"))
                .child(Editor::new(&self.details).readonly(true).flex_1().min_h_0())))
            .child(div().id("logs-status").accessibility_id("logs-status").role(Role::Status).aria_label(status.clone())
                .text_xs().text_color(if error { cx.theme().danger } else { cx.theme().muted_foreground }).child(status))
            .child(crate::design::help("logs-help", "采集与缓冲说明", div().text_sm().child(
                "最新在前，最多保留 1000 条；暂停刷新或切页不停止采集。内存日志仅限本次进程，断流期间不回放；清空不删除磁盘日志。")))
    }
}

impl Render for LogsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if self.disk_visible {
            self.disk.clone().into_any_element()
        } else {
            self.render_memory(cx).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(crate::design::page_header(
                "日志",
                "运行事件与持久记录",
                div().children((!self.disk_visible).then(|| {
                    Button::new("logs-refresh")
                        .ghost()
                        .icon(IconName::RefreshCw)
                        .accessibility_label("刷新日志")
                        .tooltip("刷新日志")
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))
                })),
                cx,
            ))
            .child(
                TabBar::new("logs-views")
                    .segmented()
                    .selected_index(usize::from(self.disk_visible))
                    .child(Tab::new().label("内存日志"))
                    .child(Tab::new().label("磁盘日志"))
                    .on_click(cx.listener(|this, index: &usize, window, cx| {
                        this.disk_visible = *index == 1;
                        if this.disk_visible {
                            this.disk.update(cx, |disk, cx| disk.refresh(window, cx));
                        } else {
                            this.refresh(cx);
                        }
                        cx.notify();
                    })),
            )
            .children(self.disk_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(format!("磁盘日志写入失败：{error}"))
            }))
            .child(content)
    }
}
