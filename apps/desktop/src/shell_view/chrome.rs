use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    scroll::ScrollableElement as _,
    sidebar::{SidebarFooter, SidebarHeader, SidebarItem as _},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, div,
};

use crate::{CoreOperationKind, DesktopView, Page, design};

impl DesktopView {
    pub(super) fn render_sidebar(
        &self,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // Bounded destinations do not need virtualization. A regular
        // scroll owner also permits group reflow and focus autoscroll in one
        // accessibility pass (the virtual Sidebar retries prepaint on reveal).
        div()
            .id("desktop-navigation")
            .role(Role::Navigation)
            .aria_label("主导航")
            .flex()
            .flex_col()
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .w(gpui_kit::rems(13.5))
            .min_w(gpui_kit::rems(13.5))
            .max_w(gpui_kit::rems(13.5))
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(
                div().px_3().pt_3().child(
                    SidebarHeader::new().child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .py_3()
                            .child(gpui_kit::img("brand/lanpath.svg").size_9())
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .min_w_0()
                                    .child(div().font_semibold().child("岚径 · Lanpath"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(crate::product::SLOGAN),
                                    ),
                            ),
                    ),
                ),
            )
            .child(
                div()
                    .id("desktop-navigation-scroll")
                    .accessibility_id("desktop-navigation-scroll")
                    .role(Role::Group)
                    .aria_label("导航项目")
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .size_full()
                            .px_3()
                            .py_3()
                            .overflow_y_scrollbar()
                            .child(
                                crate::keyboard::NavigationMenu::new(self, cx)
                                    .render("navigation-items", window, cx)
                                    .into_any_element(),
                            ),
                    ),
            )
            .child(
                div().px_3().pb_3().child(
                    SidebarFooter::new().child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .pt_2()
                            .child(
                                design::toolbar()
                                    .child(
                                        Button::new("desktop-appearance")
                                            .ghost()
                                            .small()
                                            .icon(IconName::Palette)
                                            .accessibility_label("外观")
                                            .tooltip("外观与主题 · Ctrl+,")
                                            .disabled(self.closing)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_page(Page::Appearance, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("desktop-keyboard-help")
                                            .ghost()
                                            .small()
                                            .icon(IconName::Keyboard)
                                            .accessibility_label("键盘快捷键")
                                            .tooltip("键盘快捷键…")
                                            .disabled(self.closing)
                                            .on_click(|_, window, cx| {
                                                window.open_dialog(cx, |dialog, _, cx| {
                                                    dialog.title("键盘快捷键").child(
                                                        div().flex().flex_col().gap_3().children(
                                                            [
                                                                ("切换导航与内容", "F6 / Shift+F6"),
                                                                (
                                                                    "选择导航项目",
                                                                    "↑ / ↓ / Home / End",
                                                                ),
                                                                ("打开导航项目", "Enter / Space"),
                                                                ("保存当前表单", "Ctrl+S"),
                                                                ("打开外观", "Ctrl+,"),
                                                                ("关闭弹窗", "Esc"),
                                                            ]
                                                            .map(|(label, shortcut)| {
                                                                div()
                                                                .flex()
                                                                .justify_between()
                                                                .gap_4()
                                                                .text_sm()
                                                                .child(label)
                                                                .child(
                                                                    div()
                                                                        .text_color(
                                                                            cx.theme()
                                                                                .muted_foreground,
                                                                        )
                                                                        .child(shortcut),
                                                                )
                                                            }),
                                                        ),
                                                    )
                                                });
                                            }),
                                    )
                                    .child(
                                        Button::new("desktop-about")
                                            .ghost()
                                            .small()
                                            .icon(IconName::Info)
                                            .accessibility_label("关于")
                                            .tooltip("关于岚径")
                                            .disabled(self.closing)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_page(Page::About, cx)
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(concat!("GPUI 试验版 · v", env!("CARGO_PKG_VERSION"))),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(self.cores.read(cx).selected_core_summary()),
                            ),
                    ),
                ),
            )
    }

    pub(super) fn render_session_bar(
        &self,
        operation_busy: bool,
        dirty: bool,
        window: &gpui_kit::Window,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let compact = window.viewport_size().width < window.rem_size() * 64.0;
        let state = if self.closing {
            "正在关闭"
        } else if let Some(operation) = self.core_busy {
            match operation {
                CoreOperationKind::Start => "正在启动",
                CoreOperationKind::Stop => "正在停止",
            }
        } else if self.core_recovery_needed {
            "需要恢复"
        } else if self.core_running {
            "运行中"
        } else {
            "未启动"
        };
        let state_color = if self.core_error.is_some() || self.core_recovery_needed {
            cx.theme().danger
        } else if self.core_running {
            cx.theme().success
        } else {
            cx.theme().muted_foreground
        };
        let reason = if dirty {
            Some("保存或放弃未保存修改后可启动")
        } else if !self.core.is_configured() {
            Some("请先在核心页选择可用核心")
        } else if !self.configuration.read(cx).ready_for_start() {
            Some("请先在配置预览中生成有效配置")
        } else if self.distribution.read(cx).is_pending() {
            Some("完成待处理的迁移或更新后可启动")
        } else if operation_busy {
            Some("等待当前操作完成")
        } else {
            None
        };
        let core_status = format!(
            "{state} · {} · 127.0.0.1:{}",
            self.core.selected_core_name(),
            self.core.port()
        );
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_3()
                    .px_5()
                    .py_3()
                    .children((!compact).then(|| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .min_w_0()
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(self.page.group_title(self.core.selected_core_type())),
                            )
                            .child(
                                Icon::new(IconName::ChevronRight)
                                    .xsmall()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(self.page.presentation().0)
                    }))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("core-status")
                            .accessibility_id("core-status")
                            .role(Role::Status)
                            .aria_label(core_status.clone())
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .child(
                                Icon::new(if self.core_running {
                                    IconName::CircleCheck
                                } else {
                                    IconName::Circle
                                })
                                .small()
                                .text_color(state_color),
                            )
                            .child(div().text_color(state_color).child(state))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} · {}",
                                        self.core.selected_core_name(),
                                        self.core.port()
                                    )),
                            ),
                    )
                    .children((!self.core_running && !self.core_recovery_needed).then(|| {
                        Button::new("core-start")
                            .accessibility_id("core-start")
                            .small()
                            .icon(IconName::Play)
                            .label("启动核心")
                            .loading(matches!(self.core_busy, Some(CoreOperationKind::Start)))
                            .disabled(operation_busy || reason.is_some())
                            .tooltip(reason.unwrap_or("启动已保存的配置"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_core_operation(CoreOperationKind::Start, window, cx)
                            }))
                    }))
                    .children((self.core_running || self.core_recovery_needed).then(|| {
                        Button::new("core-stop")
                            .accessibility_id("core-stop")
                            .small()
                            .ghost()
                            .icon(IconName::Square)
                            .label(if self.core_recovery_needed && !self.core_running {
                                "恢复自有控制"
                            } else {
                                "停止核心"
                            })
                            .loading(matches!(self.core_busy, Some(CoreOperationKind::Stop)))
                            .disabled(operation_busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_core_operation(CoreOperationKind::Stop, window, cx)
                            }))
                    })),
            )
            .children(self.core_error.as_ref().map(|error| {
                div()
                    .px_5()
                    .pb_3()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            }))
            .children(
                (!self.core_running && self.core_error.is_none())
                    .then_some(reason)
                    .flatten()
                    .map(|reason| {
                        div()
                            .px_5()
                            .pb_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(reason)
                    }),
            )
    }
}
