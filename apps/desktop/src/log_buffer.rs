//! 进程内内存日志缓冲：从 apps/gpui 的 `log_buffer` 迁移，去掉核心日志流状态
//! （NSH 桌面不启动核心，没有核心日志连接）。保留有界缓冲、级别筛选、
//! revision 增量快照、淘汰计数与清空语义，约束未削弱。

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub(crate) const LOG_CAPACITY: usize = 1000;
pub(crate) const MESSAGE_BYTES: usize = 8192;

/// 供 UI 展示的一条日志行：`line` 已包含时间、级别、目标与截断标记。
#[derive(Clone, Debug)]
pub(crate) struct LogEntry {
    pub(crate) id: u64,
    pub(crate) rank: u8,
    pub(crate) line: Arc<str>,
}

#[derive(Clone, Debug)]
pub(crate) struct LogSnapshot {
    pub(crate) revision: u64,
    pub(crate) rows: Vec<Arc<LogEntry>>,
    pub(crate) total: usize,
    pub(crate) evicted: u64,
}

struct BufferState {
    rows: VecDeque<Arc<LogEntry>>,
    revision: u64,
    next_id: u64,
    evicted: u64,
}

/// 有界、线程安全的日志缓冲：超出容量淘汰最旧条目，revision 在每次
/// 变更（含清空）时递增，供页面做增量拉取。
pub(crate) struct LogBuffer(Mutex<BufferState>);

impl LogBuffer {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(BufferState {
            rows: VecDeque::with_capacity(LOG_CAPACITY),
            revision: 0,
            next_id: 1,
            evicted: 0,
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

    pub(crate) fn clear(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.rows.clear();
        state.evicted = 0;
        state.revision += 1;
    }

    /// 自 `previous` revision 以来的增量快照；未变化返回 `None`。
    /// 行按最新在前返回，`min_rank` 为级别下限（0 全部），`limit` 限制返回条数。
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
            "应用",
            "INFO",
            2,
            "lanpath",
            &"中".repeat(MESSAGE_BYTES),
            false,
        );
        let snapshot = buffer.snapshot(None, 0, 1).unwrap();
        let line = &snapshot.rows[0].line;
        assert_eq!(line.matches('中').count(), MESSAGE_BYTES / 3);
        assert!(line.len() < MESSAGE_BYTES + 128);
        assert!(line.ends_with("[截断]"));
    }

    #[test]
    fn known_levels_map_to_ranks_and_unknown_levels_are_rejected() {
        assert_eq!(level_rank("trace"), Some(0));
        assert_eq!(level_rank("DEBUG"), Some(1));
        assert_eq!(level_rank("info"), Some(2));
        assert_eq!(level_rank("WARN"), Some(3));
        assert_eq!(level_rank("error"), Some(4));
        assert_eq!(level_rank("panic"), Some(4));
        assert_eq!(level_rank("notice"), None);
    }
}
