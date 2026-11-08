use super::{Operation, ProxyView};
use crate::proxy_list::ProxySort;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    input::Input,
    list::List,
    tab::{Tab, TabBar},
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div};

impl Render for ProxyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.closing || self.locked || self.operation.is_some();
        let selected_member = self.list.read(cx).delegate().selected_member().is_some();
        let sort = self.list.read(cx).delegate().sort_order();
        let active_index = self
            .groups
            .iter()
            .position(|group| Some(&group.name) == self.active_group.as_ref())
            .unwrap_or(0);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(crate::design::page_header(
                "代理策略",
                "按分组选择节点，查看连接延迟",
                crate::design::toolbar()
                    .child(
                        Button::new("proxy-test-selected")
                            .accessibility_id("proxy-test-selected")
                            .outline()
                            .icon(IconName::Zap)
                            .label(if matches!(self.operation, Some(Operation::TestNode)) {
                                "测速中…"
                            } else {
                                "测速所选"
                            })
                            .disabled(busy || !self.running || !selected_member)
                            .on_click(cx.listener(|this, _, _, cx| this.test_selected(cx))),
                    )
                    .child(
                        Button::new("proxy-test-group")
                            .accessibility_id("proxy-test-group")
                            .primary()
                            .icon(IconName::Zap)
                            .label(if matches!(self.operation, Some(Operation::TestGroup)) {
                                "测速中…"
                            } else {
                                "测速当前组"
                            })
                            .disabled(busy || !self.running || self.active_group.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.test_group(cx))),
                    ),
                cx,
            ))
            .child(
                crate::design::surface(cx)
                    .p_3()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_3()
                    .child(
                        ButtonGroup::new("proxy-mode")
                            .small()
                            .compact()
                            .outline()
                            .disabled(busy)
                            .children(
                                [("rule", "规则"), ("global", "全局"), ("direct", "直连")].map(
                                    |(mode, label)| {
                                        Button::new(format!("proxy-mode-{mode}"))
                                            .label(label)
                                            .selected(self.mode == mode)
                                    },
                                ),
                            )
                            .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                                if let Some(mode) = indices
                                    .first()
                                    .and_then(|index| ["rule", "global", "direct"].get(*index))
                                {
                                    this.set_mode(mode, cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(gpui_kit::rems(10.))
                            .child(Input::new(&self.search).aria_label("搜索当前组节点")),
                    )
                    .child(
                        ButtonGroup::new("proxy-sort")
                            .small()
                            .compact()
                            .outline()
                            .children(
                                [
                                    (ProxySort::Original, "原序"),
                                    (ProxySort::Name, "名称"),
                                    (ProxySort::Delay, "延迟"),
                                ]
                                .map(|(value, label)| {
                                    Button::new(format!("proxy-sort-{label}"))
                                        .label(label)
                                        .selected(sort == value)
                                }),
                            )
                            .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                                if let Some(sort) = indices.first().and_then(|index| {
                                    [ProxySort::Original, ProxySort::Name, ProxySort::Delay]
                                        .get(*index)
                                }) {
                                    this.list.update(cx, |list, cx| {
                                        list.delegate_mut().sort(*sort);
                                        cx.notify();
                                    });
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .children((!self.groups.is_empty()).then(|| {
                TabBar::new("proxy-groups")
                    .underline()
                    .selected_index(active_index)
                    .children(
                        self.groups
                            .iter()
                            .map(|group| Tab::new().label(group.name.clone())),
                    )
                    .on_click(cx.listener(|this, index: &usize, _, cx| {
                        if let Some(group) = this.groups.get(*index) {
                            this.select_group(group.name.clone(), cx);
                        }
                    }))
            }))
            .children(self.action_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            }))
            .children(self.notice.as_ref().map(|notice| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(notice.clone())
            }))
            .child(
                crate::design::surface(cx)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(if !self.running {
                        crate::design::empty_state(
                            IconName::Waypoints,
                            "核心尚未启动",
                            "启动核心后选择节点；路由模式可提前保存。",
                            cx,
                        )
                        .into_any_element()
                    } else {
                        List::new(&self.list)
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .into_any_element()
                    }),
            )
            .child(
                crate::design::toolbar().child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.status()),
                ),
            )
    }
}
