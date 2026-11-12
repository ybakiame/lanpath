//! 覆写模板列表：每行展示名称与覆写/远程地址配置摘要，提供编辑与删除
//! （删除需确认；被订阅引用时由业务层拒绝并显示真实错误）。底部挂载
//! 删除确认弹窗。

use lanpath_client::Profile;
use nsh_base::{button, dialog, theme};
use nsh_core::prelude::*;

use super::ProfilesPageState;

pub(super) fn profile_list(state: ProfilesPageState) -> impl IntoElement {
    let count_state = state.clone();
    let empty_state = state.clone();
    let message_state = state.clone();
    let notice_state = state.clone();
    let row_state = state.clone();
    let remove_dialog_state = state;
    scroll().flex_1().child(
        view()
            .flex_col()
            .gap_2()
            .child(
                text(move || format!("共 {} 个覆写模板", count_state.list.get().len()))
                    .text_color(move || theme().muted_foreground),
            )
            .child(when(
                move || empty_state.list.get().is_empty() && empty_state.list_error.get().is_none(),
                || text("尚无覆写模板。").text_color(move || theme().muted_foreground),
            ))
            .child(
                text(move || message_state.list_error.get().unwrap_or_default())
                    .text_color(move || theme().destructive),
            )
            .child(
                text(move || notice_state.notice.get().unwrap_or_default())
                    .text_color(move || theme().muted_foreground),
            )
            .child(each(
                row_state.list,
                |profile: &Profile| profile.id,
                move |item, _index| profile_row(row_state.clone(), item),
            ))
            .child(when(
                move || remove_dialog_state.pending_remove.get().is_some(),
                move || delete_dialog(remove_dialog_state.clone()),
            )),
    )
}

fn profile_row(state: ProfilesPageState, item: ReadSignal<Profile>) -> impl IntoElement {
    let row_test_id = format!("profile-row-{}", item.get().name);
    let edit_state = state.clone();
    let delete_state = state;
    view()
        .test_id(&row_test_id)
        .flex_col()
        .gap_1()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .bg(move || theme().card)
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    text(move || item.get().name.clone())
                        .text_size(px(15.0))
                        .truncate()
                        .min_w(px(0.0)),
                )
                .child(view().flex_1())
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || edit_state.edit.open_edit(&item.get()))
                        .child(text("编辑").text_size(px(12.0))),
                )
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || delete_state.pending_remove.set(Some(item.get())))
                        .child(text("删除").text_size(px(12.0))),
                ),
        )
        .child(
            text(move || {
                let profile = item.get();
                let mut summary = String::new();
                summary.push_str(if profile.yaml_override.trim().is_empty() {
                    "本地 YAML 覆写：未填写"
                } else {
                    "本地 YAML 覆写：已填写"
                });
                summary.push_str(if profile.js_override.trim().is_empty() {
                    " · 本地 JS 覆写：未填写"
                } else {
                    " · 本地 JS 覆写：已填写"
                });
                summary.push_str(if profile.yaml_url.is_some() {
                    " · 远程 YAML：已配置"
                } else {
                    " · 远程 YAML：未配置"
                });
                summary.push_str(if profile.js_url.is_some() {
                    " · 远程 JS：已配置"
                } else {
                    " · 远程 JS：未配置"
                });
                summary
            })
            .text_size(px(12.0))
            .text_color(move || theme().muted_foreground),
        )
        .child(
            text("远程覆写地址仅在配置合成时解析，本页不下载、不预览。")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
}

/// 删除确认弹窗：确认后由业务层在同一业务锁内拒绝仍被订阅引用的模板，
/// 真实错误上屏，不静默解除引用。
fn delete_dialog(state: ProfilesPageState) -> AnyElement {
    match state.pending_remove.get() {
        Some(target) => {
            let remove_id = target.id;
            let service = state.service.clone();
            let cancel_state = state.clone();
            let confirm_state = state;
            dialog()
                .on_cancel(move || cancel_state.pending_remove.set(None))
                .child(
                    view()
                        .flex_col()
                        .gap_3()
                        .w(px(420.0))
                        .p_4()
                        .rounded(move || theme().radius)
                        .bg(move || theme().card)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .child(text(format!("删除覆写模板“{}”？", target.name)))
                        .child(
                            text("被订阅关联时删除会被拒绝；仅移除 NSH 桌面数据根中的模板，不影响 Tauri/GPUI/apps/nsh 数据。")
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
                                        .on_press(move || {
                                            cancel_state.pending_remove.set(None)
                                        })
                                        .child(text("取消")),
                                )
                                .child(
                                    button()
                                        .px_3()
                                        .py_1()
                                        .rounded(move || theme().radius)
                                        .bg(move || theme().destructive)
                                        .on_press(move || {
                                            confirm_state.pending_remove.set(None);
                                            let result = service
                                                .as_ref()
                                                .map(|service| service.remove(remove_id));
                                            match result {
                                                Some(Ok(())) => {
                                                    confirm_state.reload();
                                                    confirm_state
                                                        .post_notice("覆写模板已删除".to_owned());
                                                }
                                                Some(Err(error)) => {
                                                    confirm_state.reload();
                                                    confirm_state.post_error(format!(
                                                        "删除覆写模板失败:{error}"
                                                    ));
                                                }
                                                None => confirm_state
                                                    .post_error("打开数据根失败".to_owned()),
                                            }
                                        })
                                        .child(
                                            text("确认删除")
                                                .text_color(move || theme().primary_foreground),
                                        ),
                                ),
                        )
                        .into_element(),
                )
                .into_element()
        }
        None => view().into_element(),
    }
}
