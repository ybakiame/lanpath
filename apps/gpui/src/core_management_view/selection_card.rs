use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, div,
};

use super::CoreManagementView;

impl CoreManagementView {
    pub(super) fn render_selected_core(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let core = self.active_core.clone();
        let summary = self.selected_core_summary();
        div()
            .id("core-selected-summary")
            .accessibility_id("core-selected-summary")
            .role(Role::Group)
            .aria_label(format!("当前核心：{summary}"))
            .flex()
            .items_center()
            .flex_wrap()
            .gap_3()
            .flex_shrink_0()
            .p_4()
            .border_1()
            .rounded_lg()
            .border_color(cx.theme().border)
            .bg(cx.theme().group_box)
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("当前核心"),
            )
            .child(div().font_semibold().child(summary))
            .child(div().flex_1())
            .children(core.map(|core| {
                Button::new("core-build-details")
                    .accessibility_id("core-build-details")
                    .ghost()
                    .small()
                    .label("构建详情…")
                    .on_click(move |_, window, cx| {
                        let detail = format!(
                            "{} · {}\nEnvironment: {}\nTags: {}\n{}",
                            core.core_type(),
                            core.version(),
                            core.environment().unwrap_or("未提供"),
                            core.tags().join(", "),
                            core.path().display()
                        );
                        window.open_dialog(cx, move |dialog, _, _| {
                            dialog.title("核心构建详情").child(
                                div()
                                    .id("core-build-information")
                                    .accessibility_id("core-build-information")
                                    .role(Role::Group)
                                    .aria_label(detail.clone())
                                    .text_sm()
                                    .min_w_0()
                                    .whitespace_normal()
                                    .child(detail.clone()),
                            )
                        });
                    })
            }))
    }
}
