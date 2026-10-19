use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub(crate) const LOG_CAPACITY: usize = 1000;
pub(crate) const MESSAGE_BYTES: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CoreLogState {
    Stopped,
    Connecting,
    Live,
    Disconnected(String),
    Unavailable(String),
}

pub(crate) struct LogEntry {
    pub(crate) id: u64,
    pub(crate) rank: u8,
    pub(crate) line: Arc<str>,
}

pub(crate) struct LogSnapshot {
    pub(crate) revision: u64,
    pub(crate) rows: Vec<Arc<LogEntry>>,
    pub(crate) total: usize,
    pub(crate) evicted: u64,
    pub(crate) state: CoreLogState,
}

struct BufferState {
    rows: VecDeque<Arc<LogEntry>>,
    revision: u64,
    next_id: u64,
    evicted: u64,
    core: CoreLogState,
}

pub(crate) struct LogBuffer(Mutex<BufferState>);

impl LogBuffer {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(BufferState {
            rows: VecDeque::with_capacity(LOG_CAPACITY),
            revision: 0,
            next_id: 1,
            evicted: 0,
            core: CoreLogState::Stopped,
        }))
    }

    pub(crate) fn push(
        &self,
        source: &str,
        level: &str,
        rank: u8,
        target: &str,
        message: &str,
        truncated: bool,
    ) {
        let end = message.floor_char_boundary(message.len().min(MESSAGE_BYTES));
        let clipped = if truncated || end < message.len() {
            " [截断]"
        } else {
            ""
        };
        let line: Arc<str> = format!(
            "{} [{level}] {source} {target}: {}{clipped}",
            chrono::Local::now().to_rfc3339(),
            &message[..end]
        )
        .into();
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let id = state.next_id;
        state.next_id += 1;
        if state.rows.len() == LOG_CAPACITY {
            state.rows.pop_front();
            state.evicted += 1;
        }
        state.rows.push_back(Arc::new(LogEntry { id, rank, line }));
        state.revision += 1;
    }

    pub(crate) fn set_core(&self, core: CoreLogState) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.core != core {
            state.core = core;
            state.revision += 1;
        }
    }

    pub(crate) fn clear(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.rows.clear();
        state.evicted = 0;
        state.revision += 1;
    }

    pub(crate) fn snapshot(
        &self,
        previous: Option<u64>,
        min_rank: u8,
        limit: usize,
    ) -> Option<LogSnapshot> {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if previous == Some(state.revision) {
            return None;
        }
        Some(LogSnapshot {
            revision: state.revision,
            rows: state
                .rows
                .iter()
                .rev()
                .filter(|row| row.rank >= min_rank)
                .take(limit.min(LOG_CAPACITY))
                .cloned()
                .collect(),
            total: state.rows.len(),
            evicted: state.evicted,
            state: state.core.clone(),
        })
    }
}

pub(crate) fn level_rank(level: &str) -> Option<u8> {
    match level {
        "trace" | "TRACE" => Some(0),
        "debug" | "DEBUG" => Some(1),
        "info" | "INFO" => Some(2),
        "warn" | "WARN" => Some(3),
        "error" | "ERROR" | "fatal" | "panic" => Some(4),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eviction_filtering_and_clear_preserve_entry_identity() {
        let buffer = LogBuffer::new();
        for id in 1..=LOG_CAPACITY + 5 {
            buffer.push(
                "应用",
                if id % 2 == 0 { "ERROR" } else { "INFO" },
                if id % 2 == 0 { 4 } else { 2 },
                "test",
                "event",
                false,
            );
        }
        let all = buffer.snapshot(None, 0, LOG_CAPACITY).unwrap();
        assert_eq!(all.total, LOG_CAPACITY);
        assert_eq!(all.evicted, 5);
        assert_eq!(all.rows.first().unwrap().id, 1005);
        assert_eq!(all.rows.last().unwrap().id, 6);
        let errors = buffer.snapshot(None, 4, 200).unwrap();
        assert_eq!(errors.rows.len(), 200);
        assert_eq!(errors.rows.first().unwrap().id, 1004);
        assert!(errors.rows.iter().all(|row| row.id % 2 == 0));
        assert!(buffer.snapshot(Some(all.revision), 0, 1000).is_none());
        buffer.clear();
        buffer.push("应用", "ERROR", 4, "test", "after clear", false);
        let next = buffer.snapshot(Some(all.revision), 0, 1000).unwrap();
        assert_eq!(next.evicted, 0);
        assert_eq!(
            next.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            [1006]
        );
    }

    #[test]
    fn oversized_unicode_message_is_bounded_without_cutting_a_character() {
        let buffer = LogBuffer::new();
        buffer.push(
            "核心",
            "INFO",
            2,
            "sing-box",
            &"中".repeat(MESSAGE_BYTES),
            false,
        );
        let snapshot = buffer.snapshot(None, 0, 1).unwrap();
        let line = &snapshot.rows[0].line;
        assert_eq!(line.matches('中').count(), MESSAGE_BYTES / 3);
        assert!(line.len() < MESSAGE_BYTES + 128);
    }
}
