//! NSH 偏好统一接入 app.toml 的 [ui] 节。
use crate::data_root::{DataRoot, open_root};
use lanpath_client::settings;
use lanpath_common::{LanpathError, LanpathResult};
use nsh_core::storage::{StorageBackend, StorageChange};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct UiDocument {
    theme: String,
    custom_theme: String,
    appearance: String,
    language: String,
    reduce_motion: String,
    window_material: String,
    window_opacity: String,
    /// NSH 自身的窗口等键与用户可读的外观设置分开。
    runtime: BTreeMap<String, String>,
}
impl Default for UiDocument {
    fn default() -> Self {
        Self {
            theme: "lanpath".into(),
            custom_theme: String::new(),
            appearance: "system".into(),
            language: "system".into(),
            reduce_motion: "system".into(),
            window_material: "acrylic".into(),
            window_opacity: "82".into(),
            runtime: BTreeMap::new(),
        }
    }
}
impl UiDocument {
    fn entries(self) -> BTreeMap<String, String> {
        let mut entries = self.runtime;
        entries.extend([
            ("theme".into(), self.theme),
            ("appearance".into(), self.appearance),
            ("language".into(), self.language),
            ("reduce_motion".into(), self.reduce_motion),
            ("window_material".into(), self.window_material),
            ("window_opacity".into(), self.window_opacity),
        ]);
        if !self.custom_theme.is_empty() {
            entries.insert("custom_theme".into(), self.custom_theme);
        }
        entries
    }
    fn from_entries(mut entries: BTreeMap<String, String>) -> Self {
        let default = Self::default();
        Self {
            theme: entries.remove("theme").unwrap_or(default.theme),
            custom_theme: entries
                .remove("custom_theme")
                .unwrap_or(default.custom_theme),
            appearance: entries.remove("appearance").unwrap_or(default.appearance),
            language: entries.remove("language").unwrap_or(default.language),
            reduce_motion: entries
                .remove("reduce_motion")
                .unwrap_or(default.reduce_motion),
            window_material: entries
                .remove("window_material")
                .unwrap_or(default.window_material),
            window_opacity: entries
                .remove("window_opacity")
                .unwrap_or(default.window_opacity),
            runtime: entries,
        }
    }
}
enum Write {
    Change(StorageChange),
    Flush(mpsc::Sender<()>),
    Stop,
}
/// 原生宿主的偏好后端。按序后台写盘，正常退出前 flush，不阻塞输入事件。
pub struct AppStorage {
    initial: BTreeMap<String, String>,
    sender: mpsc::Sender<Write>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    failure: Arc<Mutex<bool>>,
}
impl AppStorage {
    pub fn open(path: &Path) -> LanpathResult<Arc<Self>> {
        let root = open_root(path)?;
        let document = {
            let _guard = root.business_guard();
            match settings::load_app_section::<UiDocument>(root.path(), "ui")? {
                Some(document) => document,
                None => {
                    let document = UiDocument::default();
                    settings::persist_app_section(root.path(), "ui", &document)?;
                    document
                }
            }
        };
        let initial = document.entries();
        let entries = initial.clone();
        let (sender, receiver) = mpsc::channel();
        let failure = Arc::new(Mutex::new(false));
        let failed = failure.clone();
        let worker = thread::Builder::new()
            .name("lanpath-ui-settings".into())
            .spawn(move || writer(root, entries, receiver, failed))?;
        Ok(Arc::new(Self {
            initial,
            sender,
            worker: Mutex::new(Some(worker)),
            failure,
        }))
    }
    /// 屏障确保此前接受的修改完成；失败不伪装为已落盘。
    pub fn flush(&self) -> LanpathResult<()> {
        let (send, receive) = mpsc::channel();
        self.sender
            .send(Write::Flush(send))
            .map_err(|_| storage_error())?;
        receive.recv().map_err(|_| storage_error())?;
        if *self.failure.lock().map_err(|_| storage_error())? {
            Err(storage_error())
        } else {
            Ok(())
        }
    }
}
impl StorageBackend for AppStorage {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self.initial.clone())
    }
    fn write(&self, change: StorageChange) {
        if self.sender.send(Write::Change(change)).is_err() {
            if let Ok(mut failed) = self.failure.lock() {
                *failed = true;
            }
            tracing::error!("界面设置写入队列不可用");
        }
    }
}
impl Drop for AppStorage {
    fn drop(&mut self) {
        let _ = self.sender.send(Write::Stop);
        if let Ok(mut worker) = self.worker.lock()
            && let Some(worker) = worker.take()
        {
            let _ = worker.join();
        }
    }
}
fn writer(
    root: Arc<DataRoot>,
    mut entries: BTreeMap<String, String>,
    receiver: mpsc::Receiver<Write>,
    failure: Arc<Mutex<bool>>,
) {
    while let Ok(message) = receiver.recv() {
        match message {
            Write::Change(change) => {
                match change {
                    StorageChange::Set(key, value) => {
                        entries.insert(key, value);
                    }
                    StorageChange::Remove(key) => {
                        entries.remove(&key);
                    }
                    StorageChange::Clear => entries.clear(),
                }
                let document = UiDocument::from_entries(entries.clone());
                let _guard = root.business_guard();
                let result = settings::persist_app_section(root.path(), "ui", &document);
                if let Ok(mut failed) = failure.lock() {
                    *failed = result.is_err();
                }
                if result.is_err() {
                    tracing::error!("界面设置未能写入 app.toml");
                }
            }
            Write::Flush(done) => {
                let _ = done.send(());
            }
            Write::Stop => break,
        }
    }
}
fn storage_error() -> LanpathError {
    LanpathError::Config("界面设置未能写入 app.toml，请检查数据目录后重试".into())
}

#[cfg(test)]
#[path = "preferences_storage_tests.rs"]
mod tests;
