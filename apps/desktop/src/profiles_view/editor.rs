//! 覆写模板编辑弹窗：新建/编辑共用一个表单。名称、两个远程地址用单行
//! 输入；YAML/JS 本地覆写用 NSH 真实多行输入（保留换行，不丢行）。
//! 保存失败保留草稿并显示真实错误；取消有未保存修改需确认。

use nsh_base::{button, dialog, input, theme};
use nsh_core::prelude::*;

use super::{ProfilesPageState, save_edit};

pub(super) fn profile_editor_dialog(state: ProfilesPageState) -> AnyElement {
    if !state.edit.open.get() {
        return view().into_element();
    }
    let id = state.edit.id.get();
    let creating = id.is_none();
    let title = if creating {
        "新建覆写模板".to_owned()
    } else {
        state
            .list
            .get()
            .iter()
            .find(|profile| Some(profile.id) == id)
            .map(|profile| format!("编辑覆写模板“{}”", profile.name))
            .unwrap_or_else(|| "编辑覆写模板".to_owned())
    };
    let cancel_state = state.clone();
    let form_show = state.clone();
    let form_build = state.clone();
    let confirm_show = state.clone();
    let confirm_build = state;
    dialog()
        .on_cancel(move || cancel_state.edit.request_cancel())
        .child(
            view()
                .child(when(
                    move || !form_show.edit.confirm_discard.get(),
                    move || edit_form(form_build.clone(), title.clone()),
                ))
                .child(when(
                    move || confirm_show.edit.confirm_discard.get(),
                    move || discard_confirm(confirm_build.clone()),
                )),
        )
        .into_element()
}

fn edit_form(state: ProfilesPageState, title: String) -> impl IntoElement {
    let name = state.edit.name;
    let yaml_url = state.edit.yaml_url;
    let js_url = state.edit.js_url;
    let yaml_override = state.edit.yaml_override;
    let js_override = state.edit.js_override;
    let name_state = state.clone();
    let yaml_url_state = state.clone();
    let js_url_state = state.clone();
    let yaml_state = state.clone();
    let js_state = state.clone();
    let error_state = state.clone();
    let cancel_press = state.clone();
    let save_press = state;
    view()
        .flex_col()
        .gap_2()
        .w(px(520.0))
        .p_4()
        .rounded(move || theme().radius)
        .bg(move || theme().card)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text(title).text_size(px(15.0)))
        .child(
            text("远程覆写地址仅在配置合成时解析，本页不下载、不预览、不显示“已生效”。")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            input()
                .test_id("profile-name")
                .value(name)
                .placeholder("模板名称")
                .on_change(move |text: &str| {
                    name_state.edit.name.set(text.to_owned());
                    name_state.edit.dirty.set(true);
                })
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
        .child(
            input()
                .test_id("profile-yaml-url")
                .value(yaml_url)
                .placeholder("远程 YAML URL（可选，https://…）")
                .auto_correct(false)
                .on_change(move |text: &str| {
                    yaml_url_state.edit.yaml_url.set(text.to_owned());
                    yaml_url_state.edit.dirty.set(true);
                })
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
        .child(
            input()
                .test_id("profile-js-url")
                .value(js_url)
                .placeholder("远程 JS URL（可选，https://…）")
                .auto_correct(false)
                .on_change(move |text: &str| {
                    js_url_state.edit.js_url.set(text.to_owned());
                    js_url_state.edit.dirty.set(true);
                })
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
        .child(
            text("本地 YAML 覆写（可选，留空不启用）")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            input()
                .test_id("profile-yaml")
                .value(yaml_override)
                .placeholder("例如 dns:\n  enabled: true")
                .multiline(true)
                .auto_correct(false)
                .on_change(move |text: &str| {
                    yaml_state.edit.yaml_override.set(text.to_owned());
                    yaml_state.edit.dirty.set(true);
                })
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
        .child(
            text("本地 JS 覆写（可选，须定义 main 函数）")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            input()
                .test_id("profile-js")
                .value(js_override)
                .placeholder("function main(config) { … return config; }")
                .multiline(true)
                .auto_correct(false)
                .on_change(move |text: &str| {
                    js_state.edit.js_override.set(text.to_owned());
                    js_state.edit.dirty.set(true);
                })
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
        .child(
            text(move || error_state.edit.error.get().unwrap_or_default())
                .text_size(px(12.0))
                .text_color(move || theme().destructive),
        )
        .child(
            view()
                .flex_row()
                .gap_2()
                .justify_end()
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || cancel_press.edit.request_cancel())
                        .child(text("取消")),
                )
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .bg(move || theme().primary)
                        .on_press(move || save_edit(&save_press))
                        .child(text("保存").text_color(move || theme().primary_foreground)),
                ),
        )
}

fn discard_confirm(state: ProfilesPageState) -> impl IntoElement {
    let keep_state = state.clone();
    view()
        .flex_col()
        .gap_3()
        .w(px(420.0))
        .p_4()
        .rounded(move || theme().radius)
        .bg(move || theme().card)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text("放弃未保存的覆写修改？"))
        .child(
            text("草稿尚未保存，放弃后将丢失本次修改，已保存模板不受影响。")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_row()
                .gap_2()
                .justify_end()
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || state.edit.confirm_discard.set(false))
                        .child(text("继续编辑")),
                )
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .bg(move || theme().destructive)
                        .on_press(move || keep_state.edit.close())
                        .child(text("放弃修改").text_color(move || theme().primary_foreground)),
                ),
        )
}
