use std::{path::PathBuf, sync::Arc};

use lanpath_client::{
    connections::{ConnectionTrackerHandle, ConnectionView, start_connection_tracker},
    stats::{
        ConnRecordRow, DailyQuery, DailyStatRow, RecordsQuery, StatsStore, TodaySummary, today,
    },
};
use tokio::sync::{Mutex, OnceCell};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum StatsKind {
    Daily,
    Records,
}

#[derive(PartialEq)]
pub(crate) enum StatsRows {
    Daily(Vec<DailyStatRow>),
    Records(Vec<ConnRecordRow>),
}

pub(crate) struct StatsSnapshot {
    pub(crate) summary: TodaySummary,
    pub(crate) rows: StatsRows,
}

pub(crate) struct ObservationService {
    data_dir: PathBuf,
    store: OnceCell<Arc<StatsStore>>,
    tracker: Mutex<Option<ConnectionTrackerHandle>>,
}

impl ObservationService {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            store: OnceCell::new(),
            tracker: Mutex::new(None),
        }
    }

    async fn store(&self) -> Result<&Arc<StatsStore>, String> {
        self.store
            .get_or_try_init(|| async {
                tokio::fs::create_dir_all(&self.data_dir)
                    .await
                    .map_err(|e| e.to_string())?;
                StatsStore::open(&self.data_dir)
                    .await
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .await
    }

    pub(crate) async fn prepare(&self) -> Result<(), String> {
        self.store().await.map(|_| ())
    }

    pub(crate) async fn start(&self, port: u16) -> Result<(), String> {
        let store = self.store().await?;
        let mut tracker = self.tracker.lock().await;
        if tracker.is_some() {
            return Err("隔离连接跟踪器已经运行".into());
        }
        *tracker = Some(start_connection_tracker(
            port,
            String::new(),
            Some(store.clone()),
        ));
        Ok(())
    }

    pub(crate) async fn stop(&self) {
        let mut tracker = self.tracker.lock().await;
        if let Some(tracker) = tracker.take() {
            // Shared tracker stop cancels its task; this is not a final sample
            // or a database drain. Already committed batches remain queryable.
            tracker.stop().await;
        }
    }

    pub(crate) async fn closed(&self) -> Vec<ConnectionView> {
        match self.tracker.lock().await.as_ref() {
            Some(tracker) => tracker.closed().await,
            None => Vec::new(),
        }
    }

    pub(crate) async fn stats(&self, kind: StatsKind) -> Result<StatsSnapshot, String> {
        let store = self.store().await?;
        let date = today();
        let summary = store
            .today_summary(&date)
            .await
            .map_err(|e| e.to_string())?;
        let rows = match kind {
            StatsKind::Daily => StatsRows::Daily(
                store
                    .query_daily(&DailyQuery::default(), &date)
                    .await
                    .map_err(|e| e.to_string())?,
            ),
            StatsKind::Records => StatsRows::Records(
                store
                    .query_records(&RecordsQuery::default())
                    .await
                    .map_err(|e| e.to_string())?,
            ),
        };
        Ok(StatsSnapshot { summary, rows })
    }
}
