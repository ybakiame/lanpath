//! tracing 采集层：把本应用自身（`lanpath_*` target）的 tracing 事件写入
//! 已注册的日志服务（内存缓冲 + 可选磁盘）。从 apps/gpui 的 `log_capture`
//! 迁移：NSH 桌面没有核心日志流，采集来源只有本应用进程自身。
//!
//! 进程内可同时注册多个服务（headless 测试的并行场景各持独立数据根）：
//! 事件扇出到所有存活服务，生产进程永远只有一个。注册表惰性清理已死的
//! Weak，服务随页面/进程退出后自动脱离采集。

use std::{
    fmt::{self, Write as _},
    sync::{Arc, Mutex, Once, OnceLock, Weak},
};

use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    Layer,
    layer::{Context, SubscriberExt as _},
    util::SubscriberInitExt as _,
};

use crate::{
    disk_log_writer::DiskLogWriter,
    log_buffer::{LogBuffer, MESSAGE_BYTES, level_rank},
};

/// 一个采集目的地：内存缓冲 + 该数据根的磁盘写入器。
pub(crate) struct CaptureSink {
    pub(crate) buffer: Arc<LogBuffer>,
    pub(crate) disk: Option<Arc<DiskLogWriter>>,
}

static SINKS: OnceLock<Mutex<Vec<Weak<CaptureSink>>>> = OnceLock::new();
static INSTALL: Once = Once::new();

fn sinks() -> &'static Mutex<Vec<Weak<CaptureSink>>> {
    SINKS.get_or_init(|| Mutex::new(Vec::new()))
}

/// 注册一个采集目的地；重复注册同一目的地是幂等的。
pub(crate) fn register(sink: &Arc<CaptureSink>) {
    let mut sinks = sinks().lock().unwrap_or_else(|e| e.into_inner());
    sinks.retain(|weak| {
        weak.upgrade()
            .is_some_and(|existing| !Arc::ptr_eq(&existing, sink))
    });
    sinks.push(Arc::downgrade(sink));
}

/// 安装全局采集层（每进程一次，幂等）；重复调用或已有全局订阅者时静默跳过。
pub(crate) fn install() {
    INSTALL.call_once(|| {
        let _ = tracing_subscriber::registry()
            .with(
                CaptureLayer.with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                    metadata.target().starts_with("lanpath_")
                })),
            )
            .try_init();
    });
}

struct CaptureLayer;

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = EventFields {
            message: LimitedText::default(),
        };
        event.record(&mut fields);
        let metadata = event.metadata();
        let level = metadata.level().as_str();
        let rank = level_rank(level).unwrap_or(0);
        let mut sinks = sinks().lock().unwrap_or_else(|e| e.into_inner());
        sinks.retain(|weak| weak.strong_count() > 0);
        for sink in sinks.iter().filter_map(Weak::upgrade) {
            if let Some(disk) = &sink.disk {
                disk.record(level, metadata.target(), &fields.message.text);
            }
            sink.buffer.push(
                "应用",
                level,
                rank,
                metadata.target(),
                &fields.message.text,
                fields.message.truncated,
            );
        }
    }
}

#[derive(Default)]
struct LimitedText {
    text: String,
    truncated: bool,
}

impl fmt::Write for LimitedText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.truncated {
            return Ok(());
        }
        let remaining = MESSAGE_BYTES.saturating_sub(self.text.len());
        let end = text.floor_char_boundary(text.len().min(remaining));
        self.text.push_str(&text[..end]);
        self.truncated |= end < text.len();
        Ok(())
    }
}

struct EventFields {
    message: LimitedText,
}

impl EventFields {
    fn begin_field(&mut self, field: &Field) -> bool {
        if self.message.truncated {
            return false;
        }
        if !self.message.text.is_empty() {
            let _ = self.message.write_str(" ");
        }
        if field.name() != "message" {
            let _ = write!(self.message, "{}=", field.name());
        }
        !self.message.truncated
    }
}

impl Visit for EventFields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if self.begin_field(field) {
            let _ = write!(self.message, "{value:?}");
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if self.begin_field(field) {
            let _ = self.message.write_str(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_buffer::LOG_CAPACITY;

    #[test]
    fn unicode_truncation_never_appends_later_fields() {
        let prefix = "中".repeat(MESSAGE_BYTES / 3);
        let mut text = LimitedText::default();
        text.write_str(&prefix).unwrap();
        text.write_str("中").unwrap();
        text.write_str("x").unwrap();
        assert_eq!(text.text, prefix);
        assert!(text.truncated);
    }

    /// 真实采集路径：tracing 事件经全局层进入注册服务的内存缓冲，
    /// 非 lanpath_* target 被过滤，`lanpath_` 前缀 target（含测试注入）被捕获得到。
    #[test]
    fn tracing_events_reach_registered_sinks_and_filter_by_target() {
        install();
        let sink = Arc::new(CaptureSink {
            buffer: Arc::new(LogBuffer::new()),
            disk: None,
        });
        register(&sink);

        let unique = format!("capture-unique-{}", std::process::id());
        tracing::info!(target: "other_crate::noise", "应被过滤 {unique}");
        tracing::warn!(target: "lanpath_desktop_nsh::capture_test", "{unique}");
        tracing::error!(target: "lanpath_capture_test", "second {unique}");

        let snapshot = sink.buffer.snapshot(None, 0, LOG_CAPACITY).unwrap();
        let lines = snapshot
            .rows
            .iter()
            .map(|row| row.line.to_string())
            .collect::<Vec<_>>();
        assert!(
            lines
                .iter()
                .any(|line| line.contains(&unique) && line.contains("[WARN]")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("second") && line.contains("[ERROR]")),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|line| line.contains("应被过滤")),
            "{lines:?}"
        );
    }
}
