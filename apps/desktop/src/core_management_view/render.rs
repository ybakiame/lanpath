use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, button::Button, scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use crate::startup::StartupStageStatus;

use super::CoreManagementView;

impl Render for CoreManagementView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.actions_disabled();
        let selection_disabled = self.selection_disabled();
        let startup_summary = gpui_kit::SharedString::from(self.startup.summary());
        let empty = "尚无已安装核心。检查远端通道后下载一个版本。";
        let status = self
            .error
            .clone()
            .or_else(|| self.stage.clone())
            .unwrap_or_else(|| {
                if self.closing {
                    "正在关闭，等待已接受的核心操作完成".into()
                } else if self.locked {
                    "核心运行或其他配置操作期间不可更改核心".into()
                } else {
                    "核心操作就绪；下载完成并校验后会自动选中".into()
                }
            });
        div().flex().flex_col().flex_1().min_h_0().min_w_0()
            .overflow_y_scrollbar()
            .child(div().flex().flex_col().gap_4().p_1()
                .child(div().text_lg().child("核心"))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("仅管理实验数据目录内的 sing-box；不会修改系统代理、TUN 或现行应用。"))
                .children(self.core.core_override().map(|path| {
                    div().id("core-test-override").accessibility_id("core-test-override")
                        .role(Role::Status).text_sm().text_color(cx.theme().warning)
                        .child(format!("测试覆盖正在生效：{}。保存的选中项不会用于启动；覆盖期间禁止下载、选中和删除。", path.display()))
                }))
                .child(div().flex().items_center().flex_wrap().gap_2()
                    .child(Button::new("core-refresh").accessibility_id("core-refresh")
                        .label("刷新本地核心").disabled(disabled)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))))
                    .child(Button::new("core-check-remote").accessibility_id("core-check-remote")
                        .label("检查远端通道").disabled(disabled)
                        .on_click(cx.listener(|this, _, _, cx| this.check_remote(cx)))))
                .child(div().id("core-management-status").accessibility_id("core-management-status")
                    .role(Role::Status).aria_label(status.clone()).text_sm()
                    .text_color(if self.error.is_some() { cx.theme().danger } else { cx.theme().muted_foreground })
                    .child(status))
                .child(div().flex().flex_col().gap_3()
                    .child("已安装版本")
                    .children((self.local.is_empty() && self.error.is_none()).then(|| div()
                        .id("core-inventory-empty").accessibility_id("core-inventory-empty")
                        .role(Role::Status).aria_label(empty).text_sm()
                        .text_color(cx.theme().muted_foreground).child(empty)))
                    .children(self.local.iter().map(|core| {
                        let version = core.version();
                        let select_id = format!("core-select-{version}");
                        let delete_id = format!("core-delete-{version}");
                        let select_path = core.path().to_path_buf();
                        let delete_path = select_path.clone();
                        let row_id = format!("core-installed-{version}");
                        let path = gpui_kit::SharedString::from(core.path().display().to_string());
                        let state = if core.is_active() { "当前选中" } else { "未选中" };
                        div().id(gpui_kit::SharedString::from(row_id.clone())).accessibility_id(row_id)
                            .role(Role::Group).aria_label(format!("sing-box {version} · {state} · {path}"))
                            .flex().flex_col().gap_2().border_b_1().border_color(cx.theme().border).pb_3()
                            .child(div().flex().items_center().flex_wrap().gap_2()
                                .child(format!("sing-box {version}"))
                                .children(core.is_active().then(|| div().text_sm().child("当前选中"))))
                            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                                .child(path))
                            .child(div().flex().items_center().flex_wrap().gap_2()
                                .child(Button::new(gpui_kit::SharedString::from(select_id.clone()))
                                    .accessibility_id(select_id).label(format!("选中 {version}"))
                                    .disabled(selection_disabled || core.is_active())
                                    .on_click(cx.listener(move |this, _, _, cx| this.select(select_path.clone(), cx))))
                                .child(Button::new(gpui_kit::SharedString::from(delete_id.clone()))
                                    .accessibility_id(delete_id).label(format!("删除 {version}…"))
                                    .disabled(selection_disabled || core.is_active())
                                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_delete(delete_path.clone(), window, cx)))))
                    })))
                .child(div().flex().flex_col().gap_3()
                    .child("远端通道")
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child(if self.remote_checked {
                            "以下为上次成功检查返回的实际版本；缺少的通道没有可下载版本。"
                        } else {
                            "尚未检查。手动检查 stable / beta / prerelease 后才能下载。"
                        }))
                    .children([("stable", "稳定版"), ("beta", "测试版"), ("prerelease", "预发布版")].into_iter().map(|(channel, label)| {
                        let remote = self.remote.iter().find(|remote| remote.channel() == channel);
                        let mut row = div().id(channel).flex().items_center().flex_wrap().gap_2()
                            .child(format!("{label}（{channel}）"));
                        if let Some(remote) = remote {
                            let version = remote.version().to_string();
                            let id = format!("core-download-{channel}-{version}");
                            let installed = self.local.iter().any(|core| core.version() == version);
                            row = row.child(version.clone())
                                .child(Button::new(gpui_kit::SharedString::from(id.clone()))
                                    .accessibility_id(id).label(format!("下载并选中 {version}"))
                                    .disabled(selection_disabled || installed)
                                    .on_click(cx.listener(move |this, _, _, cx| this.download(version.clone(), cx))))
                                .children(installed.then(|| div().text_sm().child("已安装")));
                        } else {
                            row = row.child(if self.remote_checked { "未返回版本" } else { "尚未检查" });
                        }
                        row
                    })))
                .child(div().flex().flex_col().gap_2()
                    .child("最近启动记录")
                    .child(div().id("core-startup-summary").accessibility_id("core-startup-summary")
                        .role(Role::Status).aria_label(startup_summary.clone()).text_sm().child(startup_summary))
                    .children(self.startup.steps().iter().map(|step| {
                        let status = match step.status() {
                            StartupStageStatus::Running => "进行中",
                            StartupStageStatus::Complete => "完成",
                            StartupStageStatus::Failed => "失败",
                            StartupStageStatus::Warning => "警告",
                        };
                        let id = format!("core-startup-step-{}", step.id());
                        let summary = format!("{} · {status} · {} ms", step.label(), step.duration_ms());
                        let accessible = step.detail().map_or_else(|| summary.clone(), |detail| format!("{summary} · {detail}"));
                        div().id(gpui_kit::SharedString::from(id.clone())).accessibility_id(id)
                            .role(Role::Status).aria_label(accessible).flex().flex_col().gap_1().text_sm()
                            .child(summary)
                            .children(step.detail().map(|detail| div().text_color(cx.theme().muted_foreground).child(detail.to_string())))
                    })) ))
    }
}
