use std::{ops::Range, rc::Rc};

use gpui_kit::component::{
    ActiveTheme as _, Collapsible, ThemeStyled as _, WindowExt as _,
    list::{ListDelegate, ListState},
    sidebar::{SidebarGroup, SidebarItem, SidebarMenuItem},
};
use gpui_kit::{
    App, Context, ElementId, FocusHandle, Focusable as _, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Styled as _, WeakEntity, Window, canvas, div,
    prelude::FluentBuilder as _,
};

use crate::{DesktopView, Page};

gpui_kit::actions!(desktop, [CycleRegion, SaveCurrentForm]);
gpui_kit::actions!(desktop_navigation, [Next, Previous, First, Last, Activate]);

pub(crate) const PAGES: [Page; 19] = [
    Page::Subscriptions,
    Page::Profiles,
    Page::Structured,
    Page::Rules,
    Page::Configuration,
    Page::Proxies,
    Page::Connections,
    Page::Stats,
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
        KeyBinding::new("shift-f6", CycleRegion, Some("Desktop")),
        KeyBinding::new("ctrl-s", SaveCurrentForm, Some("Desktop")),
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
}

impl NavigationMenu {
    pub(crate) fn new(view: &DesktopView, cx: &Context<DesktopView>) -> Self {
        Self {
            view: cx.weak_entity(),
            handles: view.navigation_focus.clone(),
            active: view.page,
            disabled: view.closing,
        }
    }

    fn group(&self, title: &'static str, pages: Range<usize>) -> SidebarGroup<NavigationItems> {
        SidebarGroup::new(title).child(NavigationItems {
            menu: self.clone(),
            pages,
        })
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
        // Keep the small, fixed destination tree mounted together. Sidebar otherwise
        // virtualizes whole groups, removing offscreen keyboard destinations.
        div()
            .id(id)
            .flex()
            .flex_col()
            .gap_3()
            .child(
                self.group("配置", 0..5)
                    .render("configuration-navigation", window, cx),
            )
            .child(
                self.group("运行", 5..10)
                    .render("runtime-navigation", window, cx),
            )
            .child(
                self.group("扩展", 10..12)
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
        div()
            .id(id)
            .flex()
            .flex_col()
            .gap_1()
            .children(self.pages.map(|index| {
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
            }))
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
    pub(crate) fn focus_navigation(&self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if !self.closing {
            let Some(index) = PAGES.iter().position(|&candidate| candidate == page) else {
                return;
            };
            self.navigation_focus[index].focus(window, cx);
            cx.notify();
        }
    }

    fn move_navigation(
        &self,
        page: Page,
        offset: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = PAGES.iter().position(|&candidate| candidate == page) else {
            return;
        };
        let next = (index as isize + offset).rem_euclid(PAGES.len() as isize) as usize;
        self.focus_navigation(PAGES[next], window, cx);
    }

    pub(crate) fn cycle_region(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || window.has_active_dialog(cx) || window.has_active_sheet(cx) {
            return;
        }
        if self
            .navigation_focus
            .iter()
            .any(|handle| handle.is_focused(window))
        {
            self.content_focus.focus(window, cx);
            cx.notify();
        } else {
            self.focus_navigation(self.page, window, cx);
        }
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
            Page::Structured => self.structured.update(cx, |view, cx| view.save(window, cx)),
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
