use lanpath_client::{
    Profile, ProfileStoreV2, normalize_optional_url, validate_js_override, validate_remote_url,
    validate_yaml_override,
};
use lanpath_common::{LanpathError, LanpathResult};

use super::BusinessService;

impl BusinessService {
    pub(crate) fn profile_store(&self) -> ProfileStoreV2 {
        ProfileStoreV2::new(self.root.clone())
    }

    pub(crate) fn profiles(&self) -> LanpathResult<Vec<Profile>> {
        self.profile_store().load()
    }

    pub(crate) fn save_profile(
        &self,
        id: Option<uuid::Uuid>,
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
        let mut profiles = self.profile_store().load()?;
        if profiles
            .iter()
            .any(|candidate| Some(candidate.id) != id && candidate.name == name)
        {
            return Err(LanpathError::Validation(format!(
                "覆写模板名称「{name}」已存在"
            )));
        }
        let profile = Profile {
            id: id.unwrap_or_else(uuid::Uuid::new_v4),
            name,
            yaml_override,
            js_override,
            yaml_url: normalize_optional_url(yaml_url),
            js_url: normalize_optional_url(js_url),
        };
        match id {
            Some(id) => {
                let target = profiles
                    .iter_mut()
                    .find(|candidate| candidate.id == id)
                    .ok_or_else(|| LanpathError::Client("模板不存在".into()))?;
                *target = profile.clone();
            }
            None => profiles.push(profile.clone()),
        }
        self.profile_store().save(&profiles)?;
        Ok(profile)
    }

    pub(crate) fn remove_profile(&self, id: uuid::Uuid) -> LanpathResult<()> {
        if self
            .subscriptions()?
            .iter()
            .any(|subscription| subscription.profile_id == Some(id))
        {
            return Err(LanpathError::Client(
                "仍有订阅关联此覆写模板，请先取消关联".into(),
            ));
        }
        self.profile_store().remove(id)
    }

    pub(crate) fn validate_profile_reference(&self, id: Option<uuid::Uuid>) -> LanpathResult<()> {
        let Some(id) = id else {
            return Ok(());
        };
        if self
            .profile_store()
            .load()?
            .iter()
            .any(|profile| profile.id == id)
        {
            Ok(())
        } else {
            Err(LanpathError::Client("覆写模板不存在，请重新选择".into()))
        }
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
