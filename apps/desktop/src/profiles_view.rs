//! 覆写模板页：NSH 桌面独立数据根上的真实模板列表。模板保存共享
//! `lanpath-client::ProfileStoreV2`（本根 `profiles.json`）；YAML/JS 本地
//! 覆写经共享校验（空允许），远程覆写地址可选、规范化并共享校验后保存，
//! 仅在后续配置合成时解析——本页不下载、不预览、不显示“已生效”，
//! 不启动核心、系统代理或 TUN。删除被订阅引用的模板会被拒绝并显示真实
//! 错误。列表/编辑/删除与订阅业务共用数据根的同一把业务锁。

mod editor;
mod rows;

use lanpath_client::Profile;
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use uuid::Uuid;

use crate::profiles::Profiles;

/// 编辑草稿：与列表数据分离，保存失败保留草稿并显示真实错误；
/// 取消时有未保存修改需先确认。`open` 区分弹窗开合，`id` 为 `None`
/// 表示新建。
#[derive(Clone)]
pub(super) struct ProfileEditState {
    open: Signal<bool>,
    id: Signal<Option<Uuid>>,
    name: Signal<String>,
    yaml_override: Signal<String>,
    js_override: Signal<String>,
    yaml_url: Signal<String>,
    js_url: Signal<String>,
    dirty: Signal<bool>,
    error: Signal<Option<String>>,
    confirm_discard: Signal<bool>,
}

impl ProfileEditState {
    fn new() -> Self {
        Self {
            open: signal(false),
            id: signal(None),
            name: signal(String::new()),
            yaml_override: signal(String::new()),
            js_override: signal(String::new()),
            yaml_url: signal(String::new()),
            js_url: signal(String::new()),
            dirty: signal(false),
            error: signal(None),
            confirm_discard: signal(false),
        }
    }

    fn open_new(&self) {
        self.id.set(None);
        self.name.set(String::new());
        self.yaml_override.set(String::new());
        self.js_override.set(String::new());
        self.yaml_url.set(String::new());
        self.js_url.set(String::new());
        self.dirty.set(false);
        self.error.set(None);
        self.confirm_discard.set(false);
        // 最后才打开弹窗：when(open) 同步重建表单时所有字段必须已就位。
        self.open.set(true);
    }

    fn open_edit(&self, profile: &Profile) {
        self.id.set(Some(profile.id));
        self.name.set(profile.name.clone());
        self.yaml_override.set(profile.yaml_override.clone());
        self.js_override.set(profile.js_override.clone());
        self.yaml_url
            .set(profile.yaml_url.clone().unwrap_or_default());
        self.js_url.set(profile.js_url.clone().unwrap_or_default());
        self.dirty.set(false);
        self.error.set(None);
        self.confirm_discard.set(false);
        // 最后才打开弹窗：when(open) 同步重建表单时所有字段必须已就位。
        self.open.set(true);
    }

    fn close(&self) {
        self.open.set(false);
        self.id.set(None);
        self.dirty.set(false);
        self.error.set(None);
        self.confirm_discard.set(false);
    }

    /// 取消保护：有未保存修改时先要求确认。
    fn request_cancel(&self) {
        if self.dirty.get() {
            self.confirm_discard.set(true);
        } else {
            self.close();
        }
    }
}

/// 覆写页状态：全部为句柄（Signal/Arc），克隆廉价。
#[derive(Clone)]
pub(super) struct ProfilesPageState {
    service: Option<Profiles>,
    list: Signal<Vec<Profile>>,
    list_error: Signal<Option<String>>,
    notice: Signal<Option<String>>,
    pending_remove: Signal<Option<Profile>>,
    edit: ProfileEditState,
}

impl ProfilesPageState {
    fn new() -> Self {
        match Profiles::open() {
            Ok(service) => {
                let state = Self {
                    service: Some(service),
                    list: signal(Vec::new()),
                    list_error: signal(None),
                    notice: signal(None),
                    pending_remove: signal(None),
                    edit: ProfileEditState::new(),
                };
                state.reload();
                state
            }
            Err(error) => Self {
                service: None,
                list: signal(Vec::new()),
                list_error: signal(Some(format!("打开数据根失败:{error}"))),
                notice: signal(None),
                pending_remove: signal(None),
                edit: ProfileEditState::new(),
            },
        }
    }

    /// 从磁盘重读模板列表；任何失败都显示真实错误，不伪造成功。
    fn reload(&self) {
        let Some(service) = self.service.clone() else {
            return;
        };
        match service.load() {
            Ok(profiles) => self.list.set(profiles),
            Err(error) => self
                .list_error
                .set(Some(format!("加载覆写模板失败:{error}"))),
        }
    }

    fn post_notice(&self, text: String) {
        self.list_error.set(None);
        self.notice.set(Some(text));
    }

    fn post_error(&self, text: String) {
        self.notice.set(None);
        self.list_error.set(Some(text));
    }
}

pub(super) fn profiles_page() -> impl IntoElement {
    let state = ProfilesPageState::new();
    let create_state = state.clone();
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(text("覆写").text_size(px(24.0)))
        .child(
            text("真实覆写模板列表 · 保存到 NSH 桌面独立数据根 · 远程覆写地址仅在配置合成时解析，本页不下载/预览，不启动核心/系统代理")
                .text_color(move || theme().muted_foreground),
        )
        .child(
            button()
                .px_4()
                .py_2()
                .rounded(move || theme().radius)
                .items_center()
                .bg(move || theme().primary)
                .on_press(move || create_state.edit.open_new())
                .child(text("新建覆写模板").text_color(move || theme().primary_foreground)),
        )
        .child(rows::profile_list(state.clone()))
        .child(when(
            move || state.edit.open.get(),
            move || editor::profile_editor_dialog(state.clone()),
        ))
}

/// 保存编辑：校验失败保留草稿并显示真实错误（业务层已保证非法输入不落盘）。
pub(super) fn save_edit(state: &ProfilesPageState) {
    let Some(service) = state.service.clone() else {
        state.edit.error.set(Some("打开数据根失败".to_owned()));
        return;
    };
    let id = state.edit.id.get();
    let name = state.edit.name.get();
    let yaml = state.edit.yaml_override.get();
    let js = state.edit.js_override.get();
    let yaml_url = optional_text(&state.edit.yaml_url.get());
    let js_url = optional_text(&state.edit.js_url.get());
    match service.save(id, &name, yaml, js, yaml_url, js_url) {
        Ok(_) => {
            state.edit.close();
            state.reload();
            state.post_notice("覆写模板已校验并保存".to_owned());
        }
        Err(error) => state
            .edit
            .error
            .set(Some(format!("保存覆写模板失败:{error}"))),
    }
}

/// 空白输入归一化为 `None`（与业务层 `normalize_optional_url` 一致，
/// 业务层保存时会再次规范化）。
fn optional_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// 订阅页/模板行共用的模板名解析：找不到（被外部删除或文件损坏）时
/// 返回 `None`，由展示方给出“引用失效”的真实提示，不静默改绑。
pub(super) fn profile_name(profiles: &[Profile], id: Option<Uuid>) -> Option<String> {
    id.and_then(|id| {
        profiles
            .iter()
            .find(|profile| profile.id == id)
            .map(|profile| profile.name.clone())
    })
}
