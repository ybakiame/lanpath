use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, button::Button, list::List,
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div};

use super::ProxyView;

impl Render for ProxyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.closing || self.locked || self.operation.is_some();
        let selected_member = self.list.read(cx).delegate().selected_member().is_some();
        let has_group = self.active_group.is_some();
        let groups = self.groups.clone();
        div()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(crate::design::page_header(
                "代理策略",
                "选择路由模式与当前节点",
                crate::design::toolbar().children(
                    [("rule", "规则"), ("global", "全局"), ("direct", "直连")].map(
                        |(mode, label)| {
                            Button::new(format!("proxy-mode-{mode}"))
                                .label(label)
                                .selected(self.mode == mode)
                                .disabled(busy)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.set_mode(mode, cx)),
                                )
                        },
                    ),
                ),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        Button::new("proxy-refresh")
                            .label("重新读取")
                            .disabled(busy || !self.running)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    )
                    .child(
                        Button::new("proxy-test-selected")
                            .label("测速所选")
                            .disabled(busy || !self.running || !selected_member)
                            .on_click(cx.listener(|this, _, _, cx| this.test_selected(cx))),
                    )
                    .child(
                        Button::new("proxy-test-group")
                            .label("测速当前组")
                            .disabled(busy || !self.running || !has_group)
                            .on_click(cx.listener(|this, _, _, cx| this.test_group(cx))),
                    ),
            )
            .children((groups.len() > 1).then(|| {
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .children(groups.into_iter().map(|group| {
                        let name = group.name;
                        let selected = self.active_group.as_deref() == Some(name.as_str());
                        Button::new(format!("proxy-group-{name}"))
                            .label(name.clone())
                            .selected(selected)
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_group(name.clone(), cx)
                            }))
                    }))
            }))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.status()),
            )
            .children(self.action_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(format!("操作失败：{error}"))
            }))
            .children(
                self.notice
                    .as_ref()
                    .map(|notice| div().text_sm().child(notice.clone())),
            )
            .child(
                crate::design::surface(cx)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(if !self.running {
                        crate::design::empty_state(
                            gpui_kit::assets::IconName::Waypoints,
                            "核心尚未启动",
                            "启动核心后，在这里选择代理分组与节点。",
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
    }
}
