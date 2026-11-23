//! 日志服务：本应用自身的内存 + 磁盘日志能力，装配在 NSH 桌面独立数据根上。
//!
//! 从 apps/gpui 的 `log_stream`/runtime 日志服务迁移并裁剪：NSH 桌面不启动
//! 核心，没有核心日志 WebSocket 流与 core.log；采集来源只有本应用进程的
//! tracing 事件（`lanpath_*` target）。磁盘读取/导出复用共享
//! `lanpath-client::logs::DiskLogService`，只读本应用数据根的 `logs/`。
//!
//! 服务按数据根路径进程内缓存：页面来回切换不重开缓冲，缓冲内容跨页保留；
//! WorkerGuard 随服务丢弃时冲刷并停止写入线程（取消/退出清理）。headless
//! 测试的并行场景各持独立数据根，互不影响。

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use lanpath_client::logs::DiskLogService;
use lanpath_common::{LanpathError, LanpathResult};
use tracing_appender::non_blocking::WorkerGuard;

use crate::data_root::{open_root, resolve_data_location};
use crate::disk_log_writer::DiskLogWriter;
use crate::log_buffer::LogBuffer;
use crate::log_capture::{self, CaptureSink};

#[cfg(test)]
use crate::log_buffer::level_rank;

/// 每数据根一个日志服务：Weak 缓存，全部句柄释放后自动移除。
static SERVICES: OnceLock<Mutex<HashMap<PathBuf, Weak<LogService>>>> = OnceLock::new();

pub(crate) struct LogService {
    root: PathBuf,
    sink: Arc<CaptureSink>,
    /// 磁盘写入器创建失败的持久化错误；内存缓冲仍可用。
    disk_error: Option<String>,
    /// 退出/丢弃时冲刷队列并停止写入线程。
    _guard: Option<WorkerGuard>,
}

impl LogService {
    /// 解析本进程数据根并打开日志服务（应用启动路径）。
    pub(crate) fn open() -> LanpathResult<Arc<Self>> {
        let location = resolve_data_location()?;
        Self::open_service(&location.path)
    }

    /// 指定数据根打开（测试与显式调用路径，同样走完整准入）。
    #[cfg(test)]
    pub(crate) fn for_root(path: &Path) -> LanpathResult<Arc<Self>> {
        Self::open_service(path)
    }

    fn open_service(path: &Path) -> LanpathResult<Arc<Self>> {
        let key = std::path::absolute(path)?;
        let mut services = SERVICES
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(service) = services.get(&key).and_then(Weak::upgrade) {
            return Ok(service);
        }
        let root = open_root(&key)?;
        let (disk, disk_error, guard) = match DiskLogWriter::new(root.path()) {
            Ok((writer, guard)) => (Some(Arc::new(writer)), None, Some(guard)),
            Err(error) => (None, Some(error), None),
        };
        let service = Arc::new(Self {
            root: root.path().to_path_buf(),
            sink: Arc::new(CaptureSink {
                buffer: Arc::new(LogBuffer::new()),
                disk,
            }),
            disk_error,
            _guard: guard,
        });
        log_capture::install();
        log_capture::register(&service.sink);
        services.insert(key, Arc::downgrade(&service));
        Ok(service)
    }

    pub(crate) fn buffer(&self) -> &Arc<LogBuffer> {
        &self.sink.buffer
    }

    /// 磁盘写入的持久化错误（创建失败或队列丢条/I/O 失败）。
    pub(crate) fn disk_error(&self) -> Option<String> {
        self.disk_error
            .clone()
            .or_else(|| self.sink.disk.as_ref().and_then(|disk| disk.error()))
    }

    /// 磁盘读取服务（共享安全读取：文件名白名单、链接/硬链接拒绝、
    /// 2 MiB / 1000 行尾部上限）；磁盘不可用返回 None。
    pub(crate) fn disk_reader(&self) -> Option<DiskLogService> {
        self.sink
            .disk
            .is_some()
            .then(|| DiskLogService::new(self.root.join("logs")))
    }

    /// 直接记录一条应用日志（测试注入与自有验证日志；UI 路径走 tracing 采集）。
    #[cfg(test)]
    pub(crate) fn record(&self, level: &str, target: &str, message: &str) {
        if let Some(disk) = &self.sink.disk {
            disk.record(level, target, message);
        }
        self.sink.buffer.push(
            "应用",
            level,
            level_rank(level).unwrap_or(0),
            target,
            message,
            false,
        );
    }

    /// 把本应用磁盘日志原文按文件名顺序拼接到 `target`（用户经 NSH 原生
    /// 保存对话框选定）；不覆盖已有文件，失败清理半成品，源文件只读不删。
    pub(crate) fn export_disk_logs(&self, target: &Path) -> LanpathResult<PathBuf> {
        let Some(reader) = self.disk_reader() else {
            return Err(LanpathError::Config("磁盘日志不可用".into()));
        };
        let files = reader.export_files()?;
        if files.is_empty() {
            return Err(LanpathError::Config("没有可导出的日志文件".into()));
        }
        let target = target.to_path_buf();
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|error| export_error(&target, &error))?;
        let result = (|| -> LanpathResult<()> {
            for source in files {
                let input = File::open(&source)?;
                // 快照长度：持续增长的日志不会让导出无限进行。
                let len = input.metadata()?.len();
                io::copy(&mut input.take(len), &mut out)?;
            }
            out.flush()?;
            Ok(())
        })();
        if let Err(error) = result {
            drop(out);
            let _ = fs::remove_file(&target);
            return Err(error);
        }
        Ok(target)
    }
}

fn export_error(target: &Path, error: &io::Error) -> LanpathError {
    LanpathError::Config(format!("创建导出文件失败（{}）：{error}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_reach_memory_and_disk_on_the_same_root() {
        let temporary = tempfile::tempdir().unwrap();
        let service = LogService::for_root(temporary.path()).unwrap();
        assert!(service.disk_error().is_none());
        assert!(service.disk_reader().is_some());
        service.record("INFO", "lanpath::test", "服务记录验证消息");

        let snapshot = service.buffer().snapshot(None, 0, 1000).unwrap();
        assert!(
            snapshot
                .rows
                .iter()
                .any(|row| row.line.contains("服务记录验证消息")),
            "{:?}",
            snapshot
                .rows
                .iter()
                .map(|row| &row.line)
                .collect::<Vec<_>>()
        );

        // 释放全部句柄：Weak 缓存与采集注册自动失效，WorkerGuard 冲刷磁盘。
        drop(service);
        let logs = temporary.path().join("logs");
        let content = fs::read_dir(&logs)
            .unwrap()
            .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect::<String>();
        assert!(content.contains("INFO lanpath::test: 服务记录验证消息"));
    }

    #[test]
    fn reopening_the_same_root_reuses_the_service_and_its_buffer() {
        let temporary = tempfile::tempdir().unwrap();
        let first = LogService::for_root(temporary.path()).unwrap();
        first.record("WARN", "lanpath::test", "跨句保留消息");
        let reopened = LogService::for_root(temporary.path()).unwrap();
        assert!(Arc::ptr_eq(&first, &reopened));
        let snapshot = reopened.buffer().snapshot(None, 0, 1000).unwrap();
        assert!(
            snapshot
                .rows
                .iter()
                .any(|row| row.line.contains("跨句保留消息")),
            "{:?}",
            snapshot
                .rows
                .iter()
                .map(|row| &row.line)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn export_concatenates_real_bytes_and_never_overwrites() {
        let temporary = tempfile::tempdir().unwrap();
        let service = LogService::for_root(temporary.path()).unwrap();
        service.record("INFO", "lanpath::test", "导出验证消息甲");
        service.record("ERROR", "lanpath::test", "导出验证消息乙");
        drop(service);

        let service = LogService::for_root(temporary.path()).unwrap();
        let target = temporary.path().join("chosen-export.log");
        let exported = service.export_disk_logs(&target).unwrap();
        assert_eq!(exported, target);
        let content = fs::read_to_string(&target).unwrap();
        assert!(content.contains("导出验证消息甲"));
        assert!(content.contains("导出验证消息乙"));

        // 同一目标已存在：真实错误，且不会被覆盖。
        assert!(service.export_disk_logs(&target).is_err());
        let intact = fs::read_to_string(&target).unwrap();
        assert_eq!(intact, content);
    }

    #[test]
    fn export_to_a_missing_directory_is_a_real_error_without_a_partial_file() {
        let temporary = tempfile::tempdir().unwrap();
        let service = LogService::for_root(temporary.path()).unwrap();
        // 目标父目录不存在：创建失败，错误真实返回，且不留下半成品。
        let target = temporary.path().join("missing-dir").join("export.log");
        assert!(matches!(
            service.export_disk_logs(&target),
            Err(LanpathError::Config(message)) if message.contains("创建导出文件失败")
        ));
        assert!(!target.exists());
        assert!(!temporary.path().join("missing-dir").exists());
    }

    #[test]
    fn disk_reader_lists_and_tails_only_own_log_files() {
        let temporary = tempfile::tempdir().unwrap();
        let service = LogService::for_root(temporary.path()).unwrap();
        service.record("INFO", "lanpath::test", "尾部读取验证");
        drop(service);

        let service = LogService::for_root(temporary.path()).unwrap();
        let reader = service.disk_reader().unwrap();
        let names = reader.list_files().unwrap();
        assert!(!names.is_empty());
        assert!(names.iter().all(|name| name.starts_with("app.log")));
        let tail = reader.read_tail(&names[0], Some(10)).unwrap();
        assert!(tail.contains("尾部读取验证"));
    }
}
