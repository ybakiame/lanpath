use super::MitmView;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, div,
};
use lanpath_client::mitm::{CaOwnershipStatus, CaTrustStatus};

impl MitmView {
    pub(super) fn render_certificate(&self, cx: &Context<Self>) -> impl IntoElement {
        let status = self
            .trust
            .as_ref()
            .map(|trust| match trust.status {
                CaTrustStatus::Trusted => "已安装并信任",
                CaTrustStatus::NotTrusted => "未信任",
                CaTrustStatus::Unknown => "状态未知",
            })
            .unwrap_or("正在查询…");
        let disabled = self.locked || self.busy();
        crate::design::surface(cx)
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .gap_2()
            .p_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(div().font_semibold().child("CA"))
                    .child(
                        div()
                            .id("mitm-ca-status")
                            .accessibility_id("mitm-ca-status")
                            .role(Role::Status)
                            .aria_label(status)
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("mitm-install-ca")
                            .accessibility_id("mitm-install-ca")
                            .small()
                            .label("安装证书…")
                            .disabled(
                                disabled
                                    || self.ca.is_none()
                                    || self.trust.as_ref().is_some_and(|trust| {
                                        trust.status == CaTrustStatus::Trusted
                                    }),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_install_ca(window, cx)
                            })),
                    )
                    .child(
                        Button::new("mitm-remove-ca")
                            .accessibility_id("mitm-remove-ca")
                            .small()
                            .label("移除信任…")
                            .disabled(
                                disabled
                                    || !self.trust.as_ref().is_some_and(|trust| {
                                        matches!(
                                            trust.ownership,
                                            CaOwnershipStatus::Owned | CaOwnershipStatus::Absent
                                        )
                                    }),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_remove_ca(window, cx)
                            })),
                    )
                    .child(
                        Button::new("mitm-ca-details")
                            .accessibility_id("mitm-ca-details")
                            .small()
                            .ghost()
                            .label(if self.ca_details_open {
                                "收起详情"
                            } else {
                                "证书详情"
                            })
                            .icon(if self.ca_details_open {
                                gpui_kit::assets::IconName::ChevronUp
                            } else {
                                gpui_kit::assets::IconName::ChevronDown
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ca_details_open = !this.ca_details_open;
                                cx.notify();
                            })),
                    ),
            )
            .children(self.ca_details_open.then(|| {
                div()
                    .id("mitm-ca-details-scroll")
                    .accessibility_id("mitm-ca-details-scroll")
                    .role(Role::Group)
                    .aria_label("证书详情滚动区域")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .max_h_24()
                    .overflow_y_scroll()
                    .child(info(
                        "证书路径",
                        self.ca
                            .as_ref()
                            .map(|ca| ca.path.clone())
                            .unwrap_or_else(|| "尚未生成".into()),
                        cx,
                    ))
                    .child(info(
                        "SHA-256",
                        self.ca
                            .as_ref()
                            .map(|ca| ca.sha256.clone())
                            .unwrap_or_else(|| "未加载".into()),
                        cx,
                    ))
                    .child(info(
                        "信任范围",
                        self.ca
                            .as_ref()
                            .map(|ca| format!("{:?}", ca.scope))
                            .unwrap_or_else(|| "未加载".into()),
                        cx,
                    ))
                    .child(info(
                        "信任详情",
                        self.trust
                            .as_ref()
                            .map(|trust| trust.detail.clone())
                            .unwrap_or_else(|| "未查询".into()),
                        cx,
                    ))
                    .child(info(
                        "安装所有权",
                        self.trust
                            .as_ref()
                            .map(|trust| {
                                match trust.ownership {
                                    CaOwnershipStatus::Untracked => "无安装回执；既有证书不会移除",
                                    CaOwnershipStatus::Owned => "本应用新增；所有权匹配",
                                    CaOwnershipStatus::Absent => "证书已不在信任库；可清理回执",
                                    CaOwnershipStatus::Changed => "所有权已变；拒绝移除",
                                    CaOwnershipStatus::Unknown => "查询失败；禁止移除",
                                }
                                .to_owned()
                            })
                            .unwrap_or_else(|| "未查询".into()),
                        cx,
                    ))
                    .child(info(
                        "回执指纹",
                        self.trust
                            .as_ref()
                            .and_then(|trust| trust.receipt_sha256.clone())
                            .unwrap_or_else(|| "无安装回执".into()),
                        cx,
                    ))
                    .child(
                        Button::new("mitm-copy-ca")
                            .accessibility_id("mitm-copy-ca")
                            .label("复制证书 PEM")
                            .disabled(self.ca.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.copy_ca(cx))),
                    )
            }))
    }
}
fn info(label: &'static str, value: String, cx: &Context<MitmView>) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .gap_2()
        .min_w_0()
        .text_sm()
        .child(
            div()
                .w_24()
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(div().flex_1().min_w_0().whitespace_normal().child(value))
}
