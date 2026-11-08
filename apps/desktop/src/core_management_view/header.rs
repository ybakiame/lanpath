use gpui_kit::base::{Selectable as _, StyledExt as _};
use gpui_kit::component::{
    Disableable as _, Sizable as _,
    button::{Button, ButtonGroup},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, div,
};
use lanpath_common::CoreType;

use super::CoreManagementView;

const CORE_TYPES: [(CoreType, &str); 2] = [
    (CoreType::SingBox, "sing-box"),
    (CoreType::Mihomo, "Mihomo"),
];

impl CoreManagementView {
    pub(super) fn render_header(
        &self,
        disabled: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id("core-management-header")
            .accessibility_id("core-management-header")
            .role(Role::Group)
            .aria_label("核心管理")
            .flex()
            .items_center()
            .flex_wrap()
            .flex_shrink_0()
            .gap_3()
            .child(div().text_xl().font_semibold().child("核心"))
            .child(div().flex_1())
            .child(
                ButtonGroup::new("core-type-switcher")
                    .small()
                    .compact()
                    .outline()
                    .disabled(disabled)
                    .children(CORE_TYPES.iter().map(|(kind, label)| {
                        Button::new(format!("core-type-{}", kind.as_str()))
                            .accessibility_id(format!("core-type-{}", kind.as_str()))
                            .label(*label)
                            .selected(*kind == self.selected_core)
                    }))
                    .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                        if let Some((kind, _)) =
                            indices.first().and_then(|index| CORE_TYPES.get(*index))
                        {
                            this.select_core_type(*kind, cx);
                        }
                    })),
            )
            .child(
                Button::new("core-refresh")
                    .accessibility_id("core-refresh")
                    .small()
                    .icon(gpui_kit::assets::IconName::RefreshCw)
                    .label("刷新")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            )
    }
}
