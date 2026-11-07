use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    spinner::Spinner,
};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled as _, Task, Window,
    div,
};

use crate::{DesktopView, appearance, runtime::DesktopRuntime};

pub(crate) struct SessionView {
    state: SessionState,
    _migration_task: Option<Task<()>>,
}

enum SessionState {
    AwaitingRestore(Option<DesktopRuntime>),
    Migrating {
        restore: bool,
    },
    Ready(Entity<DesktopView>),
    Failed {
        core: Option<DesktopRuntime>,
        error: String,
    },
}

impl SessionView {
    pub(crate) fn new(core: DesktopRuntime, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view = if core.backup_restore_pending() {
            Self {
                state: SessionState::AwaitingRestore(Some(core)),
                _migration_task: None,
            }
        } else {
            match core.settings_migration_required() {
                Ok(false) => Self {
                    state: Self::ready(core, window, cx, true),
                    _migration_task: None,
                },
                Err(error) => Self {
                    state: SessionState::Failed {
                        core: Some(core),
                        error,
                    },
                    _migration_task: None,
                },
                Ok(true) => {
                    let mut view = Self {
                        state: SessionState::Migrating { restore: false },
                        _migration_task: None,
                    };
                    view.start(core, false, window, cx);
                    view
                }
            }
        };
        if !matches!(view.state, SessionState::Ready(_)) {
            let weak = cx.weak_entity();
            window.on_window_should_close(cx, move |_, cx| {
                weak.update(cx, |view, _| {
                    matches!(view.state, SessionState::Failed { .. })
                })
                .unwrap_or(true)
            });
        }
        view
    }

    fn ready(
        mut core: DesktopRuntime,
        window: &mut Window,
        cx: &mut Context<Self>,
        load_settings: bool,
    ) -> SessionState {
        if load_settings {
            let _ = core.migrate_settings();
            let _ = core.load_settings();
        }
        let core = Arc::new(core);
        appearance::restore_backup(&core, cx);
        SessionState::Ready(cx.new(|cx| DesktopView::new(core, window, cx)))
    }

    fn start(
        &mut self,
        core: DesktopRuntime,
        restore: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state = SessionState::Migrating { restore };
        self._migration_task = Some(cx.spawn_in(window, async move |this, cx| {
            let (core, result) = cx
                .background_spawn(async move {
                    let mut core = core;
                    let result = (|| {
                        if restore {
                            core.validate_backup_restore()?;
                        }
                        core.migrate_settings()?;
                        if restore {
                            core.apply_backup_restore()?;
                        }
                        core.load_settings()
                    })();
                    (core, result)
                })
                .await;
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |this, cx| {
                    match result {
                        Ok(()) => this.state = Self::ready(core, window, cx, false),
                        Err(error) => {
                            this.state = SessionState::Failed {
                                core: Some(core),
                                error,
                            };
                        }
                    }
                    cx.notify();
                });
            });
        }));
        cx.notify();
    }

    fn resume(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let core = match &mut self.state {
            SessionState::AwaitingRestore(core) | SessionState::Failed { core, .. } => core.take(),
            _ => None,
        };
        if let Some(core) = core {
            self.state = Self::ready(core, window, cx, true);
            self._migration_task = None;
            cx.notify();
        }
    }

    fn start_confirmed_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let core = match &mut self.state {
            SessionState::AwaitingRestore(core) => core.take(),
            _ => None,
        };
        if let Some(core) = core {
            self.start(core, true, window, cx);
        }
    }

    fn shell(&self, content: AnyElement, cx: &App) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child(crate::product::window_title()))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .p_8()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .w_full()
                            .max_w(gpui_kit::rems(34.0))
                            .gap_4()
                            .p_8()
                            .bg(cx.theme().popover)
                            .border_1()
                            .border_color(cx.theme().border)
                            .rounded_lg()
                            .child(content),
                    ),
            )
            .into_any_element()
    }

    fn render_steps(restore: bool, muted: Hsla) -> AnyElement {
        let steps: &[&str] = if restore {
            &[
                "1. 校验备份完整性",
                "2. 迁移当前应用设置",
                "3. 原子恢复备份",
            ]
        } else {
            &["1. 备份旧设置", "2. 转换并校验 app.toml"]
        };
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap_2()
            .text_sm()
            .text_color(muted)
            .children(steps.iter().map(|step| div().child(*step)))
            .into_any_element()
    }

    fn render_migration(&self, restore: bool, cx: &App) -> AnyElement {
        let content = div()
            .flex()
            .flex_col()
            .items_center()
            .w_full()
            .gap_4()
            .child(Spinner::new().large().color(cx.theme().primary))
            .child(div().text_xl().font_semibold().child(if restore {
                "正在恢复备份"
            } else {
                "正在升级应用设置"
            }))
            .child(
                div()
                    .id("settings-migration-status")
                    .accessibility_id("settings-migration-status")
                    .role(Role::Status)
                    .aria_label("正在执行迁移，请等待")
                    .text_sm()
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child("后台正在处理，完成后会自动进入应用。迁移期间请勿关闭程序。"),
            )
            .child(Self::render_steps(restore, cx.theme().muted_foreground));
        self.shell(content.into_any_element(), cx)
    }

    fn render_restore_confirmation(&self, cx: &mut Context<Self>) -> AnyElement {
        let content = div()
            .flex()
            .flex_col()
            .items_center()
            .w_full()
            .gap_4()
            .child(
                Icon::new(IconName::ArchiveRestore)
                    .large()
                    .text_color(cx.theme().primary),
            )
            .child(div().text_xl().font_semibold().child("检测到待恢复的备份"))
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "先验证备份，再迁移当前设置并执行原子恢复。验证通过前不会修改当前数据。",
                    ),
            )
            .child(Self::render_steps(true, cx.theme().muted_foreground))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .gap_2()
                    .child(
                        Button::new("migration-confirm-restore")
                            .primary()
                            .label("开始恢复")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_confirmed_restore(window, cx)
                            })),
                    )
                    .child(
                        Button::new("migration-skip-restore")
                            .label("暂不恢复")
                            .on_click(cx.listener(|this, _, window, cx| this.resume(window, cx))),
                    ),
            );
        self.shell(content.into_any_element(), cx)
    }

    fn render_failure(&self, error: &str, cx: &mut Context<Self>) -> AnyElement {
        let content = div()
            .flex()
            .flex_col()
            .items_center()
            .w_full()
            .gap_4()
            .child(
                Icon::new(IconName::CircleX)
                    .large()
                    .text_color(cx.theme().danger),
            )
            .child(div().text_xl().font_semibold().child("迁移或恢复未完成"))
            .child(
                div()
                    .id("settings-migration-status")
                    .accessibility_id("settings-migration-status")
                    .role(Role::Status)
                    .aria_label(error.to_owned())
                    .text_sm()
                    .text_center()
                    .whitespace_normal()
                    .text_color(cx.theme().danger)
                    .child(error.to_owned()),
            )
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child("当前配置和待恢复包均保留。关闭会话后可继续使用应用。"),
            )
            .child(
                Button::new("migration-return-app")
                    .primary()
                    .label("返回应用")
                    .on_click(cx.listener(|this, _, window, cx| this.resume(window, cx))),
            );
        self.shell(content.into_any_element(), cx)
    }
}

impl Render for SessionView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.state {
            SessionState::AwaitingRestore(_) => self.render_restore_confirmation(cx),
            SessionState::Migrating { restore } => self.render_migration(*restore, cx),
            SessionState::Ready(view) => view.clone().into_any_element(),
            SessionState::Failed { error, .. } => self.render_failure(error, cx),
        }
    }
}
