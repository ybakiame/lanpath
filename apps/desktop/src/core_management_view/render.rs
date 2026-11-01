use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    scroll::ScrollableElement as _,
    tag::Tag,
};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_client::core_management::ManagedCore;

use crate::startup::StartupStageStatus;

use super::CoreManagementView;

impl CoreManagementView {
    fn installed_card(
        &self,
        core: &ManagedCore,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let version = core.version();
        let select_path = core.path().to_path_buf();
        let delete_path = select_path.clone();
        let active = core.is_active();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .border_1()
            .rounded_lg()
            .border_color(if active {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .bg(cx.theme().group_box)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(div().font_semibold().child(core_tag(version)))
                    .child(Tag::secondary().small().child(channel_label(version)))
                    .children(active.then(|| Tag::primary().small().child("当前使用"))),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(core.path().display().to_string()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new(format!("core-select-{version}"))
                            .small()
                            .icon(IconName::CircleCheck)
                            .label("使用")
                            .disabled(disabled || active)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select(select_path.clone(), cx)
                            })),
                    )
                    .child(
                        Button::new(format!("core-delete-{version}"))
                            .small()
                            .danger()
                            .outline()
                            .icon(IconName::Trash)
                            .label("删除")
                            .disabled(disabled || active)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.confirm_delete(delete_path.clone(), window, cx)
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl Render for CoreManagementView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.actions_disabled();
        let selection_disabled = self.selection_disabled();
        let startup_summary = gpui_kit::SharedString::from(self.startup.summary());
        let status = self
            .error
            .clone()
            .or_else(|| self.stage.clone())
            .unwrap_or_else(|| "每个通道仅保留当前版本和一个上一版本备份".into());
        let mut installed = self.local.clone();
        installed.sort_by(|a, b| {
            let a = semver::Version::parse(a.version())
                .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
            let b = semver::Version::parse(b.version())
                .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
            b.cmp(&a)
        });
        let active = installed.iter().find(|core| core.is_active()).cloned();
        let actions = div()
            .flex()
            .items_center()
            .flex_wrap()
            .flex_shrink_0()
            .gap_3()
            .child(div().text_lg().font_semibold().child("核心"))
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("按通道管理，无需选择具体版本"),
            )
            .child(
                Button::new("core-refresh")
                    .accessibility_id("core-refresh")
                    .small()
                    .icon(IconName::RefreshCw)
                    .label("刷新本地核心")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            )
            .child(
                Button::new("core-check-remote")
                    .accessibility_id("core-check-remote")
                    .small()
                    .icon(IconName::CloudDownload)
                    .label("检查远端通道")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.check_remote(cx))),
            );
        #[cfg(windows)]
        let actions = actions.child(
            Button::new("core-install-bundled")
                .accessibility_id("core-install-bundled")
                .small()
                .icon(IconName::Package)
                .label("安装并选中内置核心")
                .disabled(selection_disabled)
                .on_click(cx.listener(|this, _, _, cx| this.install_bundled(cx))),
        );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_4()
            .child(actions)
            .child(div().flex().items_center().flex_wrap().gap_2().children(
                self.core.core_override().map(|path| {
                    div()
                        .id("core-test-override")
                        .accessibility_id("core-test-override")
                        .role(Role::Status)
                        .text_sm()
                        .text_color(cx.theme().warning)
                        .min_w_0()
                        .whitespace_normal()
                        .child(format!(
                            "测试覆盖正在生效：{}。覆盖期间不可安装、下载、选中或删除核心。",
                            path.display()
                        ))
                }),
            ))
            .child(
                div()
                    .id("core-management-status")
                    .accessibility_id("core-management-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
                    .flex_shrink_0()
                    .text_color(if self.error.is_some() {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .min_w_0()
                    .max_h_16()
                    .overflow_y_scrollbar()
                    .child(status),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .gap_4()
                    .overflow_y_scrollbar()
                    .p_1()
                    .children(active.map(|core| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().font_semibold().child("当前使用"))
                            .child(self.installed_card(&core, selection_disabled, cx))
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(div().font_semibold().child("本地备份"))
                            .children((installed.is_empty() && self.error.is_none()).then(|| {
                                div()
                                    .id("core-inventory-empty")
                                    .accessibility_id("core-inventory-empty")
                                    .role(Role::Status)
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("尚无已安装核心。可安装内置核心或检查远端通道后下载。")
                            }))
                            .children(
                                installed
                                    .iter()
                                    .filter(|core| !core.is_active())
                                    .map(|core| self.installed_card(core, selection_disabled, cx)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(div().font_semibold().child("远端通道"))
                            .children(
                                [("stable", "稳定版"), ("prerelease", "预发布版")]
                                    .into_iter()
                                    .map(|(channel, label)| {
                                        let remote = self
                                            .remote
                                            .iter()
                                            .find(|remote| remote.channel() == channel);
                                        let mut card = div()
                                            .flex()
                                            .flex_col()
                                            .gap_3()
                                            .p_4()
                                            .border_1()
                                            .rounded_lg()
                                            .border_color(cx.theme().border)
                                            .bg(cx.theme().group_box)
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(div().font_semibold().child(label))
                                                    .child(Tag::info().small().child(channel)),
                                            );
                                        if let Some(remote) = remote {
                                            let version = remote.version().to_string();
                                            let installed = self
                                                .local
                                                .iter()
                                                .any(|core| core.version() == version);
                                            card = card
                                                .child(div().text_sm().child(format!(
                                                    "最新构建：{}",
                                                    core_tag(&version)
                                                )))
                                                .child(
                                                    Button::new(format!(
                                                        "core-download-{channel}-{version}"
                                                    ))
                                                    .small()
                                                    .primary()
                                                    .icon(IconName::Download)
                                                    .label(if installed {
                                                        "重新校验并选中"
                                                    } else {
                                                        "下载并选中"
                                                    })
                                                    .disabled(selection_disabled)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.download(version.clone(), cx)
                                                    })),
                                                );
                                        } else {
                                            card = card.child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(if self.remote_checked {
                                                        "未返回版本"
                                                    } else {
                                                        "尚未检查远端通道"
                                                    }),
                                            );
                                        }
                                        card
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_4()
                            .border_1()
                            .rounded_lg()
                            .border_color(cx.theme().border)
                            .child(div().font_semibold().child("最近启动记录"))
                            .child(
                                div()
                                    .id("core-startup-summary")
                                    .accessibility_id("core-startup-summary")
                                    .role(Role::Status)
                                    .aria_label(startup_summary.clone())
                                    .text_sm()
                                    .child(startup_summary),
                            )
                            .children(self.startup.steps().iter().map(|step| {
                                let status = match step.status() {
                                    StartupStageStatus::Running => "进行中",
                                    StartupStageStatus::Complete => "完成",
                                    StartupStageStatus::Failed => "失败",
                                    StartupStageStatus::Warning => "警告",
                                };
                                let id = format!("core-startup-step-{}", step.id());
                                let summary = format!(
                                    "{} · {status} · {} ms",
                                    step.label(),
                                    step.duration_ms()
                                );
                                let accessible = step.detail().map_or_else(
                                    || summary.clone(),
                                    |detail| format!("{summary} · {detail}"),
                                );
                                div()
                                    .id(gpui_kit::SharedString::from(id.clone()))
                                    .accessibility_id(id)
                                    .role(Role::Status)
                                    .aria_label(accessible)
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .text_sm()
                                    .min_w_0()
                                    .whitespace_normal()
                                    .child(summary)
                                    .children(step.detail().map(|detail| {
                                        div()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(detail.to_string())
                                    }))
                            })),
                    ),
            )
    }
}

fn core_tag(version: &str) -> String {
    format!("v{}", version.trim_start_matches('v'))
}

fn channel_label(version: &str) -> &'static str {
    let version = version.trim_start_matches('v').to_ascii_lowercase();
    let Some((_, marker)) = version.split_once('-') else {
        return "稳定版";
    };
    if marker.starts_with("beta") || marker.starts_with("rc") {
        "测试版"
    } else {
        "预发布版"
    }
}
