use gpui_kit::component::{ActiveTheme as _, Disableable as _, button::Button};
use gpui_kit::{Context, ParentElement as _, Styled as _, div};

use super::{Operation, ScriptsView, render::section_title};

impl ScriptsView {
    pub(super) fn render_tasks(&self, cx: &Context<Self>) -> impl gpui_kit::IntoElement {
        let task_busy = matches!(
            self.operation,
            Some(Operation::LoadTasks | Operation::RunTask)
        );
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_2()
            .child(section_title(
                "定时任务",
                "任务来自最近一次片段更新；调度器随实验核心启动和停止，手动运行会记录输出与时间。",
                cx,
            ))
            .child(
                Button::new("tasks-refresh")
                    .label(if matches!(self.operation, Some(Operation::LoadTasks)) {
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
                        .child("当前没有可运行任务。先停止核心，在远程资源中更新含 cron 的片段。")
                }),
            )
            .children(self.tasks.iter().map(|task| {
                let name = task.name.clone();
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
                                Button::new(format!("task-run-{}", task.name))
                                    .label(if matches!(self.operation, Some(Operation::RunTask)) {
                                        "运行中…"
                                    } else {
                                        "运行"
                                    })
                                    .disabled(!self.running || task_busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.run_task(name.clone(), cx)
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
