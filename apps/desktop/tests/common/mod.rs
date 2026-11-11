//! Headless 测试共用 harness（`nsh_testing::Headless`，NSH 0.5.0）：
//! 内存 storage 后端（预置条目、可回放写入）、桌面视口启动与
//! 每场景独立的临时数据根。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use nsh_core::element::Root;
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{Points, Size};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

/// NSH storage 的内存后端：以预置条目启动并记录写入，测试直接断言其状态。
#[derive(Debug, Default)]
pub struct MemoryStorage {
    entries: Mutex<BTreeMap<String, String>>,
}

impl MemoryStorage {
    pub fn new(entries: BTreeMap<String, String>) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(entries),
        })
    }

    /// 键的当前值；仅外观场景断言使用（其余测试目标视为未用）。
    #[allow(dead_code)]
    pub fn get(&self, key: &str) -> Option<String> {
        self.entries.lock().unwrap().get(key).cloned()
    }
}

impl StorageBackend for MemoryStorage {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self.entries.lock().unwrap().clone())
    }

    fn write(&self, change: StorageChange) {
        let mut entries = self.entries.lock().unwrap();
        match change {
            StorageChange::Set(key, value) => {
                entries.insert(key, value);
            }
            StorageChange::Remove(key) => {
                entries.remove(&key);
            }
            StorageChange::Clear => entries.clear(),
        }
    }
}

/// 以给定 storage 预置在桌面视口（1100×800，与原生窗口同量级；行内含
/// 开关/删除按钮，默认 390pt 手机视口过窄）启动应用。
pub fn start_app(entries: BTreeMap<String, String>) -> (Headless, Arc<MemoryStorage>) {
    let storage = MemoryStorage::new(entries);
    let config = EngineConfig {
        viewport: Size::new(Points(1100.0), Points(800.0)),
        storage: Some(storage.clone()),
        ..EngineConfig::new(Root::of::<lanpath_desktop_nsh::App>())
    };
    let app = Headless::start(config).expect("start the headless app");
    (app, storage)
}

/// 每个场景独立的临时数据根：经 storage 键注入，走与生产一致的数据根准入。
pub fn data_root() -> (tempfile::TempDir, BTreeMap<String, String>) {
    let dir = tempfile::tempdir().expect("create a temporary data root");
    let mut entries = BTreeMap::new();
    entries.insert(
        "lanpath_desktop_nsh.data_dir".to_owned(),
        dir.path().to_string_lossy().into_owned(),
    );
    (dir, entries)
}

/// 屏幕上全部文本。
pub fn texts(app: &Headless) -> Vec<&str> {
    app.screen().texts()
}
