//! 应用内规则集分组侧边面板：供「规则集」与「规则」页共用，聚合多个规则集为 `@<tag>`，
//! 作为附加操作保留主页面上下文，变更后实时同步主页面分组列表。

use lanpath_client::rulesets::RuleSetGroup;
use nsh_base::theme;
use nsh_core::prelude::*;

use super::group_cards::{group_card, group_editor_card};
use crate::rulesets::{RuleSetRow, RuleSetsService};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, surface};

/// 可由宿主页面持有并触发打开/关闭的规则集分组侧边面板控制器。
#[derive(Clone)]
pub(crate) struct GroupSheetController {
    open: Signal<bool>,
    retained: Signal<bool>,
    pub(super) items: Signal<Vec<RuleSetRow>>,
    pub(super) groups: Signal<Vec<RuleSetGroup>>,
    editor_open: Signal<bool>,
    pub(super) editing_id: Signal<Option<String>>,
    pub(super) draft_tag: Signal<String>,
    pub(super) draft_name: Signal<String>,
    pub(super) draft_desc: Signal<String>,
    pub(super) draft_members: Signal<Vec<String>>,
    pub(super) pending_delete: Signal<Option<String>>,
    error: Signal<Option<String>>,
    notice: Signal<Option<String>>,
}

impl GroupSheetController {
    pub(crate) fn new(items: Signal<Vec<RuleSetRow>>, groups: Signal<Vec<RuleSetGroup>>) -> Self {
        Self {
            open: signal(false),
            retained: signal(false),
            items,
            groups,
            editor_open: signal(false),
            editing_id: signal(None),
            draft_tag: signal(String::new()),
            draft_name: signal(String::new()),
            draft_desc: signal(String::new()),
            draft_members: signal(Vec::new()),
            pending_delete: signal(None),
            error: signal(None),
            notice: signal(None),
        }
    }

    pub(crate) fn open_panel(&self) {
        self.error.set(None);
        self.notice.set(None);
        self.pending_delete.set(None);
        self.refresh_from_disk();
        self.retained.set(true);
        self.open.set(true);
    }

    pub(crate) fn close_panel(&self) {
        self.open.set(false);
        self.editor_open.set(false);
        self.editing_id.set(None);
        self.pending_delete.set(None);
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.open.get() && self.editor_open.get()
    }

    fn refresh_from_disk(&self) {
        if let Ok(service) = RuleSetsService::open()
            && let Ok(snapshot) = service.list_snapshot()
        {
            self.items.set(snapshot.items);
            self.groups.set(snapshot.groups);
        }
    }

    fn open_add(&self) {
        self.editing_id.set(None);
        self.draft_tag.set(String::new());
        self.draft_name.set(String::new());
        self.draft_desc.set(String::new());
        self.draft_members.set(Vec::new());
        self.pending_delete.set(None);
        self.error.set(None);
        self.editor_open.set(true);
    }

    pub(super) fn open_edit(&self, group: &RuleSetGroup) {
        self.editing_id.set(Some(group.id.clone()));
        self.draft_tag.set(group.tag.clone());
        self.draft_name.set(group.name.clone());
        self.draft_desc.set(group.description.clone());
        self.draft_members.set(group.member_tags.clone());
        self.pending_delete.set(None);
        self.error.set(None);
        self.editor_open.set(true);
    }

    pub(super) fn close_editor(&self) {
        self.editor_open.set(false);
        self.editing_id.set(None);
    }

    pub(super) fn toggle_member(&self, tag: &str) {
        self.draft_members.update(|members| {
            if let Some(pos) = members.iter().position(|t| t.eq_ignore_ascii_case(tag)) {
                members.remove(pos);
            } else {
                members.push(tag.to_owned());
            }
        });
    }

    pub(super) fn save_editor(&self) {
        let Ok(service) = RuleSetsService::open() else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        let group = RuleSetGroup {
            id: self.editing_id.get().unwrap_or_default(),
            tag: self.draft_tag.get(),
            name: self.draft_name.get(),
            description: self.draft_desc.get(),
            member_tags: self.draft_members.get(),
            created_at: 0,
        };
        match service.upsert_group(group) {
            Ok(snapshot) => {
                self.items.set(snapshot.items);
                self.groups.set(snapshot.groups);
                self.error.set(None);
                self.notice.set(Some(t!("rulesets.group.saved")));
                self.close_editor();
            }
            Err(err) => self.error.set(Some(err.to_string())),
        }
    }

    pub(super) fn confirm_remove(&self, id: &str) {
        let Ok(service) = RuleSetsService::open() else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        match service.delete_group(id) {
            Ok(snapshot) => {
                self.items.set(snapshot.items);
                self.groups.set(snapshot.groups);
                self.pending_delete.set(None);
                self.error.set(None);
                self.notice.set(Some(t!("rulesets.group.deleted")));
            }
            Err(err) => {
                self.pending_delete.set(None);
                self.error.set(Some(err.to_string()));
            }
        }
    }
}

/// 渲染共享的右侧规则集分组管理 Sheet。
pub(crate) fn groups_sheet(controller: GroupSheetController) -> AnyElement {
    let retained = controller.retained;
    let open = controller.open;
    let close_ctrl = controller.clone();
    let closed_ctrl = controller.clone();
    let body_ctrl = controller;

    when(
        move || retained.get(),
        move || {
            let on_close = close_ctrl.clone();
            let on_closed = closed_ctrl.clone();
            let body = sheet_body(body_ctrl.clone());
            crate::ui::sheet::sheet(
                540.0,
                || t!("rulesets.group.sheet_title"),
                move || open.get(),
                move || on_close.close_panel(),
                move || {
                    if !on_closed.open.get() {
                        on_closed.retained.set(false);
                    }
                },
                body,
            )
        },
    )
    .into_element()
}

fn sheet_body(ctrl: GroupSheetController) -> AnyElement {
    let groups = ctrl.groups;
    let editor_open = ctrl.editor_open;
    let error = ctrl.error;
    let notice = ctrl.notice;
    let add_ctrl = ctrl.clone();
    let empty_add = ctrl.clone();
    let editor_ctrl = ctrl.clone();
    let list_ctrl = ctrl;

    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .gap_4()
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    text(|| t!("rulesets.group.sheet_hint"))
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground)
                        .flex_1(),
                )
                .child(
                    button(ButtonVariant::Primary)
                        .test_id("ruleset-group-add")
                        .size(ButtonSize::Sm)
                        .prefix(|| icon_sized(Icon::Plus, Tone::OnPrimary, 13.0).into_element())
                        .label(|| t!("rulesets.group.add"))
                        .on_press(move || add_ctrl.open_add()),
                ),
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
                    AlertTone::Success,
                    text(move || notice.get().unwrap_or_default())
                        .text_size(px(12.5))
                        .text_color(move || theme().foreground),
                )
            },
        ))
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .flex_col()
                    .gap_4()
                    .child(when(
                        move || editor_open.get(),
                        move || group_editor_card(editor_ctrl.clone()),
                    ))
                    .child(when(
                        move || groups.with(Vec::is_empty) && !editor_open.get(),
                        move || {
                            let open_add = empty_add.clone();
                            surface(SurfaceLevel::Card)
                                .p_6()
                                .flex_col()
                                .items_center()
                                .gap_3()
                                .child(icon_sized(Icon::Layers, Tone::Muted, 28.0))
                                .child(
                                    text(|| t!("rulesets.group.empty"))
                                        .text_size(px(13.0))
                                        .text_color(move || theme().muted_foreground),
                                )
                                .child(
                                    button(ButtonVariant::Primary)
                                        .test_id("ruleset-group-empty-add")
                                        .size(ButtonSize::Md)
                                        .prefix(|| {
                                            icon_sized(Icon::Plus, Tone::OnPrimary, 14.0)
                                                .into_element()
                                        })
                                        .label(|| t!("rulesets.group.add"))
                                        .on_press(move || open_add.open_add()),
                                )
                        },
                    ))
                    .child(view().flex_col().gap_3().child(each(
                        move || groups.get(),
                        |g: &RuleSetGroup| {
                            format!("{}:{}:{}", g.id, g.tag, g.member_tags.join(","))
                        },
                        move |group, _| group_card(list_ctrl.clone(), group.get_untracked()),
                    ))),
            ),
        )
        .into_element()
}
