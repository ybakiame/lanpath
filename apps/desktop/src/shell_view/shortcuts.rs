use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, div,
};

use crate::{DesktopView, Page};

const SHORTCUTS: [(&str, &str); 6] = [
    ("切换导航、内容与状态栏", "F6 / Shift+F6"),
    ("选择导航项目", "↑ / ↓ / Home / End"),
    ("打开导航项目", "Enter / Space"),
    ("保存当前表单", "Ctrl+S"),
    ("打开外观", "Ctrl+,"),
    ("关闭弹窗", "Esc"),
];

impl DesktopView {
    pub(super) fn render_shortcuts(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new("desktop-appearance")
                    .accessibility_id("desktop-appearance")
                    .ghost()
                    .small()
                    .icon(IconName::Palette)
                    .accessibility_label("外观")
                    .tooltip("外观与主题 · Ctrl+,")
                    .disabled(self.closing)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_page(Page::Appearance, cx);
                        this.content_focus.focus(window, cx);
                    })),
            )
            .child(
                Button::new("desktop-keyboard-help")
                    .accessibility_id("desktop-keyboard-help")
                    .ghost()
                    .small()
                    .icon(IconName::Keyboard)
                    .accessibility_label("键盘快捷键")
                    .tooltip("键盘快捷键…")
                    .disabled(self.closing)
                    .on_click(|_, window, cx| {
                        window.open_dialog(cx, |dialog, _, cx| {
                            dialog.title("键盘快捷键").child(
                                div()
                                    .id("desktop-keyboard-shortcuts")
                                    .accessibility_id("desktop-keyboard-shortcuts")
                                    .role(Role::Group)
                                    .aria_label(
                                        SHORTCUTS
                                            .map(|(label, shortcut)| format!("{label}：{shortcut}"))
                                            .join("\n"),
                                    )
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .children(SHORTCUTS.map(|(label, shortcut)| {
                                        div()
                                            .flex()
                                            .justify_between()
                                            .gap_4()
                                            .text_sm()
                                            .child(label)
                                            .child(
                                                div()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(shortcut),
                                            )
                                    })),
                            )
                        });
                    }),
            )
    }
}
