use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, button::Button, checkbox::Checkbox, input::Editor,
    list::List, scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_client::mitm::{CaOwnershipStatus, CaTrustStatus};

use super::traffic_list::traffic_columns;
use super::{MitmView, Operation};

impl Render for MitmView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy();
        let editing_disabled = self.locked || busy;
        let trust = self
            .trust
            .as_ref()
            .map(|trust| {
                let label = match trust.status {
                    CaTrustStatus::Trusted => "已信任",
                    CaTrustStatus::NotTrusted => "未信任",
                    CaTrustStatus::Unknown => "未知",
                };
                if trust.detail.trim().is_empty() {
                    label.to_string()
                } else {
                    format!("{label} · {}", trust.detail)
                }
            })
            .unwrap_or_else(|| "未查询".into());
        let operation = self.operation.map(|operation| match operation {
            Operation::Save => "正在保存 MITM 设置…",
            Operation::Refresh => "正在刷新证书与运行状态…",
            Operation::InstallCa => "正在安装实验 MITM CA…",
            Operation::RemoveCa => "正在移除本应用拥有的 CA 信任…",
        });
        let status = operation
            .map(str::to_string)
            .or_else(|| self.error.clone())
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| {
                if let Some(addr) = self.mitm_addr {
                    format!("MITM 运行中 · {addr}")
                } else if self.running && self.enabled {
                    "核心运行中，但 MITM 未启动；请检查有效白名单".into()
                } else if self.enabled {
                    "已启用；保存后在下次核心启动时生效".into()
                } else {
                    "MITM 已关闭".into()
                }
            });
        let status_error = self.error.is_some();
        let traffic_count = self.traffic.read(cx).delegate().len();
        let ca_path = self
            .ca
            .as_ref()
            .map(|ca| ca.path.clone())
            .unwrap_or_else(|| "尚未生成".into());

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(crate::design::page_header(
                "MITM",
                "管理 HTTPS 检查、证书信任与请求记录",
                div(),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        Checkbox::new("mitm-enabled")
                            .label("启用 MITM")
                            .checked(self.enabled)
                            .disabled(editing_disabled)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.enabled = *checked;
                                this.dirty = true;
                                this.notice = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mitm-save")
                            .label("保存设置")
                            .disabled(editing_disabled || !self.dirty())
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    )
                    .child(
                        Button::new("mitm-reload")
                            .label("重新载入")
                            .disabled(editing_disabled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.reload(window, cx);
                            })),
                    )
                    .child(
                        Button::new("mitm-refresh")
                            .label("刷新状态")
                            .disabled(self.locked || busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_metadata(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .id("mitm-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
                    .min_w_0()
                    .max_h_16()
                    .flex_shrink_0()
                    .overflow_y_scrollbar()
                    .text_color(if status_error {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(status),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .max_w_full()
                            .min_w_0()
                            .min_h_0()
                            .gap_4()
                            .overflow_y_scrollbar()
                            .child(section_title(
                                "实验 CA",
                                "证书和私钥只写入实验数据根；系统信任必须显式确认",
                                cx,
                            ))
                            .child(info_row("证书路径", ca_path, cx))
                            .child(info_row("系统信任", trust, cx))
                            .child(info_row(
                                "SHA-256",
                                self.ca
                                    .as_ref()
                                    .map(|ca| ca.sha256.clone())
                                    .unwrap_or_else(|| "未加载".into()),
                                cx,
                            ))
                            .child(info_row(
                                "信任范围",
                                self.ca
                                    .as_ref()
                                    .map(|ca| format!("{:?}", ca.scope))
                                    .unwrap_or_else(|| "未加载".into()),
                                cx,
                            ))
                            .child(info_row(
                                "回执 SHA-256",
                                self.trust
                                    .as_ref()
                                    .and_then(|trust| trust.receipt_sha256.clone())
                                    .unwrap_or_else(|| "无拥有的安装回执".into()),
                                cx,
                            ))
                            .child(info_row(
                                "安装所有权",
                                self.trust
                                    .as_ref()
                                    .map(|trust| {
                                        match trust.ownership {
                                            CaOwnershipStatus::Untracked => {
                                                "无拥有的安装回执；既有根不会移除"
                                            }
                                            CaOwnershipStatus::Owned => {
                                                "本应用新增；回执与原始所有权匹配"
                                            }
                                            CaOwnershipStatus::Absent => {
                                                "已不在信任库；可清理旧回执"
                                            }
                                            CaOwnershipStatus::Changed => {
                                                "所有权已变；拒绝移除并保留回执"
                                            }
                                            CaOwnershipStatus::Unknown => {
                                                "所有权查询失败；禁止移除"
                                            }
                                        }
                                        .to_string()
                                    })
                                    .unwrap_or_else(|| "未查询".into()),
                                cx,
                            ))
                            .child(info_row(
                                "脚本方言",
                                format!("{:?}", self.script_dialect),
                                cx,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(
                                        Button::new("mitm-install-ca")
                                            .label("安装到系统信任库…")
                                            .min_w_0()
                                            .max_w_full()
                                            .disabled(self.locked || busy || self.ca.is_none())
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.request_install_ca(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("mitm-remove-ca")
                                            .label("移除拥有的信任…")
                                            .min_w_0()
                                            .max_w_full()
                                            .disabled(
                                                self.locked
                                                    || busy
                                                    || !self.trust.as_ref().is_some_and(|trust| {
                                                        matches!(
                                                            trust.ownership,
                                                            CaOwnershipStatus::Owned
                                                                | CaOwnershipStatus::Absent
                                                        )
                                                    }),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.request_remove_ca(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("mitm-copy-ca")
                                            .label("复制证书 PEM")
                                            .min_w_0()
                                            .max_w_full()
                                            .disabled(self.ca.is_none())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.copy_ca(cx);
                                            })),
                                    ),
                            )
                            .child(section_title(
                                "派生白名单",
                                "已启用远程片段与配置导入声明的 Hostname，只读",
                                cx,
                            ))
                            .child(if self.derived.is_empty() {
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("暂无派生 Hostname")
                            } else {
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .children(self.derived.iter().map(|entry| {
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .min_w_0()
                                            .items_center()
                                            .gap_2()
                                            .text_sm()
                                            .child(entry.hostname.clone())
                                            .child(
                                                div()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(entry.sources.join("、")),
                                            )
                                    }))
                            })
                            .child(section_title(
                                "手动补充",
                                "每行一个域名模式；- 或 ! 前缀表示排除项",
                                cx,
                            ))
                            .child(
                                div()
                                    .h_40()
                                    .flex_shrink_0()
                                    .child(Editor::new(&self.hostnames).size_full()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("空白名单不会启动 MITM；实验版不会自动接管系统代理。"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .flex_grow(2.)
                            .min_w_0()
                            .min_h_0()
                            .gap_2()
                            .child(section_title(
                                "抓包记录",
                                format!("本进程内存记录，最新在前 · {traffic_count} 条"),
                                cx,
                            ))
                            .child(
                                div()
                                    .px_3()
                                    .flex_shrink_0()
                                    .min_w_0()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(traffic_columns("时间", "方法", "URL", "状态", "耗时")),
                            )
                            .child(List::new(&self.traffic).flex_1().min_h_0()),
                    ),
            )
    }
}

fn section_title(
    title: impl Into<String>,
    description: impl Into<String>,
    cx: &Context<MitmView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .gap_1()
        .child(div().font_semibold().child(title.into()))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description.into()),
        )
}

fn info_row(
    label: impl Into<String>,
    value: impl Into<String>,
    cx: &Context<MitmView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .min_w_0()
        .items_start()
        .gap_2()
        .text_sm()
        .child(
            div()
                .w_24()
                .max_w_full()
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex_basis(gpui_kit::rems(8.))
                .whitespace_normal()
                .child(value.into()),
        )
}
