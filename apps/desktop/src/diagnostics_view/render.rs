use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::{Button, ButtonVariants as _},
    input::Input,
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_client::diagnose::DiagStatus;

use super::DiagnosticsView;

impl Render for DiagnosticsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = if let Some(target) = &self.pending_target {
            format!(
                "正在诊断 #{0}：{target}。等待真实探测完成；逐步结果完成后统一显示。",
                self.sequence
            )
        } else if let Some(error) = &self.error {
            error.clone()
        } else if self.closing {
            "正在关闭，不再接受新诊断".into()
        } else if self.locked {
            "核心或配置操作进行中；完成后可运行诊断".into()
        } else {
            "输入目标域名后运行；修改输入不会改变下方已完成报告的目标。".into()
        };
        let mut report_content = div()
            .flex()
            .flex_col()
            .min_w_0()
            .w_full()
            .gap_4()
            .p_1()
            .max_w(gpui_kit::rems(64.0));
        if let Some(completed) = &self.completed {
            let report = &completed.report;
            let heading = format!(
                "已完成报告 #{} · 目标 {} · {} 个步骤，{} 个失败",
                completed.sequence,
                report.domain,
                report.steps.len(),
                report.failed_steps
            );
            report_content = report_content.child(
                div()
                    .id("diagnostics-report-summary")
                    .role(Role::Status)
                    .accessibility_id("diagnostics-report-summary")
                    .aria_label(heading.clone())
                    .text_sm()
                    .child(heading),
            );
            for (ix, step) in report.steps.iter().enumerate() {
                let heading = format!(
                    "{}. {} · {} · {} ms",
                    ix + 1,
                    step.name,
                    status_label(step.status),
                    step.duration_ms
                );
                let step_id = SharedString::from(format!("diagnostics-step-{}", step.key));
                report_content = report_content.child(
                    div()
                        .id(step_id.clone())
                        .accessibility_id(step_id)
                        .role(Role::Group)
                        .aria_label(format!("{heading}\n{}\n{}", step.summary, step.detail))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .min_w_0()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .pt_3()
                        .child(
                            div()
                                .text_sm()
                                .text_color(if step.status == DiagStatus::Fail {
                                    cx.theme().danger
                                } else {
                                    cx.theme().foreground
                                })
                                .child(heading),
                        )
                        .child(div().text_sm().child(step.summary.clone()))
                        .children((!step.detail.is_empty()).then(|| {
                            div()
                                .text_sm()
                                .min_w_0()
                                .whitespace_normal()
                                .text_color(cx.theme().muted_foreground)
                                .child(step.detail.clone())
                        })),
                );
            }
        } else {
            report_content = report_content.child(
                div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("尚未运行诊断。停止核心时仍可运行系统 DNS、直连和 DNS 服务器探测；核心路径将按真实状态失败或跳过。"),
            );
        }
        div().flex().flex_col().flex_1().min_h_0().min_w_0().gap_3().overflow_y_scrollbar()
            .child(div().flex().flex_col().min_w_0().w_full().gap_3().p_1().flex_shrink_0()
                .max_w(gpui_kit::rems(64.0))
                .child(div().text_lg().font_semibold().child("连通性诊断"))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("检查显式请求路径，不设置系统代理、不启用 TUN，也不能证明系统隧道正常。诊断使用接受运行时已保存的实验配置；不会显示完整配置或 API 密钥。"))
                .child(div().flex().flex_col().gap_2()
                    .child("目标域名")
                    .child(Input::new(&self.domain).id("diagnostics-domain")
                        .accessibility_id("diagnostics-domain")
                        .aria_label("诊断目标域名（留空使用 google.com）")
                        .disabled(self.editing_disabled()))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("只填域名；留空使用 google.com。Enter 与“运行诊断”执行同一操作。")))
                .child(div().flex().items_center().flex_wrap().gap_2()
                    .child(Button::new("diagnostics-run").primary()
                        .accessibility_id("diagnostics-run").label("运行诊断")
                        .disabled(self.editing_disabled()).loading(self.busy())
                        .on_click(cx.listener(|this, _, _, cx| this.run(cx)))))
                .child(div().id("diagnostics-status").role(Role::Status)
                    .accessibility_id("diagnostics-status")
                    .aria_label(status.clone()).text_sm()
                    .text_color(if self.error.is_some() { cx.theme().danger } else { cx.theme().muted_foreground })
                    .child(status))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("探测逐步执行，系统 DNS、直连和核心 HTTPS 每步最多约 8 秒，各 DNS 服务器分别等待；服务器较多时需要更久。关闭窗口会等待已接受的诊断完成。")))
            .child(div().id("diagnostics-report").role(Role::Group)
                .accessibility_id("diagnostics-report")
                .aria_label("连通性诊断报告")
                .flex_shrink_0().min_w_0()
                .child(report_content))
    }
}

fn status_label(status: DiagStatus) -> &'static str {
    match status {
        DiagStatus::Ok => "通过",
        DiagStatus::Fail => "失败",
        DiagStatus::Skip => "跳过",
        DiagStatus::Info => "说明",
    }
}
