//! 请求设置页：设置下一次订阅刷新与远程资源请求的网络请求策略。
//! 四种代理模式（直连 / 跟随系统 / 核心代理 / 自定义）单选；自定义代理
//! URL 与 GitHub 镜像前缀草稿可编辑并保存，重开/重置恢复已保存值；
//! 非法自定义地址拒绝保存并显示共享层脱敏错误（不含凭据/完整 URL），
//! 草稿保留。核心代理只指向已保存的 mixed_port——本切片不启动核心，
//! 核心未运行时请求会真实失败；本页不做连接测试、不提供系统代理/TUN
//! 开关，不显示任何“连接成功”假反馈。

use lanpath_client::RequestProxyMode;
use nsh_base::theme;
use nsh_core::prelude::*;

use crate::request_settings::{RequestSettings, RequestSettingsDocument};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, section_title, surface};

/// 页面的四个代理模式（顺序即单选列表顺序）。
const MODES: [RequestProxyMode; 4] = [
    RequestProxyMode::None,
    RequestProxyMode::System,
    RequestProxyMode::Core,
    RequestProxyMode::Custom,
];

fn mode_test_id(mode: RequestProxyMode) -> &'static str {
    match mode {
        RequestProxyMode::None => "request-mode-none",
        RequestProxyMode::System => "request-mode-system",
        RequestProxyMode::Core => "request-mode-core",
        RequestProxyMode::Custom => "request-mode-custom",
    }
}

/// 页状态：草稿（mode/custom_url/github_prefix）与已保存文档分离；
/// 保存成功后草稿回写为已保存值（不会保存后仍显示旧草稿）；重置/取消
/// 恢复已保存值，有未保存修改需先确认。
#[derive(Clone)]
struct RequestSettingsState {
    service: Option<RequestSettings>,
    mode: Signal<RequestProxyMode>,
    custom_url: Signal<String>,
    github_prefix: Signal<String>,
    saved: Signal<Option<RequestSettingsDocument>>,
    dirty: Signal<bool>,
    error: Signal<Option<String>>,
    notice: Signal<Option<String>>,
    confirm_discard: Signal<bool>,
}

impl RequestSettingsState {
    fn new() -> Self {
        match RequestSettings::open() {
            Ok(service) => {
                // 先读再建状态：打开失败的真实错误也要上屏，不伪造默认。
                let loaded = service.load();
                let state = Self {
                    service: Some(service),
                    mode: signal(RequestProxyMode::None),
                    custom_url: signal(String::new()),
                    github_prefix: signal(String::new()),
                    saved: signal(None),
                    dirty: signal(false),
                    error: signal(None),
                    notice: signal(None),
                    confirm_discard: signal(false),
                };
                state.apply_saved(loaded);
                state
            }
            Err(error) => Self {
                service: None,
                mode: signal(RequestProxyMode::None),
                custom_url: signal(String::new()),
                github_prefix: signal(String::new()),
                saved: signal(None),
                dirty: signal(false),
                error: signal(Some(format!("打开数据根失败:{error}"))),
                notice: signal(None),
                confirm_discard: signal(false),
            },
        }
    }

    /// 把读取/保存结果落到草稿与已保存快照；读取失败显示真实错误。
    fn apply_saved(&self, result: lanpath_common::LanpathResult<RequestSettingsDocument>) {
        match result {
            Ok(document) => {
                self.mode.set(document.mode());
                self.custom_url.set(document.custom_url().to_owned());
                self.github_prefix.set(document.github_prefix().to_owned());
                self.saved.set(Some(document));
                self.dirty.set(false);
                self.error.set(None);
            }
            Err(error) => self.error.set(Some(format!("加载请求设置失败:{error}"))),
        }
    }

    fn draft_document(&self) -> RequestSettingsDocument {
        RequestSettingsDocument::new(
            self.mode.get(),
            self.custom_url.get(),
            self.github_prefix.get(),
        )
    }

    fn update_dirty(&self) {
        let dirty = self.saved.get().is_none_or(|saved| {
            saved.mode() != self.mode.get()
                || saved.custom_url() != self.custom_url.get()
                || saved.github_prefix() != self.github_prefix.get()
        });
        self.dirty.set(dirty);
    }

    fn save(&self) {
        let Some(service) = self.service.clone() else {
            self.error.set(Some("打开数据根失败".to_owned()));
            return;
        };
        match service.save(&self.draft_document()) {
            Ok(saved) => {
                self.apply_saved(Ok(saved));
                self.notice.set(Some(
                    "请求设置已保存；下一次订阅刷新与远程资源请求将使用新策略".to_owned(),
                ));
            }
            Err(error) => {
                // 校验失败：共享层错误不含凭据/完整 URL；草稿保留待修改。
                self.notice.set(None);
                self.error.set(Some(format!("保存请求设置失败:{error}")));
            }
        }
    }

    /// 重置入口：有未保存修改先要求确认，否则直接从盘重载。
    fn request_reset(&self) {
        if self.dirty.get() {
            self.confirm_discard.set(true);
        } else {
            self.reload();
        }
    }

    /// 放弃未保存修改并重载已保存值。
    fn reload(&self) {
        self.confirm_discard.set(false);
        let Some(service) = self.service.clone() else {
            self.error.set(Some("打开数据根失败".to_owned()));
            return;
        };
        self.apply_saved(service.load());
        self.notice.set(Some("已重新载入保存的请求设置".to_owned()));
    }
}

pub(crate) fn request_settings_page() -> impl IntoElement {
    let state = RequestSettingsState::new();
    let dirty = state.dirty;
    crate::navigation::protect(move || {
        if dirty.get() {
            crate::navigation::LeaveState::Dirty
        } else {
            crate::navigation::LeaveState::Clean
        }
    });
    let save = state.clone();
    let reset = state.clone();
    let mode = state.mode;
    let url = state.custom_url;
    let prefix = state.github_prefix;
    let url_dirty = state.clone();
    let prefix_dirty = state.clone();
    let radio_state = state.clone();
    let error = state.error;
    let notice = state.notice;
    let confirm = state.confirm_discard;
    let discard = state.clone();
    scroll().flex_1().min_h(px(0.0)).child(
        view()
            .flex_col()
            .gap_5()
            .max_w(px(760.0))
            .child(crate::ui::page_header(
                || t!("nav.request_settings"),
                || t!("request.subtitle"),
            ))
            .child(
                surface(SurfaceLevel::Card)
                    .p_5()
                    .flex_col()
                    .gap_3()
                    .child(section_title(|| t!("request.mode")))
                    .child(crate::ui::select::radio_group(
                        || t!("request.mode"),
                        view().flex_col().gap_1().children(
                            MODES
                                .iter()
                                .map(|mode| mode_radio(radio_state.clone(), *mode)),
                        ),
                    ))
                    .child(
                        text(|| t!("request.core_hint"))
                            .text_size(px(12.5))
                            .text_color(move || theme().muted_foreground),
                    ),
            )
            .child(
                surface(SurfaceLevel::Card)
                    .p_5()
                    .flex_col()
                    .gap_5()
                    .child(
                        crate::ui::field()
                            .label(|| t!("request.url"))
                            .hint(|| t!("request.url_hint"))
                            .control(
                                crate::ui::input(url)
                                    .test_id("request-custom-url")
                                    .accessible_name(|| t!("request.url"))
                                    .readonly(move || mode.get() != RequestProxyMode::Custom)
                                    .on_change(move |v| {
                                        url.set(v);
                                        url_dirty.update_dirty();
                                    }),
                            ),
                    )
                    .child(
                        crate::ui::field()
                            .label(|| t!("request.mirror"))
                            .hint(|| t!("request.mirror_hint"))
                            .control(
                                crate::ui::input(prefix)
                                    .test_id("request-github-prefix")
                                    .accessible_name(|| t!("request.mirror"))
                                    .on_change(move |v| {
                                        prefix.set(v);
                                        prefix_dirty.update_dirty();
                                    }),
                            ),
                    ),
            )
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(
                        crate::ui::button(crate::ui::ButtonVariant::Primary)
                            .label(|| t!("common.save"))
                            .on_press(move || save.save()),
                    )
                    .child(
                        crate::ui::button(crate::ui::ButtonVariant::Outline)
                            .label(|| t!("request.reset"))
                            .on_press(move || reset.request_reset()),
                    )
                    .child(when(
                        move || dirty.get(),
                        || {
                            crate::ui::badge(crate::ui::BadgeVariant::Warning, || {
                                t!("navigation.dirty_title")
                            })
                        },
                    )),
            )
            .child(when(
                move || error.get().is_some(),
                move || {
                    alert_banner(
                        AlertTone::Danger,
                        text(move || error.get().unwrap_or_default())
                            .text_size(px(12.5))
                            .text_color(move || theme().destructive)
                            .selectable(true),
                    )
                },
            ))
            .child(when(
                move || notice.get().is_some(),
                move || {
                    alert_banner(
                        AlertTone::Success,
                        text(move || notice.get().unwrap_or_default())
                            .text_size(px(12.5))
                            .text_color(move || theme().foreground),
                    )
                },
            ))
            .child(
                crate::ui::confirm_dialog(move || confirm.get())
                    .title(|| t!("request.discard_title"))
                    .description(|| t!("navigation.dirty_description"))
                    .confirm_label(|| t!("subscription.discard_confirm"))
                    .cancel_label(|| t!("navigation.keep"))
                    .on_cancel(move || confirm.set(false))
                    .on_confirm(move || discard.reload()),
            ),
    )
}

fn mode_radio(state: RequestSettingsState, target: RequestProxyMode) -> impl IntoElement {
    let checked = move || state.mode.get() == target;
    let key = match target {
        RequestProxyMode::None => "request.direct",
        RequestProxyMode::System => "request.system",
        RequestProxyMode::Core => "request.core",
        RequestProxyMode::Custom => "request.custom",
    };
    crate::ui::select::radio_row(
        checked,
        move || t!(key),
        mode_test_id(target),
        move || {
            state.mode.set(target);
            state.update_dirty();
        },
    )
}
