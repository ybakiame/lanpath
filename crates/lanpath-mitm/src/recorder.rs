//! MITM 流量记录。

use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use uuid::Uuid;

const DEFAULT_BYTE_CAPACITY: usize = 32 * 1024 * 1024;
/// 一条被捕获的 HTTP 交换记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrafficRecord {
    pub id: Uuid,
    pub method: String,
    pub url: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<String>,
    pub response_status: u16,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: u64,
}

/// 流量记录持久化抽象。
pub trait TrafficRecorder: Send + Sync {
    fn record(&self, rec: TrafficRecord);
    fn list(&self) -> Vec<TrafficRecord>;
}

/// Retains the newest records within both the count and owned-allocation budgets.
#[derive(Debug)]
pub struct MemoryRecorder {
    cap: usize,
    byte_cap: usize,
    inner: Mutex<RecordBuffer>,
}

#[derive(Debug, Default)]
struct RecordBuffer {
    bytes: usize,
    records: VecDeque<(usize, TrafficRecord)>,
}

impl MemoryRecorder {
    pub fn new(cap: usize) -> Self {
        Self::with_limits(cap, DEFAULT_BYTE_CAPACITY)
    }

    pub fn with_limits(cap: usize, byte_cap: usize) -> Self {
        Self {
            cap,
            byte_cap,
            inner: Mutex::new(RecordBuffer::default()),
        }
    }
}

fn retained_bytes(record: &TrafficRecord) -> usize {
    let mut bytes = std::mem::size_of::<(usize, TrafficRecord)>()
        .saturating_add(record.method.capacity())
        .saturating_add(record.url.capacity())
        .saturating_add(record.request_body.as_ref().map_or(0, String::capacity))
        .saturating_add(record.response_body.as_ref().map_or(0, String::capacity));
    for headers in [&record.request_headers, &record.response_headers] {
        bytes = bytes.saturating_add(
            headers
                .capacity()
                .saturating_mul(std::mem::size_of::<(String, String)>()),
        );
        for (name, value) in headers {
            bytes = bytes
                .saturating_add(name.capacity())
                .saturating_add(value.capacity());
        }
    }
    bytes
}

impl TrafficRecorder for MemoryRecorder {
    fn record(&self, rec: TrafficRecord) {
        if self.cap == 0 || self.byte_cap == 0 {
            return;
        }
        let cost = retained_bytes(&rec);
        if cost > self.byte_cap {
            return;
        }
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while inner.records.len() >= self.cap || inner.bytes > self.byte_cap - cost {
            let Some((removed, _)) = inner.records.pop_front() else {
                break;
            };
            inner.bytes -= removed;
        }
        inner.bytes += cost;
        inner.records.push_back((cost, rec));
    }

    fn list(&self) -> Vec<TrafficRecord> {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner
            .records
            .iter()
            .map(|(_, record)| record.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(url: &str) -> TrafficRecord {
        TrafficRecord {
            id: Uuid::new_v4(),
            method: "GET".to_string(),
            url: url.to_string(),
            request_headers: Vec::new(),
            request_body: None,
            response_status: 200,
            response_headers: Vec::new(),
            response_body: None,
            timestamp: Utc::now(),
            duration_ms: 0,
        }
    }

    #[test]
    fn memory_recorder_evicts_oldest_when_full() {
        let recorder = MemoryRecorder::new(3);
        for i in 0..5 {
            recorder.record(record(&format!("http://example.com/{i}")));
        }
        let list = recorder.list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].url, "http://example.com/2");
        assert_eq!(list[2].url, "http://example.com/4");
    }

    #[test]
    fn memory_recorder_cap_zero_keeps_nothing() {
        let recorder = MemoryRecorder::new(0);
        recorder.record(record("http://example.com/"));
        assert!(recorder.list().is_empty());
    }

    #[test]
    fn byte_budget_evicts_oldest_even_before_count_capacity() {
        let recorder = MemoryRecorder::with_limits(10, 8192);
        for path in ["first", "second", "third"] {
            let mut rec = record(path);
            rec.response_body = Some(path.repeat(600));
            recorder.record(rec);
        }
        let records = recorder.list();
        assert_eq!(
            records
                .iter()
                .map(|record| record.url.as_str())
                .collect::<Vec<_>>(),
            ["second", "third"]
        );
        assert_eq!(
            records[0].response_body.as_deref(),
            Some("second".repeat(600).as_str())
        );
    }

    #[test]
    fn one_oversized_record_does_not_destroy_existing_history() {
        let recorder = MemoryRecorder::with_limits(10, 8192);
        recorder.record(record("retained"));
        let mut oversized = record("oversized");
        oversized.request_body = Some("x".repeat(8193));
        recorder.record(oversized);
        assert_eq!(
            recorder
                .list()
                .iter()
                .map(|record| record.url.as_str())
                .collect::<Vec<_>>(),
            ["retained"]
        );
    }
}
