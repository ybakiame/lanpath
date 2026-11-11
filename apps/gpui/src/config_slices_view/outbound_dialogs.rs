use gpui_kit::component::{
    Disableable as _, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
};
use gpui_kit::{Context, ParentElement as _, Styled as _, Window, div};

use super::{
    ConfigSlicesView, OutboundKind, outbound_editing::outbound_kind, render::field,
    render_outbounds::outbound_kinds,
};

impl ConfigSlicesView {
    pub(super) fn open_new_outbound(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        self.new_outbound_kind = OutboundKind::Vless;
        let view = cx.entity();
        window.open_alert_dialog(cx, move |dialog, window, app| {
            let selected = view.read(app).new_outbound_kind;
            let weak = view.downgrade();
            dialog
                .title("新增出站")
                .w(window.rem_size() * 64.0)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .child("选择协议后创建条目；连接参数在编辑弹窗中填写。"),
                        )
                        .child(div().flex().flex_wrap().gap_2().children(
                            outbound_kinds().into_iter().map(|(kind, label)| {
                                Button::new(format!("outbound-add-kind-{label}"))
                                    .small()
                                    .outline()
                                    .label(label)
                                    .selected(selected == kind)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.new_outbound_kind = kind;
                                                cx.notify();
                                            });
                                        }
                                    })
                            }),
                        )),
                )
                .confirm()
                .ok_text("创建")
                .cancel_text("取消")
                .on_ok(move |_, window, cx: &mut gpui_kit::App| {
                    let kind = weak
                        .read_with(cx, |this, _| this.new_outbound_kind)
                        .unwrap_or(OutboundKind::Vless);
                    let _ = weak.update(cx, |this, cx| this.add_outbound(kind, window, cx));
                    true
                })
        });
    }

    pub(super) fn open_outbound_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || self.selected_outbound.is_none() {
            return;
        }
        let view = cx.entity();
        window.open_dialog(cx, move |dialog, window, app| {
            let this = view.read(app);
            let Some(outbound) = this
                .selected_outbound
                .as_deref()
                .and_then(|id| this.outbound(id))
                .cloned()
            else {
                return dialog
                    .title("编辑出站")
                    .child(div().text_sm().child("请选择一个出站条目。"));
            };
            let disabled = this.locked || this.busy();
            let protocol_locked = disabled || outbound.builtin;
            let selected_kind = outbound_kind(&outbound.protocol);
            let is_group = outbound.protocol.is_group();
            dialog
                .title(format!("编辑出站 · {}", outbound.name))
                .w(window.rem_size() * 72.0)
                .max_h(window.viewport_size().height * 0.75)
                .margin_top(window.rem_size() * 6.0)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .child("修改进入结构化配置草稿；保存页面后写入配置文件。"),
                        )
                        .child(field("名称", &this.inputs.out_name, protocol_locked))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_sm().child("协议"))
                                .child(
                                    ButtonGroup::new("outbound-kind")
                                        .small()
                                        .compact()
                                        .outline()
                                        .disabled(protocol_locked)
                                        .children(outbound_kinds().into_iter().map(
                                            |(kind, label)| {
                                                Button::new(format!("outbound-kind-{label}"))
                                                    .label(label)
                                                    .selected(selected_kind == kind)
                                            },
                                        ))
                                        .on_click(window.listener_for(
                                            &view,
                                            |this, indexes: &Vec<usize>, window, cx| {
                                                if let Some((kind, _)) = indexes
                                                    .first()
                                                    .map(|index| outbound_kinds()[*index])
                                                {
                                                    this.set_outbound_kind(kind, window, cx);
                                                }
                                            },
                                        )),
                                ),
                        )
                        .children(
                            outbound.builtin.then(|| {
                                div().text_sm().child("内置分组名称、协议和成员由模板维护")
                            }),
                        )
                        .children(
                            (!is_group).then(|| {
                                this.render_node_fields(&outbound, disabled, window, &view)
                            }),
                        )
                        .children(
                            is_group.then(|| {
                                this.render_group_fields(&outbound, disabled, window, &view)
                            }),
                        ),
                )
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new().child(
                        Button::new("outbound-editor-done")
                            .primary()
                            .label("完成")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
                )
        });
    }
}
