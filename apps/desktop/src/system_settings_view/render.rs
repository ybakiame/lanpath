use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    input::Input,
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use super::SystemSettingsView;
use crate::business::SystemSettingsDocument;

impl SystemSettingsView {
    fn option(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        field: fn(&mut SystemSettingsDocument) -> &mut bool,
        cx: &mut Context<Self>,
    ) -> Checkbox {
        Checkbox::new(id)
            .label(label)
            .checked(checked)
            .disabled(self.editing_disabled())
            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                if this.editing_disabled() {
                    return;
                }
                if let Some(draft) = &mut this.draft {
                    *field(draft) = *checked;
                }
                this.changed(cx);
            }))
    }
}

impl Render for SystemSettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.editing_disabled();
        let status = if let Some(working) = self.working {
            working.to_owned()
        } else if let Some(error) = &self.error {
            error.clone()
        } else if self.closing {
            "正在关闭；未保存修改会在关闭流程中确认。".into()
        } else if self.running {
            "核心运行中，停止后可编辑".into()
        } else if self.locked {
            "其他操作进行中，暂不可编辑".into()
        } else if self.dirty {
            "有未保存修改".into()
        } else {
            self.notice.clone().unwrap_or_else(|| "配置已载入".into())
        };
        let stack = self.draft.as_ref().map(|draft| draft.tun_stack.as_str());
        let stack_ix = ["system", "gvisor", "mixed"]
            .iter()
            .position(|value| Some(*value) == stack);
        div().flex().flex_col().flex_1().min_h_0().min_w_0()
            .child(div().id("system-network-scroll").accessibility_id("system-network-scroll")
                .role(Role::Group).aria_label("网络设置表单")
                .flex_1().min_h_0().min_w_0()
                .child(div().size_full().overflow_y_scrollbar()
                .child(div().flex().flex_col().flex_shrink_0().min_w_0().w_full()
                    .max_w(gpui_kit::rems(64.0)).gap_6().pb_4()
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("保存后下次启动核心生效；系统代理与 TUN 仍需明确启用。"))
                .child(crate::design::surface(cx).flex().flex_col().gap_3().p_4()
                    .child(div().font_semibold().child("系统接管（明确选择后启用）"))
                    .child(self.option("system-proxy-opt-in", "启动核心时启用系统代理", self.draft.as_ref().is_some_and(|d| d.system_proxy_enabled), |d| &mut d.system_proxy_enabled, cx))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("关闭时保留当前系统代理；启用后由核心生命周期接管并恢复本应用拥有的代理设置。"))
                    .child(self.option("system-tun-opt-in", "启动核心时启用 TUN", self.draft.as_ref().is_some_and(|d| d.tun_enabled), |d| &mut d.tun_enabled, cx))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("TUN 会改变流量路由；仅实际启动受限特权助手时请求 Windows 管理员授权。保存和浏览页面不提权。"))
                    .child("TUN 协议栈")
                    .child(RadioGroup::horizontal("system-tun-stack").selected_index(stack_ix).disabled(disabled)
                        .child(Radio::new("system-stack-system").label("System"))
                        .child(Radio::new("system-stack-gvisor").label("gVisor"))
                        .child(Radio::new("system-stack-mixed").label("Mixed"))
                        .on_change(cx.listener(|this, ix: &usize, _, cx| {
                            if this.editing_disabled() { return; }
                            if let (Some(draft), Some(stack)) = (&mut this.draft, ["system", "gvisor", "mixed"].get(*ix)) {
                                draft.tun_stack = (*stack).into();
                                this.changed(cx);
                            }
                        })))
                    .child(self.option("system-tun-auto-route", "TUN 自动路由", self.draft.as_ref().is_some_and(|d| d.tun_auto_route), |d| &mut d.tun_auto_route, cx)))
                .child(crate::design::surface(cx).flex().flex_col().gap_3().p_4()
                    .child(div().font_semibold().child("IP 与 DNS"))
                    .child(self.option("system-ipv6", "启用 IPv6", self.draft.as_ref().is_some_and(|d| d.ipv6_enabled), |d| &mut d.ipv6_enabled, cx))
                    .child(self.option("system-fakeip", "启用 DNS FakeIP", self.draft.as_ref().is_some_and(|d| d.dns_fakeip_enabled), |d| &mut d.dns_fakeip_enabled, cx)))
                .child(crate::design::surface(cx).flex().flex_col().gap_3().p_4()
                    .child(div().font_semibold().child("本地监听"))
                    .child("混合代理端口（HTTP / SOCKS）")
                    .child(Input::new(&self.mixed_port).id("system-mixed-port").aria_label("混合代理端口").disabled(disabled))
                    .child(self.option("system-api-enabled", "启用 Clash API", self.draft.as_ref().is_some_and(|d| d.clash_api_enabled), |d| &mut d.clash_api_enabled, cx))
                    .child("Clash API 端口")
                    .child(Input::new(&self.api_port).id("system-api-port").aria_label("Clash API 端口").disabled(disabled))
                    .child("Clash API 密钥")
                    .child(Input::new(&self.api_secret).id("system-api-secret").aria_label("Clash API 密钥（遮蔽显示）").disabled(disabled))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("密钥仅遮蔽显示，按原值保存；禁用 API 不会清空密钥。不要将密钥加入截图或诊断信息。"))
                    .child("Clash API 面板")
                    .child(Input::new(&self.api_ui).id("system-api-ui").aria_label("Clash API 面板").disabled(disabled))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("支持 metacubexd、zashboard。混合代理端口及启用的 API 端口必须为 1–65535，且不能与其他监听或已启用的 MITM 返回端口冲突；API 关闭时可保留端口 0，重新启用前需设置有效端口。"))))))
            .child(crate::design::form_footer("system-settings-status", status, self.error.is_some(),
div().flex().items_center().flex_wrap().gap_2()
                    .child(Button::new("system-save").primary().label("保存配置").disabled(disabled || !self.dirty)
                        .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))))
                    .child(Button::new("system-discard").label("放弃修改…").disabled(disabled || !self.dirty)
                        .on_click(cx.listener(|this, _, window, cx| this.discard(window, cx))))
                    .child(Button::new("system-reload").label("重新载入").disabled(self.unavailable() || self.dirty)
                        .on_click(cx.listener(|this, _, window, cx| this.load(window, cx)))), cx))
    }
}
