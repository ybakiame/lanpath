//! 覆写脚本的原生代码 Dialog；确认取消时保留 Editor 的选择、撤销和输入状态。
use super::{ProfilesPageState, save_edit};
use crate::ui::{
    button::{ButtonVariant, button},
    input::{field, input},
};
use nsh_base::theme;
use nsh_core::prelude::*;

pub(super) fn profile_editor_dialog(state: ProfilesPageState) -> AnyElement {
    let name = state.edit.name;
    let source = state.edit.js_override;
    let dirty = state.edit.dirty;
    let error = state.edit.error;
    let confirm = state.edit.confirm_discard;
    let cancel = state.clone();
    let footer_cancel = state.clone();
    let save = state.clone();
    let discard = state;
    let focus = element_ref();
    let body = view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .gap_3()
        .child(
            field().label(|| t!("profile.name")).control(
                input(name)
                    .element_ref(focus)
                    .test_id("profile-name")
                    .accessible_name(|| t!("profile.name"))
                    .on_change(move |value| {
                        name.set(value);
                        dirty.set(true);
                    }),
            ),
        )
        .child(crate::ui::code_editor::code_editor(source, dirty))
        .child(when(
            move || error.get().is_some(),
            move || {
                text(move || error.get().unwrap_or_default())
                    .text_color(move || theme().destructive)
            },
        ));
    let footer = view()
        .flex_row()
        .justify_end()
        .gap_3()
        .child(
            button(ButtonVariant::Outline)
                .label(|| t!("profile.cancel"))
                .on_press(move || footer_cancel.edit.request_cancel()),
        )
        .child(
            button(ButtonVariant::Primary)
                .label(|| t!("profile.save"))
                .on_press(move || save_edit(&save)),
        );
    view()
        .child(crate::ui::dialog::form_dialog(
            || t!("profile.editor"),
            crate::ui::dialog::DialogSize::Code,
            focus,
            move || cancel.edit.request_cancel(),
            body,
            footer,
        ))
        .child(
            crate::ui::confirm_dialog(move || confirm.get())
                .title(|| t!("profile.discard_question"))
                .description(|| t!("profile.discard_hint"))
                .destructive()
                .confirm_label(|| t!("profile.discard"))
                .cancel_label(|| t!("profile.keep"))
                .on_cancel(move || confirm.set(false))
                .on_confirm(move || discard.edit.close()),
        )
        .into_element()
}
