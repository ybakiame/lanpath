use std::{
    io::{self, Write},
    path::Path,
    sync::{Arc, Mutex},
};

use tracing_appender::{
    non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard},
    rolling::{Builder, Rotation},
};

/// Clonable producers; their worker guards are owned separately by the runtime.
pub(crate) struct DiskLogWriter {
    app: NonBlocking,
    core: NonBlocking,
    error: Arc<Mutex<Option<String>>>,
}

impl DiskLogWriter {
    pub(crate) fn new(root: &Path) -> Result<(Self, [WorkerGuard; 2]), String> {
        let dir = root.join("logs");
        let error = Arc::new(Mutex::new(None));
        let app = Builder::new()
            .rotation(Rotation::DAILY)
            .filename_prefix("app.log")
            .build(&dir)
            .map_err(|error| format!("无法创建应用日志：{error}"))?;
        let core = Builder::new()
            .rotation(Rotation::NEVER)
            .filename_prefix("core.log")
            .build(&dir)
            .map_err(|error| format!("无法创建核心日志：{error}"))?;
        let (app, app_guard) = NonBlockingBuilder::default()
            .buffered_lines_limit(1024)
            .finish(ReportingWriter {
                inner: app,
                error: error.clone(),
            });
        let (core, core_guard) = NonBlockingBuilder::default()
            .buffered_lines_limit(1024)
            .finish(ReportingWriter {
                inner: core,
                error: error.clone(),
            });
        Ok((Self { app, core, error }, [app_guard, core_guard]))
    }

    pub(crate) fn app(&self, level: &str, target: &str, message: &str) {
        self.record(self.app.clone(), level, target, message);
    }

    pub(crate) fn core(&self, level: &str, message: &str) {
        self.record(self.core.clone(), level, "sing-box", message);
    }

    fn record(&self, mut writer: NonBlocking, level: &str, target: &str, message: &str) {
        // Enqueue one complete record, not separate format fragments that can interleave.
        let line = format!(
            "{} {level} {target}: {message}\n",
            chrono::Local::now().to_rfc3339()
        );
        if let Err(error) = writer.write_all(line.as_bytes()) {
            *self.error.lock().unwrap_or_else(|error| error.into_inner()) = Some(error.to_string());
        }
    }

    pub(crate) fn error(&self) -> Option<String> {
        let error = self
            .error
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let dropped =
            self.app.error_counter().dropped_lines() + self.core.error_counter().dropped_lines();
        error.or_else(|| (dropped > 0).then(|| format!("磁盘日志队列已丢弃 {dropped} 条记录")))
    }
}

struct ReportingWriter<W> {
    inner: W,
    error: Arc<Mutex<Option<String>>>,
}

impl<W: Write> Write for ReportingWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = self.inner.write(bytes);
        if let Err(error) = &result {
            *self.error.lock().unwrap_or_else(|error| error.into_inner()) = Some(error.to_string());
        }
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        let result = self.inner.flush();
        if let Err(error) = &result {
            *self.error.lock().unwrap_or_else(|error| error.into_inner()) = Some(error.to_string());
        }
        result
    }
}
