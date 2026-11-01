use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    form::{Form, field},
    input::{Editor, Input},
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    AnyElement, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Role, StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _,
};

use super::{Operation, ProfilesView};
use gpui_kit::component::input::EditorState;

impl ProfilesView {
    pub(super) fn open_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_open || self.locked || self.busy() {
            return;
        }
        self.editor_open = true;
        self.editor_confirm_discard = false;
        let creating = self.selected.is_none();
        let view = cx.entity();
        window.open_dialog(cx, move |dialog, window, app| {
            let view = view.clone();
            let confirming = view.read(app).editor_confirm_discard;
            let mut dialog = dialog
                .w(window.rem_size() * 80.0)
                .h(window.viewport_size().height * 0.75)
                .margin_top(window.viewport_size().height / 8.0)
                .shadow_lg();
            if confirming {
                dialog = dialog
                    .title("放弃未保存的覆写修改？")
                    .child("当前模板修改尚未保存，放弃后不会影响已保存的模板。")
                    .footer(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("profile-discard-cancel")
                                    .label("继续编辑")
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.editor_confirm_discard = false;
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("profile-discard-confirm")
                                    .danger()
                                    .label("放弃修改")
                                    .on_click({
                                        let view = view.clone();
                                        move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                let selected = this.selected;
                                                this.select(selected, window, cx);
                                                this.close_editor(window, cx);
                                            });
                                        }
                                    }),
                            ),
                    );
            } else {
                let this = view.read(app);
                let disabled = this.locked || this.busy();
                let dirty = this.dirty;
                dialog = dialog
                    .title(if creating {
                        "新建覆写模板"
                    } else {
                        "编辑覆写模板"
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .size_full()
                            .gap_4()
                            .child(
                                Form::new()
                                    .columns(2)
                                    .gap_3()
                                    .child(
                                        field().label("名称").required(true).child(
                                            Input::new(&view.read(app).name.clone())
                                                .accessibility_id("profile-name")
                                                .aria_label("覆写模板名称")
                                                .disabled(disabled),
                                        ),
                                    )
                                    .child(
                                        field().label("远程 YAML URL（可选）").child(
                                            Input::new(&view.read(app).yaml_url.clone())
                                                .accessibility_id("profile-yaml-url")
                                                .aria_label("远程 YAML URL")
                                                .disabled(disabled),
                                        ),
                                    )
                                    .child(
                                        field().label("远程 JS URL（可选）").col_span(2).child(
                                            Input::new(&view.read(app).js_url.clone())
                                                .accessibility_id("profile-js-url")
                                                .aria_label("远程 JS URL")
                                                .disabled(disabled),
                                        ),
                                    ),
                            )
                            .child(code_pair(
                                &view.read(app).yaml,
                                &view.read(app).js,
                                disabled,
                            )),
                    )
                    .footer(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("profile-cancel-edit")
                                    .label("取消")
                                    .disabled(disabled)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.cancel_editor(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("profile-save")
                                    .primary()
                                    .icon(IconName::Save)
                                    .label("保存")
                                    .loading(matches!(view.read(app).busy, Some(Operation::Save)))
                                    .disabled(disabled || !dirty)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.run(Operation::Save, window, cx)
                                            });
                                        }
                                    }),
                            ),
                    );
            }
            dialog
                .on_cancel({
                    let view = view.clone();
                    move |_, window, cx| view.update(cx, |this, cx| this.cancel_editor(window, cx))
                })
                .on_close({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, _| {
                            this.editor_open = false;
                            this.editor_confirm_discard = false;
                        });
                    }
                })
        });
        cx.notify();
    }

    pub(super) fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_open {
            self.editor_open = false;
            self.editor_confirm_discard = false;
            window.close_dialog(cx);
        }
    }

    fn cancel_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.locked || self.busy() {
            return false;
        }
        if self.dirty {
            self.editor_confirm_discard = true;
            cx.notify();
            return false;
        }
        self.editor_open = false;
        window.close_dialog(cx);
        true
    }

    pub(super) fn open_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = self.selected else {
            return;
        };
        let title = self
            .profiles
            .iter()
            .find(|profile| profile.id == selected)
            .map(|profile| format!("覆写内容 · {}", profile.name))
            .unwrap_or_else(|| "覆写内容".into());
        let yaml = self.yaml.clone();
        let js = self.js.clone();
        window.open_dialog(cx, move |dialog, window, _| {
            dialog
                .title(title.clone())
                .w(window.rem_size() * 80.0)
                .h(window.viewport_size().height * 0.75)
                .margin_top(window.viewport_size().height / 8.0)
                .shadow_lg()
                .child(code_pair(&yaml, &js, true))
        });
    }
}

fn code_pair(yaml: &Entity<EditorState>, js: &Entity<EditorState>, readonly: bool) -> AnyElement {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .gap_2()
                .child(div().text_sm().font_medium().child("YAML 深合并"))
                .child(
                    Editor::new(yaml)
                        .aria_label("YAML 覆写正文")
                        .readonly(readonly)
                        .size_full(),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .gap_2()
                .child(div().text_sm().font_medium().child("JS main(config)"))
                .child(
                    Editor::new(js)
                        .aria_label("JS 覆写正文")
                        .readonly(readonly)
                        .size_full(),
                ),
        )
        .into_any_element()
}

impl Render for ProfilesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let status = self
            .error
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| "YAML 深合并后执行 JS；远程覆写失败时使用缓存".into());

        div()
            .id("profiles-page")
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().text_lg().font_semibold().child("覆写模板"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("共 {} 个模板", self.profiles.len())),
                            ),
                    )
                    .child(
                        Button::new("profile-new")
                            .small()
                            .icon(IconName::Plus)
                            .label("新建模板")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_select(None, window, cx, true)
                            })),
                    ),
            )
            .child(
                div()
                    .id("profile-status")
                    .accessibility_id("profile-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
                    .flex_shrink_0()
                    .text_color(if self.error.is_some() {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(status),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .gap_4()
                    .overflow_y_scrollbar()
                    .p_1()
                    .children(
                        self.profiles
                            .iter()
                            .map(|profile| self.render_card(profile, disabled, cx)),
                    )
                    .when(self.profiles.is_empty(), |element| {
                        element.child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .flex_1()
                                .gap_3()
                                .p_8()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(IconName::Inbox)
                                .child("暂无覆写模板"),
                        )
                    }),
            )
    }
}
