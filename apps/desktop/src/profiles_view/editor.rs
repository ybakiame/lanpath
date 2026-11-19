//! TypeScript 覆写编辑：共享语义控件与脏草稿保护。
use super::{ProfilesPageState, save_edit};
use crate::ui::{
    button::{ButtonVariant, button},
    input::{field, input, textarea},
    layout::{SurfaceLevel, surface},
};
use nsh_core::prelude::*;

pub(super) fn profile_editor_dialog(state: ProfilesPageState) -> AnyElement {
    let cancel = state.clone();
    let form = state.clone();
    let confirm = state.clone();
    crate::ui::overlay::editor_dialog(
        || t!("profile.editor"),
        move || cancel.edit.request_cancel(),
        surface(SurfaceLevel::Popover)
            .w(px(520.0))
            .max_h(px(560.0))
            .p_4()
            .child(
                scroll().child(
                    view()
                        .flex_col()
                        .gap_3()
                        .child(when(
                            move || !state.edit.confirm_discard.get(),
                            move || edit_form(form.clone()),
                        ))
                        .child(when(
                            move || confirm.edit.confirm_discard.get(),
                            move || discard_confirm(confirm.clone()),
                        )),
                ),
            ),
    )
}

fn edit_form(state: ProfilesPageState) -> impl IntoElement {
    let name = state.edit.name;
    let url = state.edit.js_url;
    let source = state.edit.js_override;
    let dirty = state.edit.dirty;
    let error = state.edit.error;
    let cancel = state.clone();
    let save = state;
    view()
        .flex_col()
        .gap_3()
        .child(crate::ui::layout::section_title(|| t!("profile.editor")))
        .child(
            field().label(|| t!("profile.name")).control(
                input(name)
                    .test_id("profile-name")
                    .accessible_name(|| t!("profile.name"))
                    .on_change(move |v| {
                        name.set(v);
                        dirty.set(true);
                    }),
            ),
        )
        .child(
            field()
                .label(|| t!("profile.url"))
                .hint(|| t!("profile.url_hint"))
                .control(
                    input(url)
                        .test_id("profile-js-url")
                        .accessible_name(|| t!("profile.url"))
                        .on_change(move |v| {
                            url.set(v);
                            dirty.set(true);
                        }),
                ),
        )
        .child(
            field()
                .label(|| t!("profile.source"))
                .hint(|| t!("profile.source_hint"))
                .control(
                    textarea(source)
                        .test_id("profile-js")
                        .accessible_name(|| t!("profile.source"))
                        .on_change(move |v| {
                            source.set(v);
                            dirty.set(true);
                        }),
                ),
        )
        .child(
            text(move || error.get().unwrap_or_default())
                .text_color(move || nsh_base::theme().destructive),
        )
        .child(
            view()
                .flex_row()
                .gap_3()
                .child(
                    button(ButtonVariant::Ghost)
                        .label(|| t!("profile.cancel"))
                        .on_press(move || cancel.edit.request_cancel()),
                )
                .child(
                    button(ButtonVariant::Primary)
                        .label(|| t!("profile.save"))
                        .on_press(move || save_edit(&save)),
                ),
        )
}
fn discard_confirm(state: ProfilesPageState) -> impl IntoElement {
    let keep = state.clone();
    view()
        .flex_col()
        .gap_3()
        .child(text(|| t!("profile.discard_question")))
        .child(
            button(ButtonVariant::Outline)
                .label(|| t!("profile.keep"))
                .on_press(move || keep.edit.confirm_discard.set(false)),
        )
        .child(
            button(ButtonVariant::Destructive)
                .label(|| t!("profile.discard"))
                .on_press(move || state.edit.close()),
        )
}
