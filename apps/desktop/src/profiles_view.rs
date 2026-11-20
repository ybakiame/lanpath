//! 首页覆写脚本面板。脚本保存共享 `lanpath-client::ProfileStoreV2`
//! （本根 `profiles.json`）；本地 TypeScript 经共享校验后保存，
//! 仅在后续配置合成时解析——本页不下载、不预览、不显示“已生效”，
//! 不启动核心、系统代理或 TUN。删除被订阅引用的模板会被拒绝并显示真实
//! 错误。列表/编辑/删除与订阅业务共用数据根的同一把业务锁。

mod editor;
mod rows;

use crate::ui::{ButtonVariant, button};
use lanpath_client::Profile;
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
    js_override: Signal<String>,
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
            js_override: signal(String::new()),
            dirty: signal(false),
            error: signal(None),
            confirm_discard: signal(false),
        }
    }

    fn open_new(&self) {
        self.id.set(None);
        self.name.set(String::new());
        self.js_override
            .set(crate::ui::code_editor::DEFAULT_SCRIPT.to_owned());
        self.dirty.set(false);
        self.error.set(None);
        self.confirm_discard.set(false);
        // 最后才打开弹窗：when(open) 同步重建表单时所有字段必须已就位。
        self.open.set(true);
    }

    fn open_edit(&self, profile: &Profile) {
        self.id.set(Some(profile.id));
        self.name.set(profile.name.clone());
        self.js_override.set(profile.js_override.clone());
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
    let dirty = state.edit.dirty;
    crate::navigation::protect(move || {
        if !crate::workspace_panels::is_open() {
            return crate::navigation::LeaveState::Clean;
        }
        if dirty.get() {
            crate::navigation::LeaveState::Dirty
        } else {
            crate::navigation::LeaveState::Clean
        }
    });
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(
            text(|| t!("profiles.subtitle"))
                .text_size(px(12.0))
                .text_color(move || nsh_base::theme().muted_foreground),
        )
        .child(
            button(ButtonVariant::Outline)
                .test_id("profiles-create")
                .label(|| t!("profiles.create"))
                .prefix(|| {
                    crate::ui::icon::icon(crate::ui::icon::Icon::Plus, crate::ui::icon::Tone::Muted)
                })
                .on_press(move || create_state.edit.open_new()),
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
    let js = state.edit.js_override.get();
    match service.save(id, &name, js, None) {
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
