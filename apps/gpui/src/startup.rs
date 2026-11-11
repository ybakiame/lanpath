//! Actual lifecycle boundaries; no timers manufacture stages or percentages.
use std::time::Instant;

use tokio::sync::watch;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupStageStatus {
    Running,
    Complete,
    Failed,
    Warning,
}

#[derive(Clone)]
pub(crate) struct StartupStep {
    id: &'static str,
    label: &'static str,
    status: StartupStageStatus,
    duration_ms: u64,
    detail: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct StartupProgress {
    attempt: u64,
    steps: Vec<StartupStep>,
    finished: bool,
}

impl StartupProgress {
    pub(crate) fn summary(&self) -> String {
        match self.steps.last() {
            None => "尚无启动记录".into(),
            Some(step) if step.status == StartupStageStatus::Failed => {
                format!("第 {} 次启动失败 · {}", self.attempt, step.label)
            }
            Some(_) if self.finished => format!("第 {} 次启动已完成", self.attempt),
            Some(step) => format!("第 {} 次启动 · {}", self.attempt, step.label),
        }
    }
}

pub(crate) struct StartupReporter<'a> {
    sender: &'a watch::Sender<StartupProgress>,
    started: Instant,
}

impl<'a> StartupReporter<'a> {
    pub(crate) fn new(sender: &'a watch::Sender<StartupProgress>) -> Self {
        let attempt = sender.borrow().attempt + 1;
        sender.send_replace(StartupProgress {
            attempt,
            ..Default::default()
        });
        Self {
            sender,
            started: Instant::now(),
        }
    }

    pub(crate) fn stage(&mut self, id: &'static str, label: &'static str) {
        let mut progress = self.sender.borrow().clone();
        self.complete_current(&mut progress);
        progress.steps.push(StartupStep {
            id,
            label,
            status: StartupStageStatus::Running,
            duration_ms: 0,
            detail: None,
        });
        self.started = Instant::now();
        tracing::info!(
            attempt = progress.attempt,
            stage = id,
            label,
            "核心启动阶段开始"
        );
        self.sender.send_replace(progress);
    }

    pub(crate) fn warning(&mut self, detail: String) {
        let mut progress = self.sender.borrow().clone();
        if let Some(step) = progress.steps.last_mut() {
            step.duration_ms = self.started.elapsed().as_millis() as u64;
            step.status = StartupStageStatus::Warning;
            tracing::warn!(attempt = progress.attempt, stage = step.id, label = step.label,
                duration_ms = step.duration_ms, detail = %detail, "核心启动阶段警告");
            step.detail = Some(detail);
        }
        self.sender.send_replace(progress);
    }

    pub(crate) fn finish(&mut self, result: &Result<(), String>) {
        let mut progress = self.sender.borrow().clone();
        if let Err(error) = result {
            if let Some(step) = progress.steps.last_mut() {
                step.duration_ms = self.started.elapsed().as_millis() as u64;
                step.status = StartupStageStatus::Failed;
                step.detail = Some(error.clone());
                tracing::error!(attempt = progress.attempt, stage = step.id, label = step.label,
                    duration_ms = step.duration_ms, error = %error, "核心启动失败");
            } else {
                tracing::error!(attempt = progress.attempt, error = %error, "核心启动失败");
            }
        } else {
            self.complete_current(&mut progress);
            tracing::info!(attempt = progress.attempt, "核心启动完成");
        }
        progress.finished = true;
        self.sender.send_replace(progress);
    }

    fn complete_current(&self, progress: &mut StartupProgress) {
        if let Some(step) = progress.steps.last_mut()
            && step.status == StartupStageStatus::Running
        {
            step.duration_ms = self.started.elapsed().as_millis() as u64;
            step.status = StartupStageStatus::Complete;
            tracing::info!(
                attempt = progress.attempt,
                stage = step.id,
                label = step.label,
                duration_ms = step.duration_ms,
                "核心启动阶段完成"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::prelude::*;

    // Scoped subscribers still share tracing's global callsite cache. Keep the
    // uninstrumented state test from racing registration in the capture test.
    static TRACING_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Clone)]
    struct Captured(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(bytes)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn logs_keep_stage_timing_warnings_failure_and_retry_without_a_core_page() {
        let _guard = TRACING_TEST_LOCK.lock().unwrap();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let captured = bytes.clone();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .without_time()
                .with_ansi(false)
                .with_writer(move || Captured(captured.clone())),
        );
        tracing::subscriber::with_default(subscriber, || {
            let (sender, _) = watch::channel(StartupProgress::default());
            let mut start = StartupReporter::new(&sender);
            start.stage("selection", "读取选择");
            start.stage("validation", "核心校验");
            start.finish(&Err("invalid fixture configuration".into()));
            let mut retry = StartupReporter::new(&sender);
            retry.stage("optional", "可选资源");
            retry.warning("optional fixture unavailable".into());
            retry.stage("process", "启动进程");
            retry.finish(&Ok(()));
        });
        // Formatter versions may quote simple strings differently.
        let logs = String::from_utf8(bytes.lock().unwrap().clone())
            .unwrap()
            .replace('"', "");
        assert!(
            logs.lines().any(|line| line.contains("核心启动阶段完成")
                && line.contains("stage=selection")
                && line.contains("duration_ms=")),
            "{logs}"
        );
        assert!(logs.lines().any(|line| line.contains("核心启动失败")
            && line.contains("stage=validation")
            && line.contains("invalid fixture configuration")));
        assert!(
            !logs
                .lines()
                .any(|line| line.contains("核心启动阶段完成") && line.contains("stage=validation"))
        );
        assert!(logs.contains("核心启动阶段警告") && logs.contains("optional fixture unavailable"));
        assert!(
            logs.lines()
                .any(|line| line.contains("核心启动完成") && line.contains("attempt=2"))
        );
    }

    #[test]
    fn failure_keeps_real_stage_and_retry_clears_old_attempt() {
        let _guard = TRACING_TEST_LOCK.lock().unwrap();
        let (sender, receiver) = watch::channel(StartupProgress::default());
        let mut reporter = StartupReporter::new(&sender);
        reporter.stage("selection", "读取选择");
        reporter.stage("validation", "核心校验");
        reporter.finish(&Err("invalid configuration".into()));
        {
            let failed = receiver.borrow();
            assert!(failed.steps[0].status == StartupStageStatus::Complete);
            assert!(failed.steps[1].status == StartupStageStatus::Failed);
            assert_eq!(
                failed.steps[1].detail.as_deref(),
                Some("invalid configuration")
            );
        }
        let mut retry = StartupReporter::new(&sender);
        retry.stage("ports", "端口预检");
        retry.warning("optional scheduler unavailable".into());
        retry.stage("process", "启动进程");
        retry.finish(&Ok(()));
        let completed = receiver.borrow();
        assert_eq!(completed.attempt, 2);
        assert!(completed.finished);
        assert_eq!(completed.steps.len(), 2);
        assert!(completed.steps[0].status == StartupStageStatus::Warning);
        assert!(completed.steps[1].status == StartupStageStatus::Complete);
    }
}
