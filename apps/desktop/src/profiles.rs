//! 覆写模板（Profile）业务：共享 `lanpath-client::ProfileStoreV2` 落在 NSH
//! 桌面自己的隔离数据根内（本根 `profiles.json`），不读取/迁移其他应用的
//! 数据。模板只在本页维护：YAML/JS 本地覆写与两个远程覆写地址；远程地址
//! 仅保存供后续配置合成时解析，本切片不下载、不预览、不显示“已生效”。
//!
//! 所有清单读写与订阅业务共用数据根的同一把业务锁：删除前的“无订阅
//! 引用”校验与落盘在同一临界区内完成，订阅侧的绑定校验也在同一把锁内，
//! 不会出现“校验无引用后另一线程完成绑定”的竞争。本服务自身无状态、
//! 无独立锁，重开只是再取一次共享数据根句柄。

use std::path::Path;
use std::sync::Arc;

use lanpath_client::{
    Profile, ProfileStoreV2, SubscriptionStore, normalize_optional_url, validate_js_override,
    validate_remote_url, validate_yaml_override,
};
use lanpath_common::{LanpathError, LanpathResult};
use uuid::Uuid;

use crate::data_root::{DataRoot, open_root, resolve_data_location};

/// 覆写模板业务句柄：仅持有共享数据根，克隆廉价；所有操作临时取业务锁。
#[derive(Clone)]
pub(crate) struct Profiles {
    root: Arc<DataRoot>,
}

impl Profiles {
    /// 解析本进程数据根并打开覆写模板业务（应用启动路径）。
    pub(crate) fn open() -> LanpathResult<Self> {
        let location = resolve_data_location()?;
        Self::for_root(&location.path)
    }

    /// 指定数据根打开（测试与显式调用路径，同样走完整准入）。
    pub(crate) fn for_root(path: &Path) -> LanpathResult<Self> {
        Ok(Self {
            root: open_root(path)?,
        })
    }

    fn store(&self) -> ProfileStoreV2 {
        ProfileStoreV2::new(self.root.path().to_path_buf())
    }

    /// 读取模板列表；文件缺失按空列表，损坏时共享存储层已回退空列表并告警。
    pub(crate) fn load(&self) -> LanpathResult<Vec<Profile>> {
        let _guard = self.root.business_guard();
        self.store().load()
    }

    /// 校验并保存模板（`id` 为 `None` 时新建）：名称 trim 必填且除自身外
    /// 唯一；YAML/JS 覆写真实调用共享校验（空允许）；远程 URL 可选、
    /// 规范化并共享校验。任何失败都不触碰已有数据。
    pub(crate) fn save(
        &self,
        id: Option<Uuid>,
        name: &str,
        yaml_override: String,
        js_override: String,
        yaml_url: Option<String>,
        js_url: Option<String>,
    ) -> LanpathResult<Profile> {
        let name = required_name(name)?;
        validate_yaml_override(&yaml_override).map_err(LanpathError::Validation)?;
        validate_js_override(&js_override).map_err(LanpathError::Validation)?;
        validate_remote_url(&yaml_url).map_err(LanpathError::Validation)?;
        validate_remote_url(&js_url).map_err(LanpathError::Validation)?;
        let yaml_url = normalize_optional_url(yaml_url);
        let js_url = normalize_optional_url(js_url);
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let mut profiles = self.store().load()?;
        if profiles
            .iter()
            .any(|candidate| Some(candidate.id) != id && candidate.name == name)
        {
            return Err(LanpathError::Validation(format!(
                "覆写模板名称「{name}」已存在"
            )));
        }
        let profile = Profile {
            id: id.unwrap_or_else(Uuid::new_v4),
            name,
            yaml_override,
            js_override,
            yaml_url,
            js_url,
        };
        match id {
            Some(id) => {
                let target = profiles
                    .iter_mut()
                    .find(|candidate| candidate.id == id)
                    .ok_or_else(|| LanpathError::Client("覆写模板不存在".into()))?;
                *target = profile.clone();
            }
            None => profiles.push(profile.clone()),
        }
        self.store().save(&profiles)?;
        tracing::info!(target: "lanpath_desktop_nsh::profiles", "保存覆写模板：{}", profile.name.escape_debug());
        Ok(profile)
    }

    /// 删除模板；同一锁内检查订阅引用，被任一订阅 `profile_id` 引用时
    /// 拒绝（须先在订阅侧解除绑定），不静默解除引用。
    pub(crate) fn remove(&self, id: Uuid) -> LanpathResult<()> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        if SubscriptionStore::new(self.root.path().to_path_buf())
            .load()?
            .iter()
            .any(|subscription| subscription.profile_id == Some(id))
        {
            return Err(LanpathError::Client(
                "仍有订阅关联此覆写模板，请先取消关联".into(),
            ));
        }
        self.store().remove(id)?;
        tracing::info!(target: "lanpath_desktop_nsh::profiles", "删除覆写模板：{id}");
        Ok(())
    }
}

/// 在指定数据根校验模板引用存在（调用方须已持有该根业务锁）。
/// 模板被外部删除或 `profiles.json` 损坏（共享层回退空列表）都会得到
/// 真实的“不存在”错误，由调用方展示，不静默改绑。
pub(crate) fn validate_profile_reference_locked(root: &Path, id: Uuid) -> LanpathResult<()> {
    if ProfileStoreV2::new(root.to_path_buf())
        .load()?
        .iter()
        .any(|profile| profile.id == id)
    {
        Ok(())
    } else {
        Err(LanpathError::Client("覆写模板不存在，请重新选择".into()))
    }
}

fn required_name(value: &str) -> LanpathResult<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(LanpathError::Validation("模板名称不能为空".into()))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
