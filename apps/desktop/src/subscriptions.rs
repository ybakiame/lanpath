//! 订阅业务：共享 `lanpath-client::SubscriptionStore` 落在 NSH 桌面自己的
//! 隔离数据根内。不读取/迁移 Tauri/GPUI/apps/nsh 数据；不发起网络请求。

#[cfg(test)]
use std::path::Path;
use std::sync::Arc;

use lanpath_client::{
    Subscription, SubscriptionStore, normalize_resource_url, validate_subscription_url,
};
use lanpath_common::{LanpathError, LanpathResult};
use uuid::Uuid;

use crate::data_root::{DataRoot, open_root, resolve_data_location};

pub(crate) struct Subscriptions {
    root: Arc<DataRoot>,
}

impl Subscriptions {
    /// 解析本进程数据根并打开订阅业务（应用启动路径）。
    pub(crate) fn open() -> LanpathResult<Arc<Self>> {
        let location = resolve_data_location()?;
        let root = open_root(&location.path)?;
        Ok(Arc::new(Self { root }))
    }

    /// 指定数据根打开（测试与显式调用路径，同样走完整准入）。
    #[cfg(test)]
    pub(crate) fn for_root(path: &Path) -> LanpathResult<Arc<Self>> {
        let root = open_root(path)?;
        Ok(Arc::new(Self { root }))
    }

    fn store(&self) -> SubscriptionStore {
        SubscriptionStore::new(self.root.path().to_path_buf())
    }

    /// 读取订阅列表；文件缺失按空列表，损坏时共享存储层已回退空列表并告警。
    pub(crate) fn load(&self) -> LanpathResult<Vec<Subscription>> {
        self.store().load()
    }

    /// 校验并新增订阅后持久化；任何失败都不触碰已有数据。
    pub(crate) fn add(&self, name: &str, url: &str) -> LanpathResult<Subscription> {
        let name = required_text(name, "订阅名称")?;
        let url = normalize_resource_url(url.trim());
        validate_subscription_url(&url).map_err(LanpathError::Validation)?;
        let subscription = self.store().add(&name, &url, true, None)?;
        tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "新增订阅：{}", subscription.name.escape_debug());
        Ok(subscription)
    }

    pub(crate) fn set_enabled(&self, id: Uuid, enabled: bool) -> LanpathResult<()> {
        self.store().set_enabled(id, enabled)?;
        tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "{}订阅：{id}", if enabled { "启用" } else { "停用" });
        Ok(())
    }

    pub(crate) fn remove(&self, id: Uuid) -> LanpathResult<()> {
        self.store().remove(id)?;
        tracing::info!(target: "lanpath_desktop_nsh::subscriptions", "删除订阅：{id}");
        Ok(())
    }
}

fn required_text(value: &str, field: &str) -> LanpathResult<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(LanpathError::Validation(format!("{field}不能为空")))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_loads_an_empty_subscription_list() {
        let temporary = tempfile::tempdir().unwrap();
        let service = Subscriptions::for_root(temporary.path()).unwrap();
        assert!(service.load().unwrap().is_empty());
        assert!(!temporary.path().join("subscriptions.json").exists());
    }

    #[test]
    fn add_persists_and_reopen_on_the_same_root_restores_it() {
        let temporary = tempfile::tempdir().unwrap();
        let service = Subscriptions::for_root(temporary.path()).unwrap();
        let added = service
            .add("东京专线", " https://example.com/sub.yaml ")
            .unwrap();
        assert!(added.enabled);
        assert_eq!(added.node_count, 0);
        assert!(added.format.is_none());

        // 同一数据根上的“重开”：释放业务句柄后重新打开。
        let path = temporary.path().to_path_buf();
        let reopened = Subscriptions::for_root(&path).unwrap();
        let loaded = reopened.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, added.id);
        assert_eq!(loaded[0].name, "东京专线");
        assert_eq!(loaded[0].url, "https://example.com/sub.yaml");
    }

    #[test]
    fn invalid_input_is_rejected_and_existing_data_is_untouched() {
        let temporary = tempfile::tempdir().unwrap();
        let service = Subscriptions::for_root(temporary.path()).unwrap();
        let kept = service.add("保留", "https://example.com/a.yaml").unwrap();

        for (name, url) in [
            ("", "https://example.com/b.yaml"),
            ("   ", "https://example.com/b.yaml"),
            ("新订阅", ""),
            ("新订阅", "ftp://example.com/c.yaml"),
            ("新订阅", "not-a-url"),
        ] {
            assert!(service.add(name, url).is_err(), "accepted {name:?} {url:?}");
        }

        let loaded = service.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, kept.id);
        let text = std::fs::read_to_string(temporary.path().join("subscriptions.json")).unwrap();
        assert!(text.contains("保留"));
        assert!(!text.contains("新订阅"));
    }

    #[test]
    fn enabled_toggle_and_remove_persist() {
        let temporary = tempfile::tempdir().unwrap();
        let service = Subscriptions::for_root(temporary.path()).unwrap();
        let first = service.add("一号", "https://example.com/1.yaml").unwrap();
        let second = service.add("二号", "https://example.com/2.yaml").unwrap();

        service.set_enabled(first.id, false).unwrap();
        let path = temporary.path().to_path_buf();
        let reopened = Subscriptions::for_root(&path).unwrap();
        let loaded = reopened.load().unwrap();
        let first = loaded.iter().find(|s| s.id == first.id).unwrap();
        let second = loaded.iter().find(|s| s.id == second.id).unwrap();
        assert!(!first.enabled);
        assert!(second.enabled);

        reopened.remove(first.id).unwrap();
        let loaded = reopened.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, second.id);
    }

    #[test]
    fn save_error_does_not_overwrite_existing_list() {
        let temporary = tempfile::tempdir().unwrap();
        let service = Subscriptions::for_root(temporary.path()).unwrap();
        service.add("保留", "https://example.com/a.yaml").unwrap();

        // 把订阅文件设为只读使保存失败：读取仍正常，新增失败，
        // 已有数据必须原样保留。
        let file = temporary.path().join("subscriptions.json");
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        let readonly = permissions.readonly();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions).unwrap();
        let result = service.add("新订阅", "https://example.com/b.yaml");
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(readonly);
        std::fs::set_permissions(&file, permissions).unwrap();

        assert!(result.is_err());
        let loaded = service.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "保留");
    }
}
