use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::{Editor, Input},
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _,
};

use super::{Operation, ProfilesView};

impl Render for ProfilesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().width < window.rem_size() * 64.0;
        let controls_max_height =
            (window.viewport_size().height * 0.35).min(window.rem_size() * 14.0);
        let disabled = self.locked || self.busy();
        let status = self
            .error
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| "YAML 深合并后执行 JS；远程覆写失败时使用独立目录缓存".into());
        let controls = div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child("覆写模板")
                    .child(
                        Button::new("profile-new")
                            .label("新建")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_select(None, window, cx)
                            })),
                    )
                    .child(
                        Button::new("profile-save")
                            .label("保存")
                            .primary()
                            .disabled(disabled || !self.dirty)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run(Operation::Save, window, cx)
                            })),
                    )
                    .child(
                        Button::new("profile-delete")
                            .label("删除")
                            .danger()
                            .disabled(disabled || self.selected.is_none())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.request_delete(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("profile-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
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
                    .flex_wrap()
                    .min_w_0()
                    .h_auto()
                    .max_h_24()
                    .overflow_y_scrollbar()
                    .gap_2()
                    .children(self.profiles.iter().map(|profile| {
                        let id = profile.id;
                        Button::new(format!("profile-{id}"))
                            .min_w_0()
                            .max_w_full()
                            .tooltip(profile.name.clone())
                            .disabled(disabled)
                            .label(profile.name.clone())
                            .selected(Some(id) == self.selected)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.request_select(Some(id), window, cx)
                            }))
                    })),
            )
            .child(
                Input::new(&self.name)
                    .aria_label("覆写模板名称")
                    .disabled(disabled),
            )
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .when(compact, |row| row.flex_col())
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.yaml_url)
                                .aria_label("远程 YAML URL")
                                .disabled(disabled),
                        ),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.js_url)
                                .aria_label("远程 JS URL")
                                .disabled(disabled),
                        ),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_2()
            .child(
                controls
                    .w_full()
                    .h_auto()
                    .flex_shrink_0()
                    .max_h(controls_max_height)
                    .overflow_y_scrollbar(),
            )
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .when(compact, |row| row.flex_col())
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .gap_1()
                            .child("YAML 深合并")
                            .child(
                                Editor::new(&self.yaml)
                                    .aria_label("YAML 覆写正文")
                                    .readonly(disabled)
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
                            .gap_1()
                            .child("JS main(config)")
                            .child(
                                Editor::new(&self.js)
                                    .aria_label("JS 覆写正文")
                                    .readonly(disabled)
                                    .size_full(),
                            ),
                    ),
            )
    }
}
