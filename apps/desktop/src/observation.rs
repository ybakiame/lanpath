use std::{path::PathBuf, sync::Arc};

use lanpath_client::{
    connections::{
        ActiveConnections, ConnectionTrackerHandle, ConnectionView, start_connection_tracker,
    },
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
    pub(crate) daily: Vec<DailyStatRow>,
    pub(crate) records: Vec<ConnRecordRow>,
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

    pub(crate) async fn active(&self) -> Result<ActiveConnections, String> {
        match self.tracker.lock().await.as_ref() {
            Some(tracker) => Ok(tracker.active().await),
            None => Err("连接跟踪器未运行".into()),
        }
    }

    pub(crate) async fn closed(&self) -> Vec<ConnectionView> {
        match self.tracker.lock().await.as_ref() {
            Some(tracker) => tracker.closed().await,
            None => Vec::new(),
        }
    }

    pub(crate) async fn stats(
        &self,
        daily: &DailyQuery,
        records: &RecordsQuery,
    ) -> Result<StatsSnapshot, String> {
        let store = self.store().await?;
        let date = today();
        let summary = store
            .today_summary(&date)
            .await
            .map_err(|e| e.to_string())?;
        let daily = store
            .query_daily(daily, &date)
            .await
            .map_err(|e| e.to_string())?;
        let records = store
            .query_records(records)
            .await
            .map_err(|e| e.to_string())?;
        Ok(StatsSnapshot {
            summary,
            daily,
            records,
        })
    }

    pub(crate) async fn clear_stats(&self) -> Result<(), String> {
        self.store().await?.clear().await.map_err(|e| e.to_string())
    }
}
