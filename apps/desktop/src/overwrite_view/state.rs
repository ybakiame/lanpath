//! `overwrite_view` 状态模型：维护 5 个切片的草稿/已保存/定制标记以及
//! `app.toml` 受管入站端口与控制 API 草稿。

use lanpath_client::singbox_overwrite::SingboxSliceKind;
use lanpath_common::CoreType;
use nsh_core::prelude::*;

use crate::singbox_overwrite::{ManagedBindings, SingboxOverwriteService};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditMode {
    Form,
    Json,
}

#[derive(Clone)]
pub(super) struct OverwriteState {
    pub(super) service: Option<SingboxOverwriteService>,
    pub(super) active_slice: Signal<SingboxSliceKind>,
    pub(super) edit_mode: Signal<EditMode>,
    pub(super) drafts: Signal<Vec<String>>,
    pub(super) saved_raws: Signal<Vec<String>>,
    pub(super) customized: Signal<Vec<bool>>,
    pub(super) mixed_port: Signal<String>,
    pub(super) clash_api_enabled: Signal<bool>,
    pub(super) clash_api_port: Signal<String>,
    pub(super) clash_api_secret: Signal<String>,
    pub(super) saved_bindings: Signal<Option<ManagedBindings>>,
    pub(super) error: Signal<Option<String>>,
    pub(super) notice: Signal<Option<String>>,
    pub(super) confirm_reset: Signal<bool>,
}

impl OverwriteState {
    pub(super) fn new() -> Self {
        match SingboxOverwriteService::open() {
            Ok(service) => {
                let mut error = None;
                let (drafts, saved_raws, customized) = match service.load_all_slices() {
                    Ok(items) => {
                        let raws: Vec<String> = items.iter().map(|s| s.raw.clone()).collect();
                        let flags: Vec<bool> = items.iter().map(|s| s.customized).collect();
                        (raws.clone(), raws, flags)
                    }
                    Err(err) => {
                        error = Some(format!("读取配置切片失败：{err}"));
                        let defaults: Vec<String> = SingboxSliceKind::ALL
                            .iter()
                            .map(|k| lanpath_client::singbox_overwrite::default_slice_pretty(*k))
                            .collect();
                        (defaults.clone(), defaults, vec![false; 5])
                    }
                };
                let bindings = match service.load_bindings() {
                    Ok(b) => Some(b),
                    Err(err) => {
                        if error.is_none() {
                            error = Some(format!("读取端口与控制 API 设置失败：{err}"));
                        }
                        None
                    }
                };
                let (mixed_port, clash_api_enabled, clash_api_port, clash_api_secret) =
                    match &bindings {
                        Some(b) => (
                            b.mixed_port.to_string(),
                            b.clash_api_enabled,
                            b.clash_api_port.to_string(),
                            b.clash_api_secret.clone(),
                        ),
                        None => (
                            crate::core_runtime::DEFAULT_MIXED_PORT.to_string(),
                            true,
                            crate::core_runtime::DEFAULT_CLASH_API_PORT.to_string(),
                            String::new(),
                        ),
                    };
                Self {
                    service: Some(service),
                    active_slice: signal(SingboxSliceKind::Log),
                    edit_mode: signal(EditMode::Form),
                    drafts: signal(drafts),
                    saved_raws: signal(saved_raws),
                    customized: signal(customized),
                    mixed_port: signal(mixed_port),
                    clash_api_enabled: signal(clash_api_enabled),
                    clash_api_port: signal(clash_api_port),
                    clash_api_secret: signal(clash_api_secret),
                    saved_bindings: signal(bindings),
                    error: signal(error),
                    notice: signal(None),
                    confirm_reset: signal(false),
                }
            }
            Err(err) => {
                let defaults: Vec<String> = SingboxSliceKind::ALL
                    .iter()
                    .map(|k| lanpath_client::singbox_overwrite::default_slice_pretty(*k))
                    .collect();
                Self {
                    service: None,
                    active_slice: signal(SingboxSliceKind::Log),
                    edit_mode: signal(EditMode::Form),
                    drafts: signal(defaults.clone()),
                    saved_raws: signal(defaults),
                    customized: signal(vec![false; 5]),
                    mixed_port: signal(crate::core_runtime::DEFAULT_MIXED_PORT.to_string()),
                    clash_api_enabled: signal(true),
                    clash_api_port: signal(crate::core_runtime::DEFAULT_CLASH_API_PORT.to_string()),
                    clash_api_secret: signal(String::new()),
                    saved_bindings: signal(None),
                    error: signal(Some(format!("打开数据根失败：{err}"))),
                    notice: signal(None),
                    confirm_reset: signal(false),
                }
            }
        }
    }

    pub(super) fn slice_index(kind: SingboxSliceKind) -> usize {
        match kind {
            SingboxSliceKind::Log => 0,
            SingboxSliceKind::Dns => 1,
            SingboxSliceKind::Route => 2,
            SingboxSliceKind::Outbounds => 3,
            SingboxSliceKind::Experimental => 4,
        }
    }

    pub(super) fn active_draft(&self) -> String {
        let idx = Self::slice_index(self.active_slice.get());
        self.drafts
            .with(|list| list.get(idx).cloned().unwrap_or_default())
    }

    pub(super) fn set_active_draft(&self, value: String) {
        let idx = Self::slice_index(self.active_slice.get());
        self.drafts.update(|list| {
            if let Some(slot) = list.get_mut(idx) {
                *slot = value;
            }
        });
        self.error.set(None);
        self.notice.set(None);
    }

    pub(super) fn is_active_slice_dirty(&self) -> bool {
        let idx = Self::slice_index(self.active_slice.get());
        self.drafts
            .with(|d| self.saved_raws.with(|s| d.get(idx) != s.get(idx)))
    }

    pub(super) fn is_bindings_dirty(&self) -> bool {
        let Some(saved) = self.saved_bindings.get() else {
            return false;
        };
        self.mixed_port.get().trim() != saved.mixed_port.to_string()
            || self.clash_api_enabled.get() != saved.clash_api_enabled
            || self.clash_api_port.get().trim() != saved.clash_api_port.to_string()
            || self.clash_api_secret.get() != saved.clash_api_secret
    }

    pub(super) fn is_any_dirty(&self) -> bool {
        let slices_dirty = self.drafts.with(|d| self.saved_raws.with(|s| d != s));
        slices_dirty || self.is_bindings_dirty()
    }

    pub(super) fn save_active_slice(&self) {
        let Some(service) = &self.service else {
            self.error.set(Some("打开数据根失败".into()));
            return;
        };
        let kind = self.active_slice.get();
        let raw = self.active_draft();
        match service.save_slice(kind, &raw) {
            Ok(snapshot) => {
                let idx = Self::slice_index(kind);
                self.drafts.update(|list| {
                    if let Some(slot) = list.get_mut(idx) {
                        *slot = snapshot.raw.clone();
                    }
                });
                self.saved_raws.update(|list| {
                    if let Some(slot) = list.get_mut(idx) {
                        *slot = snapshot.raw;
                    }
                });
                self.customized.update(|list| {
                    if let Some(slot) = list.get_mut(idx) {
                        *slot = snapshot.customized;
                    }
                });
                self.error.set(None);
                self.notice
                    .set(Some(t!("overwrite.saved", file = kind.file_name())));
            }
            Err(err) => {
                self.notice.set(None);
                self.error.set(Some(err.to_string()));
            }
        }
    }

    pub(super) fn format_active_slice(&self) {
        let raw = self.active_draft();
        match SingboxOverwriteService::format_slice(&raw) {
            Ok(formatted) => {
                self.set_active_draft(formatted);
                self.notice.set(Some(t!("overwrite.formatted")));
            }
            Err(err) => {
                self.notice.set(None);
                self.error.set(Some(err.to_string()));
            }
        }
    }

    pub(super) fn reset_active_slice(&self) {
        self.confirm_reset.set(false);
        let Some(service) = &self.service else {
            self.error.set(Some("打开数据根失败".into()));
            return;
        };
        let kind = self.active_slice.get();
        match service.reset_slice(kind) {
            Ok(snapshot) => {
                let idx = Self::slice_index(kind);
                self.drafts.update(|list| {
                    if let Some(slot) = list.get_mut(idx) {
                        *slot = snapshot.raw.clone();
                    }
                });
                self.saved_raws.update(|list| {
                    if let Some(slot) = list.get_mut(idx) {
                        *slot = snapshot.raw;
                    }
                });
                self.customized.update(|list| {
                    if let Some(slot) = list.get_mut(idx) {
                        *slot = false;
                    }
                });
                self.error.set(None);
                self.notice
                    .set(Some(t!("overwrite.reset_done", file = kind.file_name())));
            }
            Err(err) => {
                self.notice.set(None);
                self.error.set(Some(err.to_string()));
            }
        }
    }

    pub(super) fn save_bindings(&self) {
        let Some(service) = &self.service else {
            self.error.set(Some("打开数据根失败".into()));
            return;
        };
        let Ok(mixed_port) = self.mixed_port.get().trim().parse::<u16>() else {
            self.notice.set(None);
            self.error
                .set(Some("本地 Mixed 端口必须为 1–65535 的整数".into()));
            return;
        };
        let Ok(clash_api_port) = self.clash_api_port.get().trim().parse::<u16>() else {
            self.notice.set(None);
            self.error
                .set(Some("Clash API 端口必须为 1–65535 的整数".into()));
            return;
        };
        let core_type = self
            .saved_bindings
            .get()
            .map(|b| b.core_type)
            .unwrap_or(CoreType::SingBox);
        let candidate = ManagedBindings {
            mixed_port,
            clash_api_enabled: self.clash_api_enabled.get(),
            clash_api_port,
            clash_api_secret: self.clash_api_secret.get(),
            core_type,
        };
        match service.save_bindings(&candidate) {
            Ok(saved) => {
                self.mixed_port.set(saved.mixed_port.to_string());
                self.clash_api_enabled.set(saved.clash_api_enabled);
                self.clash_api_port.set(saved.clash_api_port.to_string());
                self.clash_api_secret.set(saved.clash_api_secret.clone());
                self.saved_bindings.set(Some(saved));
                self.error.set(None);
                self.notice.set(Some(t!("overwrite.bindings_saved")));
            }
            Err(err) => {
                self.notice.set(None);
                self.error.set(Some(err.to_string()));
            }
        }
    }
}
