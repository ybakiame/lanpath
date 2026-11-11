use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
    tag::Tag,
};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div,
    prelude::FluentBuilder as _,
};
use lanpath_client::Profile;

use super::{Operation, ProfilesView};

impl ProfilesView {
    pub(super) fn render_card(
        &self,
        profile: &Profile,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = profile.id;
        let selected = Some(id) == self.selected;
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
                Button::new(format!("profile-{id}"))
                    .accessibility_label(format!("查看覆写 {}", profile.name))
                    .tooltip(profile.name.clone())
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
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::Layers)
                                            .small()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .font_semibold()
                                            .child(profile.name.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_1()
                                    .when(!profile.yaml_override.is_empty(), |element| {
                                        element.child(Tag::secondary().small().child("YAML"))
                                    })
                                    .when(!profile.js_override.is_empty(), |element| {
                                        element.child(Tag::secondary().small().child("JS"))
                                    })
                                    .when(profile.yaml_url.is_some(), |element| {
                                        element.child(Tag::info().small().child("远程 YAML"))
                                    })
                                    .when(profile.js_url.is_some(), |element| {
                                        element.child(Tag::info().small().child("远程 JS"))
                                    }),
                            ),
                    ),
            )
            .when(selected, |element| {
                element.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .pt_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(
                            Button::new("profile-edit")
                                .small()
                                .icon(IconName::SquarePen)
                                .label("编辑")
                                .disabled(disabled)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let selected = this.selected;
                                    this.request_select(selected, window, cx, true)
                                })),
                        )
                        .child(
                            Button::new("profile-preview")
                                .small()
                                .ghost()
                                .icon(IconName::FileText)
                                .label("查看内容")
                                .disabled(disabled)
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.open_preview(window, cx)
                                    }),
                                ),
                        )
                        .child(
                            Button::new("profile-delete")
                                .small()
                                .danger()
                                .outline()
                                .icon(IconName::Trash)
                                .label("删除")
                                .loading(matches!(self.busy, Some(Operation::Delete)))
                                .disabled(disabled)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.request_delete(window, cx)
                                })),
                        ),
                )
            })
            .into_any_element()
    }
}
