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

impl StartupStep {
    pub(crate) fn id(&self) -> &'static str {
        self.id
    }
    pub(crate) fn label(&self) -> &'static str {
        self.label
    }
    pub(crate) fn status(&self) -> StartupStageStatus {
        self.status
    }
    pub(crate) fn duration_ms(&self) -> u64 {
        self.duration_ms
    }
    pub(crate) fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

#[derive(Clone, Default)]
pub(crate) struct StartupProgress {
    attempt: u64,
    steps: Vec<StartupStep>,
    finished: bool,
}

impl StartupProgress {
    pub(crate) fn steps(&self) -> &[StartupStep] {
        &self.steps
    }
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
        self.sender.send_replace(progress);
        tracing::info!(stage = label, "实验核心启动阶段");
    }

    pub(crate) fn warning(&mut self, detail: String) {
        let mut progress = self.sender.borrow().clone();
        self.complete_current(&mut progress);
        if let Some(step) = progress.steps.last_mut() {
            step.status = StartupStageStatus::Warning;
            step.detail = Some(detail);
        }
        self.sender.send_replace(progress);
    }

    pub(crate) fn finish(&mut self, result: &Result<(), String>) {
        let mut progress = self.sender.borrow().clone();
        self.complete_current(&mut progress);
        if let (Err(error), Some(step)) = (result, progress.steps.last_mut()) {
            step.status = StartupStageStatus::Failed;
            step.detail = Some(error.clone());
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_keeps_real_stage_and_retry_clears_old_attempt() {
        let (sender, receiver) = watch::channel(StartupProgress::default());
        let mut reporter = StartupReporter::new(&sender);
        reporter.stage("selection", "读取选择");
        reporter.stage("validation", "核心校验");
        reporter.finish(&Err("invalid configuration".into()));
        {
            let failed = receiver.borrow();
            assert!(failed.steps[0].status == StartupStageStatus::Complete);
            assert!(failed.steps[1].status == StartupStageStatus::Failed);
            assert_eq!(failed.steps[1].detail(), Some("invalid configuration"));
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
