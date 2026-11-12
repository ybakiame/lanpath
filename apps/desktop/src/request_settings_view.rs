//! 请求设置页：设置下一次订阅刷新与远程资源请求的网络请求策略。
//! 四种代理模式（直连 / 跟随系统 / 核心代理 / 自定义）单选；自定义代理
//! URL 与 GitHub 镜像前缀草稿可编辑并保存，重开/重置恢复已保存值；
//! 非法自定义地址拒绝保存并显示共享层脱敏错误（不含凭据/完整 URL），
//! 草稿保留。核心代理只指向已保存的 mixed_port——本切片不启动核心，
//! 核心未运行时请求会真实失败；本页不做连接测试、不提供系统代理/TUN
//! 开关，不显示任何“连接成功”假反馈。

use lanpath_client::RequestProxyMode;
use nsh_base::{button, dialog, input, radio, radio_group, theme};
use nsh_core::prelude::*;

use crate::request_settings::{RequestSettings, RequestSettingsDocument};

/// 页面的四个代理模式（顺序即单选列表顺序）。
const MODES: [RequestProxyMode; 4] = [
    RequestProxyMode::None,
    RequestProxyMode::System,
    RequestProxyMode::Core,
    RequestProxyMode::Custom,
];

fn mode_label(mode: RequestProxyMode) -> &'static str {
    match mode {
        RequestProxyMode::None => "直连（不使用代理）",
        RequestProxyMode::System => "跟随系统代理",
        RequestProxyMode::Core => "核心代理（指向已保存的 mixed_port）",
        RequestProxyMode::Custom => "自定义 HTTP(S) / SOCKS5 代理",
    }
}

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
    let save_state = state.clone();
    let reset_state = state.clone();
    let summary_state = state.clone();
    let radio_state = state.clone();
    let url_state = state.clone();
    let prefix_state = state.clone();
    let error_state = state.clone();
    let notice_state = state.clone();
    let dirty_state = state.clone();
    let dialog_state = state;
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(text("请求设置").text_size(px(24.0)))
        .child(
            text("设置下一次订阅刷新与远程资源请求的网络路径 · 保存后立即生效 · 本页不启动核心/系统代理/TUN")
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text("请求代理模式").text_size(px(15.0)))
                .child(
                    radio_group().flex_col().gap_2().children(
                        MODES
                            .iter()
                            .map(|mode| mode_radio(radio_state.clone(), *mode)),
                    ),
                )
                .child(
                    text("核心代理只指向已保存的 mixed_port；本切片不启动核心，核心未运行时请求会真实失败。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            view()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text("自定义代理 URL").text_size(px(15.0)))
                .child(proxy_url_input(url_state))
                .child(
                    text("允许 http / https / socks5 / socks5h 地址（可含认证）；错误提示不含凭据。非自定义模式下地址仍保存，切回后继续生效。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            view()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text("GitHub 镜像前缀").text_size(px(15.0)))
                .child(prefix_input(prefix_state))
                .child(
                    text("留空直连 GitHub；仅按共享规则把前缀拼到 GitHub 资源地址前（保存原样保留，使用时修剪空白），本页不做连接测试。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(text(move || {
            summary_state
                .saved
                .get()
                .map(|saved| {
                    format!(
                        "已保存：{} · GitHub 镜像{}",
                        mode_label(saved.mode()),
                        if saved.github_prefix().trim().is_empty() {
                            "未配置"
                        } else {
                            "已配置"
                        },
                    )
                })
                .unwrap_or_else(|| "尚未载入保存的请求设置".to_owned())
        }))
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(
                    button()
                        .px_4()
                        .py_2()
                        .rounded(move || theme().radius)
                        .bg(move || theme().primary)
                        .on_press(move || save_state.save())
                        .child(text("保存").text_color(move || theme().primary_foreground)),
                )
                .child(
                    button()
                        .px_4()
                        .py_2()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || reset_state.request_reset())
                        .child(text("重置")),
                ),
        )
        .child(
            text(move || error_state.error.get().unwrap_or_default())
                .text_color(move || theme().destructive),
        )
        .child(
            text(move || notice_state.notice.get().unwrap_or_default())
                .text_color(move || theme().muted_foreground),
        )
        .child(
            text(move || {
                if dirty_state.dirty.get() {
                    "有未保存的修改"
                } else {
                    "设置与已保存的请求策略一致"
                }
            })
            .text_size(px(12.0))
            .text_color(move || theme().muted_foreground),
        )
        .child(when(
            move || dialog_state.confirm_discard.get(),
            move || discard_dialog(dialog_state.clone()),
        ))
}

fn mode_radio(state: RequestSettingsState, target: RequestProxyMode) -> impl IntoElement {
    let checked = move || state.mode.get() == target;
    radio()
        .checked(checked)
        .on_change(move |_: &bool| {
            state.mode.set(target);
            state.dirty.set(true);
        })
        .child(
            view()
                .test_id(mode_test_id(target))
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    view()
                        .size(px(16.0))
                        .rounded_full()
                        .border(px(1.0))
                        .border_color(move || theme().muted_foreground)
                        .bg(move || {
                            if checked() {
                                theme().primary
                            } else {
                                theme().card
                            }
                        }),
                )
                .child(text(mode_label(target))),
        )
}

/// 自定义代理 URL 输入：非 Custom 模式只读展示（地址不清空，切回可继续
/// 编辑）；草稿改动标记未保存。
fn proxy_url_input(state: RequestSettingsState) -> impl IntoElement {
    let mode = state.mode;
    let change_state = state.clone();
    input()
        .test_id("request-custom-url")
        .value(state.custom_url)
        .placeholder("http://127.0.0.1:8080 或 socks5://user:pass@host:1080")
        .auto_correct(false)
        .readonly(move || mode.get() != RequestProxyMode::Custom)
        .on_change(move |text: &str| {
            change_state.custom_url.set(text.to_owned());
            change_state.dirty.set(true);
        })
        .p_2()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().input)
        .bg(move || theme().background)
}

fn prefix_input(state: RequestSettingsState) -> impl IntoElement {
    let change_state = state.clone();
    input()
        .test_id("request-github-prefix")
        .value(state.github_prefix)
        .placeholder("https://gh-proxy.com（可选，仅 GitHub 资源生效）")
        .auto_correct(false)
        .on_change(move |text: &str| {
            change_state.github_prefix.set(text.to_owned());
            change_state.dirty.set(true);
        })
        .p_2()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().input)
        .bg(move || theme().background)
}

/// 放弃未保存修改的确认弹窗：继续编辑保留草稿；放弃修改恢复已保存值。
fn discard_dialog(state: RequestSettingsState) -> AnyElement {
    let cancel_state = state.clone();
    let keep_state = state.clone();
    let discard_state = state;
    dialog()
        .on_cancel(move || cancel_state.confirm_discard.set(false))
        .child(
            view()
                .flex_col()
                .gap_3()
                .w(px(420.0))
                .p_4()
                .rounded(move || theme().radius)
                .bg(move || theme().card)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text("放弃未保存的请求设置修改？"))
                .child(
                    text("草稿尚未保存，放弃后将恢复为磁盘上已保存的请求设置。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .justify_end()
                        .child(
                            button()
                                .px_3()
                                .py_1()
                                .rounded(move || theme().radius)
                                .border(px(1.0))
                                .border_color(move || theme().border)
                                .on_press(move || keep_state.confirm_discard.set(false))
                                .child(text("继续编辑")),
                        )
                        .child(
                            button()
                                .px_3()
                                .py_1()
                                .rounded(move || theme().radius)
                                .bg(move || theme().destructive)
                                .on_press(move || discard_state.reload())
                                .child(
                                    text("放弃修改").text_color(move || theme().primary_foreground),
                                ),
                        ),
                ),
        )
        .into_element()
}
