use gpui_kit::{Context, Window, component::WindowExt as _};

use super::RulesView;

impl RulesView {
    pub(super) fn open_new_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("新建规则")
                .description("创建空规则后，再编辑匹配条件和动作。")
                .confirm()
                .ok_text("创建规则")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| this.add_rule(window, cx));
                    true
                })
        });
    }

    pub(super) fn open_new_rule_set(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("新建规则集")
                .description("创建空规则集后，再选择远程 URL 或手动 JSON。")
                .confirm()
                .ok_text("创建规则集")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| this.add_rule_set(window, cx));
                    true
                })
        });
    }
}
