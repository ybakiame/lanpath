use gpui_kit::assets::IconName;
use gpui_kit::base::{Selectable as _, StyledExt as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    scroll::ScrollableElement as _,
    tag::Tag,
};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_client::{
    core_management::ManagedCore,
    cores::{CoreChannel, channel_of_version},
};
use lanpath_common::CoreType;

use crate::startup::StartupStageStatus;

use super::{CoreManagementView, supported_channel};

const CORE_TYPES: [(CoreType, &str); 2] = [
    (CoreType::SingBox, "sing-box"),
    (CoreType::Mihomo, "Mihomo"),
];
const CHANNELS: [(CoreChannel, &str); 2] = [
    (CoreChannel::Stable, "稳定版"),
    (CoreChannel::Prerelease, "预发布版"),
];

impl CoreManagementView {
    fn version_row(
        &self,
        core: &ManagedCore,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let version = core.version().to_string();
        let select_path = core.path().to_path_buf();
        let delete_path = select_path.clone();
        let active = core.is_active();
        let id = format!("{}-{version}", self.selected_core);
        div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_3()
            .py_2()
            .child(div().w_32().font_medium().child(core_tag(&version)))
            .children(active.then(|| Tag::primary().small().child("当前使用")))
            .child(div().flex_1())
            .child(
                Button::new(format!("core-select-{id}"))
                    .small()
                    .icon(IconName::CircleCheck)
                    .label("使用")
                    .disabled(disabled || active)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select(select_path.clone(), cx)),
                    ),
            )
            .child(
                Button::new(format!("core-delete-{id}"))
                    .small()
                    .danger()
                    .outline()
                    .icon(IconName::Trash)
                    .label("删除")
                    .disabled(disabled || active)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_delete(delete_path.clone(), window, cx)
                    })),
            )
            .into_any_element()
    }

    fn startup_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let startup_summary = gpui_kit::SharedString::from(self.startup.summary());
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
                let summary = format!("{} · {status} · {} ms", step.label(), step.duration_ms());
                let accessible = step
                    .detail()
                    .map_or_else(|| summary.clone(), |detail| format!("{summary} · {detail}"));
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
            }))
            .into_any_element()
    }
}

impl Render for CoreManagementView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.actions_disabled();
        let selection_disabled = self.selection_disabled();
        let status = self
            .error
            .clone()
            .or_else(|| self.stage.clone())
            .unwrap_or_else(|| "选择核心类型与发布通道，再管理本地版本或检查更新".into());
        let selected_channel = self.selected_channel;
        let remote = self
            .remote
            .iter()
            .find(|remote| remote.channel() == selected_channel.as_str())
            .cloned();
        let mut installed: Vec<_> = self
            .local
            .iter()
            .filter(|core| {
                supported_channel(channel_of_version(core.version())) == selected_channel
            })
            .cloned()
            .collect();
        installed.sort_by(|a, b| {
            let a = semver::Version::parse(a.version())
                .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
            let b = semver::Version::parse(b.version())
                .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
            b.cmp(&a)
        });
        let local_latest = installed.first().map(|core| core_tag(core.version()));

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_3()
                    .child(div().text_lg().font_semibold().child("核心"))
                    .child(
                        ButtonGroup::new("core-type-switcher")
                            .small()
                            .compact()
                            .outline()
                            .disabled(disabled)
                            .children(CORE_TYPES.iter().map(|(kind, label)| {
                                Button::new(format!("core-type-{}", kind.as_str()))
                                    .accessibility_id(format!("core-type-{}", kind.as_str()))
                                    .label(*label)
                                    .selected(*kind == self.selected_core)
                            }))
                            .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                                if let Some((kind, _)) = indexes
                                    .first()
                                    .and_then(|index| CORE_TYPES.get(*index))
                                {
                                    this.select_core_type(*kind, cx);
                                }
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("core-refresh")
                            .accessibility_id("core-refresh")
                            .small()
                            .icon(IconName::RefreshCw)
                            .label("刷新")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
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
                            "测试覆盖正在生效：{}。覆盖期间不可下载、选中或删除核心。",
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
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().font_semibold().child("当前核心"))
                            .children(self.active_core.as_ref().map(|core| {
                                let mut card = div()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .p_4()
                                    .border_1()
                                    .rounded_lg()
                                    .border_color(cx.theme().primary)
                                    .bg(cx.theme().group_box)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .flex_wrap()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .font_semibold()
                                                    .child(core.core_type().to_string()),
                                            )
                                            .child(Tag::primary().small().child(core_tag(core.version()))),
                                    );
                                if let Some(environment) = core.environment() {
                                    card = card.child(
                                        div()
                                            .text_sm()
                                            .whitespace_normal()
                                            .child(format!("Environment: {environment}")),
                                    );
                                }
                                if !core.tags().is_empty() {
                                    card = card.child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("编译 Tags"),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_wrap()
                                                    .gap_1()
                                                    .children(core.tags().iter().map(|tag| {
                                                        Tag::secondary().small().child(tag.clone())
                                                    })),
                                            ),
                                    );
                                }
                                card.child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(core.path().display().to_string()),
                                )
                            }))
                            .children(self.active_core.is_none().then(|| {
                                div()
                                    .p_4()
                                    .border_1()
                                    .rounded_lg()
                                    .border_color(cx.theme().border)
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("尚未选择生效核心。请从下方本地版本选择，或检查更新后下载。")
                            })),
                    )
                    .child(
                        div()
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
                                    .flex_wrap()
                                    .gap_3()
                                    .child(div().font_semibold().child("版本信息"))
                                    .child(
                                        ButtonGroup::new("core-channel-switcher")
                                            .small()
                                            .compact()
                                            .outline()
                                            .disabled(disabled)
                                            .children(CHANNELS.iter().map(|(channel, label)| {
                                                Button::new(format!(
                                                    "core-channel-{}",
                                                    channel.as_str()
                                                ))
                                                .accessibility_id(format!(
                                                    "core-channel-{}",
                                                    channel.as_str()
                                                ))
                                                .label(*label)
                                                .selected(*channel == selected_channel)
                                            }))
                                            .on_click(cx.listener(
                                                |this, indexes: &Vec<usize>, _, cx| {
                                                    if let Some((channel, _)) = indexes
                                                        .first()
                                                        .and_then(|index| CHANNELS.get(*index))
                                                    {
                                                        this.select_channel(*channel, cx);
                                                    }
                                                },
                                            )),
                                    )
                                    .child(div().flex_1())
                                    .child(
                                        Button::new("core-check-remote")
                                            .accessibility_id("core-check-remote")
                                            .small()
                                            .icon(IconName::CloudDownload)
                                            .label("检查更新")
                                            .disabled(disabled)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.check_remote(cx)
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(Tag::secondary().small().child(format!(
                                        "本地：{}",
                                        local_latest.as_deref().unwrap_or("未安装")
                                    )))
                                    .child(Tag::info().small().child(format!(
                                        "远程：{}",
                                        remote
                                            .as_ref()
                                            .map(|remote| core_tag(remote.version()))
                                            .unwrap_or_else(|| "尚未检查".into())
                                    ))),
                            )
                            .children(remote.map(|remote| {
                                let version = remote.version().to_string();
                                let installed = self
                                    .local
                                    .iter()
                                    .any(|core| core.version() == version);
                                Button::new(format!(
                                    "core-download-{}-{}-{version}",
                                    self.selected_core, selected_channel.as_str()
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
                                }))
                            }))
                            .child(div().border_t_1().border_color(cx.theme().border))
                            .child(div().font_semibold().child("本地版本"))
                            .children((installed.is_empty() && self.error.is_none()).then(|| {
                                div()
                                    .id("core-inventory-empty")
                                    .accessibility_id("core-inventory-empty")
                                    .role(Role::Status)
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("此通道尚无本地版本，可切换通道或检查更新后下载。")
                            }))
                            .children(
                                installed
                                    .iter()
                                    .map(|core| self.version_row(core, selection_disabled, cx)),
                            ),
                    )
                    .child(self.startup_card(cx)),
            )
    }
}

fn core_tag(version: &str) -> String {
    let version = version.trim();
    if version.starts_with("alpha-") {
        version.to_string()
    } else {
        format!("v{}", version.trim_start_matches('v'))
    }
}
