use super::{DIALECT_SCOPE_NOTICE, Operation, ScriptsView, Section};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::{Button, ButtonVariants as _},
    scroll::ScrollableElement as _,
    tab::{Tab, TabBar},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

impl Render for ScriptsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let selected = match self.section {
            Section::Remotes => 0,
            Section::Tasks => 1,
            Section::Local => 2,
        };
        let status = self
            .error
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| {
                if self.operation == Some(Operation::AutomaticFetch) {
                    "正在自动更新缓存…".into()
                } else if self.dirty() {
                    "有未保存内容，自动更新暂缓".into()
                } else if self.running {
                    "核心运行中 · 缓存更新在下次启动生效".into()
                } else {
                    format!(
                        "{} 个资源 · {} 个启用",
                        self.resources.len(),
                        self.resources
                            .iter()
                            .filter(|resource| resource.enabled)
                            .count()
                    )
                }
            });
        div().flex().flex_col().flex_1().min_h_0().min_w_0().gap_3()
            .child(crate::design::page_header("脚本与片段","管理远程与本地资源，查看定时任务",
                crate::design::toolbar().children((self.section == Section::Remotes).then(|| {
                    crate::design::toolbar()
                        .child(Button::new("scripts-fetch").label(if matches!(self.operation,Some(Operation::Fetch | Operation::AutomaticFetch)) { "更新中…" } else { "更新资源" })
                            .disabled(disabled || self.dirty() || self.resources.is_empty())
                            .on_click(cx.listener(|this,_,window,cx| this.fetch_resources(window,cx))))
                        .child(Button::new("remote-add").primary().label("添加资源…").disabled(disabled)
                            .on_click(cx.listener(|this,_,window,cx| this.start_new(window,cx))))
                })).children((self.section == Section::Local).then(|| Button::new("local-add").accessibility_id("local-add").primary().label("添加本地资源…").disabled(disabled).on_click(cx.listener(|this,_,window,cx| this.edit_local(None,window,cx))))),cx))
            .child(TabBar::new("scripts-sections").segmented().selected_index(selected)
                .children(["远程资源","定时任务","本地资源"].map(|label| Tab::new().label(label)))
                .on_click(cx.listener(|this,index: &usize,_,cx| {
                    if let Some(section) = [Section::Remotes,Section::Tasks,Section::Local].get(*index) { this.set_section(*section,cx); }
                })))
            .child(div().id("scripts-content-scroll").accessibility_id("scripts-content-scroll")
                .role(Role::Group).aria_label("脚本内容滚动区域").flex_1().min_h_0().min_w_0()
                .child(div().size_full().overflow_y_scrollbar().child(match self.section {
                    Section::Remotes => self.render_remotes(disabled,cx).into_any_element(),
                    Section::Tasks => self.render_tasks(cx).into_any_element(),
                    Section::Local => self.render_local(disabled,cx).into_any_element(),
                })))
            .child(div().id("scripts-status").accessibility_id("scripts-status").role(Role::Status).aria_label(status.clone())
                .text_sm().max_h_16().min_w_0().flex_shrink_0().overflow_y_scroll()
                .text_color(if self.error.is_some() { cx.theme().danger } else { cx.theme().muted_foreground }).child(status))
            .child(crate::design::help("scripts-help","更新与兼容范围",div().flex().flex_col().gap_2().text_sm()
                .child("启用的资源按保存间隔自动更新缓存；存在草稿或其他业务操作时暂缓。缓存更新不替换正在运行的脚本、任务或路由，下次启动时加载。纯 JS 不自动绑定 hook。")
                .child(DIALECT_SCOPE_NOTICE)))
            .into_any_element()
    }
}

pub(super) fn section_title(
    title: impl Into<String>,
    description: impl Into<String>,
    cx: &Context<ScriptsView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().font_semibold().child(title.into()))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description.into()),
        )
}
