use gpui_kit::{
    App, ElementId, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
    component::{
        scroll::ScrollableElement as _,
        tab::{Tab, TabBar},
    },
    div,
};

/// Shared workspace surface for a segmented tab and its connected editor.
/// GPUI Kit owns selection, accessibility and the sliding indicator; callers
/// keep only domain mapping and editor ownership.
pub(crate) fn workspace_tabs(
    id: impl Into<ElementId>,
    tabs: impl IntoIterator<Item = impl Into<SharedString>>,
    selected_index: usize,
    on_click: impl Fn(&usize, &mut Window, &mut App) + 'static,
    content: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(
            TabBar::new(id)
                .segmented()
                .selected_index(selected_index)
                .children(tabs.into_iter().map(|label| Tab::new().label(label)))
                .on_click(on_click),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .overflow_y_scrollbar()
                .child(crate::design::surface(cx).p_4().child(content)),
        )
}
