use gpui_kit::component::{Sizable as _, accordion::Accordion};
use gpui_kit::{
    AnyElement, App, ElementId, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window,
};

/// Low-priority help retains its open state by domain key across redraws.
/// Accordion owns activation, focus, expansion and accessibility semantics.
#[derive(IntoElement)]
pub(crate) struct HelpDisclosure {
    id: ElementId,
    title: &'static str,
    content: AnyElement,
}

pub(crate) fn help(
    id: &'static str,
    title: &'static str,
    content: impl IntoElement,
) -> HelpDisclosure {
    HelpDisclosure {
        id: id.into(),
        title,
        content: content.into_any_element(),
    }
}

impl RenderOnce for HelpDisclosure {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state((self.id.clone(), "expanded"), cx, |_, _| false);
        let open = *state.read(cx);
        Accordion::new(self.id)
            .small()
            .bordered(false)
            .h_auto()
            .w_full()
            .item(|item| item.title(self.title).open(open).child(self.content))
            .on_toggle_click(move |indices, _, cx| {
                state.update(cx, |open, cx| {
                    *open = !indices.is_empty();
                    cx.notify();
                });
            })
    }
}
