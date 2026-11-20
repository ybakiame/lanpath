//! 概览工作面板的呈现状态；退出动画完成后才释放页面 owner 与草稿。
use crate::{Page, navigation, ui};
use nsh_core::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Subscriptions,
    Scripts,
    Preview,
}
#[derive(Clone, Copy)]
struct Panels {
    open: Signal<Option<Panel>>,
    retained: Signal<Option<Panel>>,
}
impl Default for Panels {
    fn default() -> Self {
        Self {
            open: signal(None),
            retained: signal(None),
        }
    }
}
pub(crate) fn is_panel(target: Page) -> bool {
    matches!(target, Page::Subscription | Page::Profiles | Page::Config)
}
pub(crate) fn open(target: Page) {
    let panel = match target {
        Page::Subscription => Panel::Subscriptions,
        Page::Profiles => Panel::Scripts,
        Page::Config => Panel::Preview,
        _ => return,
    };
    let state = global::<Panels>().get();
    state.retained.set(Some(panel));
    state.open.set(Some(panel));
}
pub(crate) fn is_open() -> bool {
    global::<Panels>().get().open.get().is_some()
}
pub(crate) fn close() {
    global::<Panels>().get().open.set(None);
}
pub(crate) fn host() -> impl IntoElement {
    let state = global::<Panels>().get();
    view()
        .child(when(
            move || state.retained.get() == Some(Panel::Subscriptions),
            move || {
                ui::sheet::sheet(
                    440.0,
                    || t!("nav.subscriptions"),
                    move || state.open.get() == Some(Panel::Subscriptions),
                    navigation::request_panel_close,
                    move || {
                        if state.open.get().is_none() {
                            state.retained.set(None);
                        }
                    },
                    crate::subscriptions_view::subscription_page(),
                )
            },
        ))
        .child(when(
            move || state.retained.get() == Some(Panel::Scripts),
            move || {
                ui::sheet::sheet(
                    880.0,
                    || t!("profile.editor"),
                    move || state.open.get() == Some(Panel::Scripts),
                    navigation::request_panel_close,
                    move || {
                        if state.open.get().is_none() {
                            state.retained.set(None);
                        }
                    },
                    crate::profiles_view::profiles_page(),
                )
            },
        ))
        .child(when(
            move || state.retained.get() == Some(Panel::Preview),
            move || {
                ui::sheet::sheet(
                    880.0,
                    || t!("nav.config"),
                    move || state.open.get() == Some(Panel::Preview),
                    navigation::request_panel_close,
                    move || {
                        if state.open.get().is_none() {
                            state.retained.set(None);
                        }
                    },
                    crate::config_view::config_page(),
                )
            },
        ))
}
