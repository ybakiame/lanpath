use std::{
    fmt::{self, Write as _},
    sync::Arc,
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

pub(crate) fn install(
    buffer: Arc<LogBuffer>,
    disk: Option<Arc<DiskLogWriter>>,
) -> Result<(), tracing_subscriber::util::TryInitError> {
    tracing_subscriber::registry()
        .with(
            CaptureLayer { buffer, disk }.with_filter(tracing_subscriber::filter::filter_fn(
                |metadata| metadata.target().starts_with("lanpath_"),
            )),
        )
        .try_init()
}

struct CaptureLayer {
    buffer: Arc<LogBuffer>,
    disk: Option<Arc<DiskLogWriter>>,
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = EventFields {
            message: LimitedText::default(),
        };
        event.record(&mut fields);
        let metadata = event.metadata();
        if let Some(disk) = &self.disk {
            disk.app(
                metadata.level().as_str(),
                metadata.target(),
                &fields.message.text,
            );
        }
        self.buffer.push(
            "应用",
            metadata.level().as_str(),
            level_rank(metadata.level().as_str()).unwrap_or(0),
            metadata.target(),
            &fields.message.text,
            fields.message.truncated,
        );
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
}
