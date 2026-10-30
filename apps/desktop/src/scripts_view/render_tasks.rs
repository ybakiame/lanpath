use gpui_kit::component::{ActiveTheme as _, Disableable as _, button::Button};
use gpui_kit::{Context, ParentElement as _, Styled as _, div};

use super::{Operation, ScriptsView, render::section_title};

impl ScriptsView {
    pub(super) fn render_tasks(&self, cx: &Context<Self>) -> impl gpui_kit::IntoElement {
        let task_busy = self.task_status_busy || self.busy();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_2()
            .child(section_title(
                "定时任务",
                "任务来自本次核心启动时的缓存；运行中更新仅供下次启动使用。手动运行记录输出与时间。",
                cx,
            ))
            .child(
                Button::new("tasks-refresh")
                    .label(if self.task_status_busy {
                        "刷新中…"
                    } else {
                        "刷新任务状态"
                    })
                    .disabled(!self.running || task_busy)
                    .on_click(cx.listener(|this, _, _, cx| this.load_tasks(cx))),
            )
            .children((!self.running).then(|| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("核心已停止。启动实验核心后，已缓存且启用的 cron 任务会在此列出。")
            }))
            .children(
                (self.running && self.tasks.is_empty() && !task_busy).then(|| {
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("本次启动没有可运行任务。更新含 cron 的片段缓存后，下次启动核心时加载。")
                }),
            )
            .children(self.tasks.iter().map(|task| {
                let id = task.id.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(cx.theme().radius)
                    .p_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().flex_1().min_w_0().child(task.name.clone()))
                            .child(
                                Button::new(format!("task-run-{}", task.id))
                                    .label(if matches!(self.operation, Some(Operation::RunTask)) {
                                        "运行中…"
                                    } else {
                                        "运行"
                                    })
                                    .disabled(!self.running || task_busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.run_task(id.clone(), cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} · cron {} · 下次 {} · 上次 {}",
                                task.dialect,
                                task.cron_expr,
                                format_time(task.next_run.as_ref()),
                                format_time(task.last_run.as_ref())
                            )),
                    )
                    .children(task.last_error.as_ref().map(|error| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().danger)
                            .child(format!("上次错误：{error}"))
                    }))
            }))
            .children(self.task_output.as_ref().map(|(name, output)| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(format!("任务“{name}”输出"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if output.is_empty() {
                                "$done()".to_string()
                            } else {
                                output.clone()
                            }),
                    )
            }))
    }
}

fn format_time(value: Option<&chrono::DateTime<chrono::Utc>>) -> String {
    value
        .map(chrono::DateTime::to_rfc3339)
        .unwrap_or_else(|| "-".into())
}
