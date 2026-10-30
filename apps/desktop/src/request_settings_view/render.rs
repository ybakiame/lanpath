use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::{Button, ButtonVariants as _},
    input::Input,
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_client::RequestProxyMode;

use super::{RequestSettingsView, mode_label};

const MODES: [RequestProxyMode; 4] = [
    RequestProxyMode::None,
    RequestProxyMode::System,
    RequestProxyMode::Core,
    RequestProxyMode::Custom,
];

impl Render for RequestSettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editing_disabled = self.editing_disabled();
        let status = if self.probing {
            format!(
                "正在测试已保存的快照 #{}，不会使用未保存的修改…",
                self.probe_sequence
            )
        } else if let Some(error) = &self.error {
            error.clone()
        } else if let Some(notice) = &self.notice {
            notice.clone()
        } else if self.closing {
            "正在关闭，等待已接受的连接测试完成".into()
        } else if self.locked {
            "核心运行或其他配置操作期间不可修改；仍可测试已保存的请求策略".into()
        } else if self.dirty() {
            "有未保存的修改；连接测试仍使用磁盘中已保存的设置".into()
        } else {
            "设置与已保存的请求策略一致".into()
        };
        let saved_summary = self
            .saved
            .as_ref()
            .map(|settings| {
                format!(
                    "页面已载入的保存设置：{} · GitHub 镜像{}",
                    mode_label(settings.mode()),
                    if settings.github_prefix().trim().is_empty() {
                        "未配置"
                    } else {
                        "已配置"
                    },
                )
            })
            .unwrap_or_else(|| "尚未载入保存设置；请修复读取错误后重新载入".into());
        let probe_summary = self.probe_result.as_ref().map(|probe| {
            let outcome = match &probe.result {
                Ok(elapsed) => format!("连接成功（{elapsed} ms）"),
                Err(error) => format!("连接失败：{error}"),
            };
            let current = self.saved.as_ref() == Some(&probe.settings);
            format!(
                "测试快照 #{}：{} · GitHub 镜像{} · {outcome}{}",
                probe.sequence,
                mode_label(probe.settings.mode()),
                if probe.settings.github_prefix().trim().is_empty() {
                    "未配置"
                } else {
                    "已配置"
                },
                if current {
                    ""
                } else {
                    "；此结果不对应页面当前载入的保存设置，请重新测试"
                },
            )
        });

        div().flex().flex_col().flex_1().min_h_0().min_w_0()
            .child(div().flex_1().min_h_0().min_w_0().overflow_y_scrollbar()
                .child(div().flex().flex_col().flex_shrink_0().min_w_0().w_full()
                    .max_w(gpui_kit::rems(64.0)).gap_4().p_1().pb_4()
                .child(div().text_lg().child("网络请求"))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("仅影响 Lanpath 拉取订阅、脚本和 GitHub 资源，不会修改系统代理、TUN 或证书。"))
                .child(div().flex().flex_col().gap_3()
                    .child("请求代理模式")
                    .child(RadioGroup::vertical("request-mode")
                        .selected_index(MODES.iter().position(|mode| *mode == self.mode))
                        .disabled(editing_disabled)
                        .child(Radio::new("request-mode-none")
                            .accessibility_id("request-mode-none").label("不使用代理（直连）"))
                        .child(Radio::new("request-mode-system")
                            .accessibility_id("request-mode-system").label("使用系统代理"))
                        .child(Radio::new("request-mode-core")
                            .accessibility_id("request-mode-core").label("使用实验核心代理（核心须已启动）"))
                        .child(Radio::new("request-mode-custom")
                            .accessibility_id("request-mode-custom").label("使用自定义 HTTP(S) / SOCKS5 代理"))
                        .on_change(cx.listener(|this, ix: &usize, _, cx| {
                            if this.editing_disabled() { return; }
                            if let Some(mode) = MODES.get(*ix) {
                                this.mode = *mode;
                                this.sync_readonly(cx);
                                this.draft_changed(cx);
                            }
                        }))))
                .child(div().flex().flex_col().gap_2()
                    .child("自定义代理 URL")
                    .child(Input::new(&self.custom_url)
                        .id("request-custom-url").accessibility_id("request-custom-url")
                        .aria_label("自定义请求代理 URL（密码遮蔽）")
                        .disabled(editing_disabled || self.mode != RequestProxyMode::Custom))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("整个地址均遮蔽显示；认证信息以明文保存到实验 client.json。切换模式不会清空地址。")))
                .child(div().flex().flex_col().gap_2()
                    .child("GitHub 镜像前缀")
                    .child(Input::new(&self.github_prefix)
                        .id("request-github-prefix").accessibility_id("request-github-prefix")
                        .aria_label("GitHub 镜像前缀").disabled(editing_disabled))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("留空不使用镜像；仅按现有规则将前缀拼接到 GitHub 资源 URL 前。")))
                .child(div().id("request-saved-policy").text_sm()
                    .text_color(cx.theme().muted_foreground).child(saved_summary))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("连接测试使用接受测试时的已保存快照，请求 https://api.github.com/zen；每次请求超时 10 秒，沿用真实资源拉取的重试规则。"))
                .children(probe_summary.map(|summary| div().id("request-probe-result")
                    .role(Role::Status).aria_label(summary.clone()).text_sm().child(summary)))))
            .child(div().flex().flex_col().flex_shrink_0().min_w_0().gap_2()
                .border_t_1().border_color(cx.theme().border).pt_3().px_1().pb_1()
                .child(div().flex().items_center().flex_wrap().gap_2()
                    .child(Button::new("request-save").primary().label("保存")
                        .disabled(editing_disabled || !self.dirty())
                        .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))))
                    .child(Button::new("request-reload").label(if self.dirty() { "重新载入…" } else { "重新载入" })
                        .disabled(editing_disabled)
                        .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))))
                    .child(Button::new("request-probe").label("测试已保存的 GitHub 连接")
                        .disabled(self.closing || self.busy() || self.service.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.probe(cx)))))
                .child(div().id("request-draft-status").role(Role::Status)
                    .aria_label(status.clone()).text_sm().min_w_0().max_h_16().overflow_y_scrollbar()
                    .text_color(if self.error.is_some() { cx.theme().danger } else { cx.theme().muted_foreground })
                    .child(status)))
    }
}
