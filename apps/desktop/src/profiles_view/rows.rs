//! 覆写模板列表：每行展示名称与覆写/远程地址配置摘要，提供编辑与删除
//! （删除需确认；被订阅引用时由业务层拒绝并显示真实错误）。底部挂载
//! 删除确认弹窗。

use lanpath_client::Profile;
use nsh_base::theme;
use nsh_core::prelude::*;

use crate::ui::icon::Icon;
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, surface};

use super::ProfilesPageState;

pub(super) fn profile_list(state: ProfilesPageState) -> impl IntoElement {
    let rows = state.list;
    let error = state.list_error;
    let notice = state.notice;
    let build = state.clone();
    let remove = state;
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .gap_3()
        .child(
            text(move || t!("profiles.count", count = rows.get().len()))
                .text_size(px(12.5))
                .text_color(move || theme().muted_foreground),
        )
        .child(when(
            move || error.get().is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || error.get().unwrap_or_default())
                        .text_size(px(12.5))
                        .text_color(move || theme().destructive),
                )
            },
        ))
        .child(when(
            move || notice.get().is_some(),
            move || {
                alert_banner(
                    AlertTone::Info,
                    text(move || notice.get().unwrap_or_default())
                        .text_size(px(12.5))
                        .text_color(move || theme().foreground),
                )
            },
        ))
        .child(
            virtual_list(
                move || rows.get().len(),
                move |at| {
                    let current = computed(move || {
                        rows.with(|rows| {
                            rows.get(at.index())
                                .cloned()
                                .into_iter()
                                .collect::<Vec<_>>()
                        })
                    });
                    let state = build.clone();
                    each(
                        current,
                        |profile: &Profile| profile.id,
                        move |item, _| profile_row(state.clone(), item),
                    )
                },
            )
            .keys(move || {
                rows.with(|rows| rows.iter().map(|r| r.id.to_string()).collect::<Vec<_>>())
            })
            .estimated_item_size(px(92.0))
            .flex_1()
            .min_h(px(80.0))
            .empty(crate::ui::empty_with_icon(
                Some(Icon::Code),
                || t!("profile.editor"),
                || t!("product.profiles.empty"),
            )),
        )
        .child(delete_dialog(remove))
}

fn profile_row(state: ProfilesPageState, item: ReadSignal<Profile>) -> impl IntoElement {
    let row_test_id = format!("profile-row-{}", item.get().name);
    let edit_state = state.clone();
    let delete_state = state;
    surface(SurfaceLevel::Card)
        .test_id(&row_test_id)
        .flex_col()
        .gap_2()
        .p_4()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    text(move || item.get().name.clone())
                        .text_size(px(14.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .min_w(px(0.0)),
                )
                .child(view().flex_1())
                .child(
                    crate::ui::button(crate::ui::ButtonVariant::Outline)
                        .size(crate::ui::ButtonSize::Sm)
                        .on_press(move || edit_state.edit.open_edit(&item.get()))
                        .label(|| t!("product.action.edit")),
                )
                .child(
                    crate::ui::button(crate::ui::ButtonVariant::Ghost)
                        .size(crate::ui::ButtonSize::Sm)
                        .on_press(move || delete_state.pending_remove.set(Some(item.get())))
                        .label(|| t!("product.action.delete")),
                ),
        )
        .child(
            view().flex_row().items_center().gap_2().child(
                when(
                    move || !item.get().js_override.trim().is_empty(),
                    || {
                        crate::ui::badge(crate::ui::BadgeVariant::Success, || {
                            "本地 TS 覆写：已填写".to_owned()
                        })
                    },
                )
                .otherwise(|| {
                    crate::ui::badge(crate::ui::BadgeVariant::Outline, || {
                        "本地 TS 覆写：未填写".to_owned()
                    })
                }),
            ),
        )
}

/// 删除确认弹窗：确认后由业务层在同一业务锁内拒绝仍被订阅引用的模板，
/// 真实错误上屏，不静默解除引用。
fn delete_dialog(state: ProfilesPageState) -> AnyElement {
    let target = state.pending_remove;
    let service = state.service.clone();
    crate::ui::confirm_dialog(move || target.get().is_some())
        .title(move || {
            target
                .get()
                .map(|p| format!("删除覆写模板“{}”？", p.name))
                .unwrap_or_default()
        })
        .description(|| t!("product.profiles.delete_hint"))
        .destructive()
        .confirm_label(|| t!("product.action.confirm_delete"))
        .on_cancel(move || target.set(None))
        .on_confirm(move || {
            let Some(profile) = target.get() else {
                return;
            };
            target.set(None);
            match service.as_ref().map(|service| service.remove(profile.id)) {
                Some(Ok(())) => {
                    state.reload();
                    state.post_notice("覆写模板已删除".into());
                }
                Some(Err(error)) => state.post_error(format!("删除覆写模板失败:{error}")),
                None => state.post_error("打开数据根失败".into()),
            }
        })
        .into_element()
}
