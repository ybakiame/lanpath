use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
};
use gpui_kit::{Context, IntoElement};
use lanpath_client::Subscription;

use super::{Operation, SubscriptionsView};

#[derive(Clone, Copy)]
enum SecondaryAction {
    Toggle,
    Export,
    Delete,
}

impl SubscriptionsView {
    pub(super) fn more_actions(
        &self,
        item: &Subscription,
        disabled: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let id = item.id;
        let view = cx.weak_entity();
        Button::new(format!("subscription-more-{id}"))
            .small()
            .ghost()
            .icon(IconName::Ellipsis)
            .label("更多")
            .disabled(disabled)
            .accessibility_label(format!("订阅 {} 的更多操作", item.name))
            .dropdown_menu(move |mut menu, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return menu;
                };
                let this = entity.read(cx);
                let enabled = this
                    .subscriptions
                    .iter()
                    .find(|item| item.id == id)
                    .is_some_and(|item| item.enabled);
                let disabled = this.locked || this.busy() || this.selected != Some(id);
                for (action, label, icon) in [
                    (
                        SecondaryAction::Toggle,
                        if enabled { "停用" } else { "启用" },
                        IconName::Power,
                    ),
                    (SecondaryAction::Export, "导出原文…", IconName::Download),
                    (SecondaryAction::Delete, "删除订阅…", IconName::Trash),
                ] {
                    if matches!(action, SecondaryAction::Delete) {
                        menu = menu.separator();
                    }
                    let view = view.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .icon(icon)
                            .disabled(disabled)
                            .on_click(move |_, window, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    if this.locked || this.busy() || this.selected != Some(id) {
                                        return;
                                    }
                                    match action {
                                        SecondaryAction::Toggle => {
                                            this.run(Operation::Toggle, window, cx)
                                        }
                                        SecondaryAction::Export => {
                                            this.run(Operation::Export, window, cx)
                                        }
                                        SecondaryAction::Delete => this.request_delete(window, cx),
                                    }
                                });
                            }),
                    );
                }
                menu
            })
    }
}
