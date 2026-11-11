use std::{ops::Range, rc::Rc};

use gpui_kit::component::{
    ActiveTheme as _, Collapsible, Disableable as _, Sizable as _, ThemeStyled as _,
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    list::{ListDelegate, ListState},
    sidebar::{SidebarItem, SidebarMenuItem},
};
use gpui_kit::{
    App, Context, ElementId, FocusHandle, Focusable as _, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Styled as _, WeakEntity, Window, canvas, div,
    prelude::FluentBuilder as _,
};

use crate::{DesktopView, Page};

gpui_kit::actions!(
    desktop,
    [CycleRegion, PreviousRegion, SaveCurrentForm, OpenAppearance]
);
gpui_kit::actions!(desktop_navigation, [Next, Previous, First, Last, Activate]);

pub(crate) const PAGES: [Page; 19] = [
    Page::Subscriptions,
    Page::Proxies,
    Page::Connections,
    Page::Stats,
    Page::Profiles,
    Page::Rules,
    Page::Structured,
    Page::Configuration,
    Page::Logs,
    Page::Diagnostics,
    Page::Scripts,
    Page::Mitm,
    Page::System,
    Page::Cores,
    Page::Requests,
    Page::Appearance,
    Page::Backup,
    Page::Distribution,
    Page::About,
];

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("f6", CycleRegion, Some("Desktop")),
        KeyBinding::new("shift-f6", PreviousRegion, Some("Desktop")),
        KeyBinding::new("ctrl-s", SaveCurrentForm, Some("Desktop")),
        KeyBinding::new("ctrl-,", OpenAppearance, Some("Desktop")),
        KeyBinding::new("down", Next, Some("DesktopNavigation")),
        KeyBinding::new("up", Previous, Some("DesktopNavigation")),
        KeyBinding::new("home", First, Some("DesktopNavigation")),
        KeyBinding::new("end", Last, Some("DesktopNavigation")),
        KeyBinding::new("enter", Activate, Some("DesktopNavigation")),
        KeyBinding::new("space", Activate, Some("DesktopNavigation")),
    ]);
}

/// Opt the retained non-searchable List handle into GPUI's existing Tab traversal.
pub(crate) fn list_state<D: ListDelegate>(
    delegate: D,
    window: &mut Window,
    cx: &mut Context<ListState<D>>,
) -> ListState<D> {
    let state = ListState::new(delegate, window, cx);
    // FocusHandle::tab_stop updates the shared registry; track_focus clones the
    // retained handle from that registry when the List paints.
    let _ = state.focus_handle(cx).tab_stop(true);
    state
}

#[derive(Clone)]
pub(crate) struct NavigationMenu {
    view: WeakEntity<DesktopView>,
    handles: Rc<[FocusHandle; 19]>,
    active: Page,
    disabled: bool,
    expanded: [bool; 4],
    core_type: lanpath_common::CoreType,
}

impl NavigationMenu {
    pub(crate) fn new(view: &DesktopView, cx: &Context<DesktopView>) -> Self {
        Self {
            view: cx.weak_entity(),
            handles: view.navigation_focus.clone(),
            active: view.page,
            disabled: view.closing,
            expanded: view.navigation_expanded,
            core_type: view.core.selected_core_type(),
        }
    }

    fn group(&self, title: &'static str, pages: Range<usize>) -> NavigationItems {
        NavigationItems {
            menu: self.clone(),
            pages,
            title,
        }
    }
}

impl Collapsible for NavigationMenu {
    fn is_collapsed(&self) -> bool {
        false
    }
    fn collapsed(self, _: bool) -> Self {
        self
    }
}

impl SidebarItem for NavigationMenu {
    fn render(
        self,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        // The fixed destination tree uses an ordinary scroll owner. Groups retain
        // their domain focus handles while collapsed and reveal on keyboard entry.
        div()
            .id(id)
            .flex()
            .flex_col()
            .gap_3()
            .child(
                self.group("工作区", 0..4)
                    .render("workspace-navigation", window, cx),
            )
            .child(
                self.group(
                    crate::configuration_workspace::configuration_title(self.core_type),
                    4..8,
                )
                .render("configuration-navigation", window, cx),
            )
            .child(
                self.group("工具与扩展", 8..12)
                    .render("extension-navigation", window, cx),
            )
            .child(
                self.group("设置", 12..PAGES.len())
                    .render("settings-navigation", window, cx),
            )
    }
}

#[derive(Clone)]
struct NavigationItems {
    menu: NavigationMenu,
    pages: Range<usize>,
    title: &'static str,
}

impl Collapsible for NavigationItems {
    fn is_collapsed(&self) -> bool {
        false
    }
    fn collapsed(self, _: bool) -> Self {
        self
    }
}

impl SidebarItem for NavigationItems {
    fn render(
        self,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let group = self.pages.start / 4;
        let expanded = self.menu.expanded[group];
        let view = self.menu.view.clone();
        div()
            .id(id)
            .flex()
            .flex_col()
            .gap_1()
            .child(
                Button::new(("navigation-group", group))
                    .ghost()
                    .small()
                    .w_full()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .w_full()
                            .gap_2()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                gpui_kit::component::Icon::new(if expanded {
                                    gpui_kit::assets::IconName::ChevronDown
                                } else {
                                    gpui_kit::assets::IconName::ChevronRight
                                })
                                .xsmall(),
                            )
                            .child(self.title),
                    )
                    .accessibility_label(format!(
                        "{}{}",
                        if expanded { "收起" } else { "展开" },
                        self.title
                    ))
                    .disabled(self.menu.disabled)
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |view, cx| {
                            if !view.closing {
                                view.navigation_expanded[group] = !view.navigation_expanded[group];
                                cx.notify();
                            }
                        });
                    }),
            )
            .children(
                self.pages
                    .filter(|index| expanded && PAGES[*index].visible_for(self.menu.core_type))
                    .map(|index| {
                        let page = PAGES[index];
                        NavigationItem {
                            view: self.menu.view.clone(),
                            handle: self.menu.handles[index].clone(),
                            page,
                            active: self.menu.active == page,
                            disabled: self.menu.disabled,
                        }
                        .render(("navigation-page", page as usize), window, cx)
                        .into_any_element()
                    }),
            )
    }
}

#[derive(Clone)]
struct NavigationItem {
    view: WeakEntity<DesktopView>,
    handle: FocusHandle,
    page: Page,
    active: bool,
    disabled: bool,
}

impl Collapsible for NavigationItem {
    fn is_collapsed(&self) -> bool {
        false
    }
    fn collapsed(self, _: bool) -> Self {
        self
    }
}

impl SidebarItem for NavigationItem {
    fn render(
        self,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let page = self.page;
        let (label, icon) = page.presentation();
        let view = self.view;
        let click_view = view.clone();
        let reveal = self.handle.clone();
        let focused = self.handle.is_focused(window);
        div()
            .id(id)
            .relative()
            .rounded(cx.theme().radius)
            .when(!self.disabled, |element| element.track_focus(&self.handle))
            .when(focused, |element| element.focus_ring_style(window, cx))
            .key_context("DesktopNavigation")
            .on_action({
                let view = view.clone();
                move |_: &Next, window, cx| {
                    let _ = view.update(cx, |view, cx| view.move_navigation(page, 1, window, cx));
                }
            })
            .on_action({
                let view = view.clone();
                move |_: &Previous, window, cx| {
                    let _ = view.update(cx, |view, cx| view.move_navigation(page, -1, window, cx));
                }
            })
            .on_action({
                let view = view.clone();
                move |_: &First, window, cx| {
                    let _ = view.update(cx, |view, cx| view.focus_navigation(PAGES[0], window, cx));
                }
            })
            .on_action({
                let view = view.clone();
                move |_: &Last, window, cx| {
                    let _ =
                        view.update(cx, |view, cx| view.focus_navigation(PAGES[18], window, cx));
                }
            })
            .on_action(move |_: &Activate, _, cx| {
                let _ = view.update(cx, |view, cx| view.set_page(page, cx));
            })
            .child(
                SidebarMenuItem::new(label)
                    .icon(icon)
                    .active(self.active)
                    .disable(self.disabled)
                    .on_click(move |_, window, cx| {
                        let _ = click_view.update(cx, |view, cx| {
                            view.focus_navigation(page, window, cx);
                            view.set_page(page, cx);
                        });
                    })
                    .render("destination", window, cx),
            )
            .child(
                canvas(
                    move |bounds, window, _| {
                        if reveal.is_focused(window) {
                            window.request_autoscroll(bounds);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
}

impl DesktopView {
    pub(crate) fn focus_navigation(
        &mut self,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.closing {
            let page = self.available_page(page);
            let Some(index) = PAGES.iter().position(|&candidate| candidate == page) else {
                return;
            };
            self.navigation_expanded[page.group_index()] = true;
            self.navigation_focus[index].focus(window, cx);
            cx.notify();
        }
    }

    fn move_navigation(
        &mut self,
        page: Page,
        offset: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pages: Vec<_> = PAGES
            .into_iter()
            .filter(|page| page.visible_for(self.core.selected_core_type()))
            .collect();
        let Some(index) = pages.iter().position(|&candidate| candidate == page) else {
            return;
        };
        let next = (index as isize + offset).rem_euclid(pages.len() as isize) as usize;
        self.focus_navigation(pages[next], window, cx);
    }

    pub(crate) fn cycle_region(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closing || window.has_active_dialog(cx) || window.has_active_sheet(cx) {
            return;
        }
        let current: i32 = if self.status_focus.contains_focused(window, cx) {
            2
        } else if self.content_focus.contains_focused(window, cx) {
            1
        } else {
            0
        };
        match (current + if reverse { -1 } else { 1 }).rem_euclid(3) {
            0 => self.focus_navigation(self.page, window, cx),
            1 => self.content_focus.focus(window, cx),
            _ => self.status_focus.focus(window, cx),
        }
        cx.notify();
    }

    pub(crate) fn save_current_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || window.has_active_dialog(cx) || window.has_active_sheet(cx) {
            return;
        }
        match self.page {
            Page::Profiles => self
                .profiles
                .update(cx, |view, cx| view.save_from_shortcut(window, cx)),
            Page::Subscriptions => self
                .subscriptions
                .update(cx, |view, cx| view.save_from_shortcut(window, cx)),
            Page::Structured => self.save_structured_section(window, cx),
            Page::Rules => self.rules.update(cx, |view, cx| view.save(window, cx)),
            Page::Scripts => self
                .scripts
                .update(cx, |view, cx| view.save_from_shortcut(window, cx)),
            Page::Mitm => self.mitm.update(cx, |view, cx| view.save(window, cx)),
            Page::Requests => self.requests.update(cx, |view, cx| view.save(window, cx)),
            Page::System => self.system.update(cx, |view, cx| view.save(window, cx)),
            _ => {}
        }
    }
}
