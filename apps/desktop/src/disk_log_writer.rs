//! 磁盘日志写入器：从 apps/gpui 的 `disk_log_writer` 迁移，去掉核心日志通道
//! （NSH 桌面不启动核心，只落本应用自身的 app.log）。保留非阻塞写入、
//! 日轮转、队列上限、真实错误上报与 WorkerGuard 退出冲刷语义。

use std::{
    io::{self, Write},
    path::Path,
    sync::{Arc, Mutex},
};

use tracing_appender::{
    non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard},
    rolling::{Builder, Rotation},
};

/// 本应用磁盘日志（`logs/app.log.YYYY-MM-DD`，日轮转）。克隆得到的
/// `NonBlocking` 是生产者句柄；`WorkerGuard` 由 `LogService` 持有，
/// 退出/丢弃时冲刷并停止后台线程。
#[derive(Clone)]
pub(crate) struct DiskLogWriter {
    app: NonBlocking,
    error: Arc<Mutex<Option<String>>>,
}

impl DiskLogWriter {
    pub(crate) fn new(root: &Path) -> Result<(Self, WorkerGuard), String> {
        let dir = root.join("logs");
        let error = Arc::new(Mutex::new(None));
        let app = Builder::new()
            .rotation(Rotation::DAILY)
            .filename_prefix("app.log")
            .build(&dir)
            .map_err(|error| format!("无法创建应用日志：{error}"))?;
        let (app, guard) = NonBlockingBuilder::default()
            .buffered_lines_limit(1024)
            .finish(ReportingWriter {
                inner: app,
                error: error.clone(),
            });
        Ok((Self { app, error }, guard))
    }

    pub(crate) fn record(&self, level: &str, target: &str, message: &str) {
        // 入队一整条记录，而不是可能交错的多个格式片段。
        let line = format!(
            "{} {level} {target}: {message}\n",
            chrono::Local::now().to_rfc3339()
        );
        if let Err(error) = self.app.clone().write_all(line.as_bytes()) {
            *self.error.lock().unwrap_or_else(|error| error.into_inner()) = Some(error.to_string());
        }
    }

    /// 写入层的真实错误：I/O 失败优先，否则报告队列丢弃的条目数。
    pub(crate) fn error(&self) -> Option<String> {
        let error = self
            .error
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let dropped = self.app.error_counter().dropped_lines();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn records_reach_the_daily_file_and_flush_on_guard_drop() {
        let temporary = tempfile::tempdir().unwrap();
        let (writer, guard) = DiskLogWriter::new(temporary.path()).unwrap();

        writer.record("INFO", "lanpath::test", "磁盘写入验证消息");
        writer.record("ERROR", "lanpath::test", "磁盘错误验证消息");
        assert!(writer.error().is_none());
        // WorkerGuard 落盘并停止后台线程；生产路径由 LogService 持有并在退出时丢弃。
        drop(guard);

        let mut files = fs::read_dir(temporary.path().join("logs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1);
        assert!(files.remove(0).starts_with("app.log"));
        let content = fs::read_to_string(
            fs::read_dir(temporary.path().join("logs"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .unwrap();
        assert!(content.contains("INFO lanpath::test: 磁盘写入验证消息"));
        assert!(content.contains("ERROR lanpath::test: 磁盘错误验证消息"));
        assert!(content.lines().count() == 2);
    }

    #[test]
    fn unwritable_logs_dir_is_a_real_error_not_a_panic() {
        let temporary = tempfile::tempdir().unwrap();
        // 一个普通文件占住 logs 目录名：构建必须以 Err 返回真实错误。
        fs::write(temporary.path().join("logs"), b"not a directory").unwrap();
        let result = DiskLogWriter::new(temporary.path());
        assert!(matches!(result, Err(message) if message.contains("无法创建应用日志")));
    }
}
