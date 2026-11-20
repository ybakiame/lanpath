//! 应用级离页/关闭保护。草稿仍由页面持有；这里只保存当前页面的判定器。
use crate::Page;
use nsh_core::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeaveState {
    Clean,
    Dirty,
    Busy,
}
type DraftCheck = Rc<RefCell<Option<Box<dyn Fn() -> LeaveState>>>>;

#[derive(Clone, Default)]
struct Guard {
    generation: Rc<Cell<u64>>,
    check: DraftCheck,
}
/// 页面注册自己的草稿/事务判定；随页面 owner 自动释放，不保存敏感草稿副本。
pub(crate) fn protect(check: impl Fn() -> LeaveState + 'static) {
    let guard = global::<Guard>().get();
    let id = guard.generation.get().wrapping_add(1);
    guard.generation.set(id);
    *guard.check.borrow_mut() = Some(Box::new(check));
    cleanup(move || {
        if guard.generation.get() == id {
            *guard.check.borrow_mut() = None;
        }
    });
}
fn state() -> LeaveState {
    let guard = global::<Guard>().get();
    guard
        .check
        .borrow()
        .as_ref()
        .map(|check| check())
        .unwrap_or(LeaveState::Clean)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Destination {
    Page(Page),
    Panel,
    #[cfg(target_os = "windows")]
    Close,
}
#[derive(Clone, Copy)]
struct Pending {
    target: Signal<Option<Destination>>,
    blocked: Signal<bool>,
}
impl Default for Pending {
    fn default() -> Self {
        Self {
            target: signal(None),
            blocked: signal(false),
        }
    }
}
/// Sheet 的所有关闭入口共用检查；等待确认时保留原面板和编辑器。
pub(crate) fn request_panel_close() {
    let pending = global::<Pending>().get();
    let leave = untracked(state);
    if leave == LeaveState::Clean {
        crate::workspace_panels::close();
    } else {
        pending.blocked.set(leave == LeaveState::Busy);
        pending.target.set(Some(Destination::Panel));
    }
}
fn activate(target: Page, active: Signal<Page>, requested: Signal<Page>) {
    if crate::workspace_panels::is_panel(target) {
        active.set(Page::Overview);
        requested.set(Page::Overview);
        crate::workspace_panels::open(target);
    } else {
        crate::workspace_panels::close();
        active.set(target);
        requested.set(target);
    }
}

/// 返回真实当前页面与受保护的导航请求信号。业务页只发请求，不能销毁有草稿的旧页面。
pub(crate) fn install() -> (Signal<Page>, Signal<Page>, AnyElement) {
    let active = signal(Page::Overview);
    let requested = signal(Page::Overview);
    let controls = global::<Pending>().get();
    let pending = controls.target;
    let blocked = controls.blocked;
    effect(move || {
        let target = requested.get();
        if target == active.get_untracked() {
            return;
        }
        let leave = untracked(state);
        if leave == LeaveState::Clean {
            activate(target, active, requested);
        } else {
            blocked.set(leave == LeaveState::Busy);
            pending.set(Some(Destination::Page(target)));
            requested.set(active.get_untracked());
        }
    });
    #[cfg(target_os = "windows")]
    {
        use nsh_core::{
            desktop::WindowAttributes,
            element::desktop::{main_window, on_quit_request, quit},
        };
        let mut attributes = WindowAttributes::default();
        attributes.close_on_press = Some(false);
        main_window().set(attributes);
        let request_close = move || {
            let app = crate::app_state::current();
            let leave = if app.command.get().busy || app.status.get().operation.is_some() {
                LeaveState::Busy
            } else {
                state()
            };
            if leave == LeaveState::Clean {
                quit();
            } else {
                blocked.set(leave == LeaveState::Busy);
                pending.set(Some(Destination::Close));
            }
        };
        main_window().on_close_press(move |_| request_close());
        on_quit_request(request_close);
    }
    let dialog = crate::ui::confirm_dialog(move || pending.get().is_some())
        .test_id("navigation-leave-dialog")
        .title(move || {
            if blocked.get() {
                t!("navigation.busy_title")
            } else {
                t!("navigation.dirty_title")
            }
        })
        .description(move || {
            if blocked.get() {
                t!("navigation.busy_description")
            } else {
                t!("navigation.dirty_description")
            }
        })
        .confirm_label(move || {
            if blocked.get() {
                t!("common.close")
            } else {
                t!("navigation.discard")
            }
        })
        .cancel_label(|| t!("navigation.keep"))
        .on_cancel(move || pending.set(None))
        .on_confirm(move || {
            if blocked.get() {
                pending.set(None);
                return;
            }
            let destination = pending.get();
            pending.set(None);
            match destination {
                Some(Destination::Page(target)) => {
                    activate(target, active, requested);
                }
                Some(Destination::Panel) => crate::workspace_panels::close(),
                #[cfg(target_os = "windows")]
                Some(Destination::Close) => {
                    #[cfg(target_os = "windows")]
                    nsh_core::element::desktop::quit();
                }
                None => {}
            }
        })
        .into_element();
    (active, requested, dialog)
}
