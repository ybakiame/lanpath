use gpui_kit::{AppContext as _, Context};

use super::{CompletedReport, DiagnosticsView};

impl DiagnosticsView {
    pub(super) fn run(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let value = self.domain.read(cx).value();
        let target = if value.trim().is_empty() {
            "google.com".to_owned()
        } else {
            value.trim().to_owned()
        };
        self.sequence += 1;
        let sequence = self.sequence;
        self.pending_target = Some(target.clone());
        self.error = None;
        self.sync_readonly(cx);
        let core = self.core.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let attempted_target = target.clone();
            let result = cx
                .background_spawn(async move { core.diagnose(Some(target)) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.pending_target = None;
                this.task = None;
                match result {
                    Ok(report) => {
                        this.completed = Some(CompletedReport { sequence, report });
                        this.error = None;
                    }
                    Err(error) => {
                        this.error = Some(format!(
                            "目标 {attempted_target} 的诊断 #{sequence} 未能完成：{error}。修复后可重新运行；已有报告保留。"
                        ));
                    }
                }
                this.sync_readonly(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
}
