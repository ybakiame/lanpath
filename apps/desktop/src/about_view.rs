use gpui_kit::base::StyledExt as _;
use std::sync::Arc;

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, button::Button, scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_client::core_management::CoreManagementService;

use crate::runtime::DesktopRuntime;

pub(crate) struct AboutView {
    core: Arc<DesktopRuntime>,
    root: Option<SharedString>,
    selection: Option<SharedString>,
    error: Option<String>,
    locked: bool,
    closing: bool,
}

impl AboutView {
    pub(crate) fn new(core: Arc<DesktopRuntime>, cx: &mut Context<Self>) -> Self {
        let root = core
            .business()
            .ok()
            .map(|service| SharedString::from(service.data_dir().display().to_string()));
        let mut view = Self {
            core,
            root,
            selection: None,
            error: None,
            locked: false,
            closing: false,
        };
        view.refresh(cx);
        view
    }

    /// Refresh after persisted core selection, or through the explicit reload
    /// command. Inventory metadata is read here, never during rendering.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.locked || self.closing {
            return;
        }
        let result = self.read_selection();
        match result {
            Ok(selection) => {
                self.selection = Some(selection.into());
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        if self.locked != locked {
            self.locked = locked;
            cx.notify();
        }
    }

    pub(crate) fn set_closing(&mut self, closing: bool, cx: &mut Context<Self>) {
        if self.closing != closing {
            self.closing = closing;
            cx.notify();
        }
    }

    fn read_selection(&self) -> Result<String, String> {
        let business = self.core.business()?;
        let config = business
            .client_config()
            .map_err(|error| error.to_string())?;
        if let Some(binary) = self.core.core_override() {
            return Ok(format!(
                "测试核心覆盖（仅供显式验收，不改写已保存选择）\n{}\n持久选择：{}",
                binary.display(),
                if config.core_binary.as_os_str().is_empty() {
                    "尚未选择".to_owned()
                } else {
                    config.core_binary.display().to_string()
                },
            ));
        }
        let inventory = CoreManagementService::experimental(business.data_dir().to_path_buf());
        let installed = inventory.list_local().map_err(|error| error.to_string())?;
        if config.core_binary.as_os_str().is_empty() {
            return Ok("尚未选择核心；请在“核心”页下载或选择独立数据目录中的核心。".into());
        }
        match installed
            .iter()
            .find(|entry| entry.path() == config.core_binary)
        {
            Some(entry) => Ok(format!(
                "sing-box {}（本地库存版本）\n{}",
                entry.version(),
                entry.path().display()
            )),
            None => Err("已保存的核心未出现在有效本地库存中；请在“核心”页重新选择。".into()),
        }
    }
}

impl Render for AboutView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let information = |id: &'static str, value: SharedString| {
            div()
                .id(id)
                .accessibility_id(id)
                .role(Role::Group)
                .aria_label(value.clone())
                .text_sm()
                .child(value)
        };
        let root = self
            .root
            .clone()
            .unwrap_or_else(|| "独立数据目录不可用；未读取现行客户端数据。".into());
        div().flex().flex_col().flex_1().min_h_0().min_w_0()
            .overflow_y_scrollbar()
            .child(div().flex().flex_col().flex_shrink_0().min_w_0().w_full()
                .max_w(gpui_kit::rems(64.0)).gap_4().p_1()
                .child(div().text_lg().font_semibold().child(crate::product::DISPLAY_NAME))
                .child(div().flex().flex_col().gap_2()
                    .child("原生 GPUI 桌面客户端")
                    .child(information("about-app-version", concat!("应用版本：", env!("CARGO_PKG_VERSION")).into()))
                    .child(information("about-platform", format!("操作系统 / 架构：{} / {}", std::env::consts::OS, std::env::consts::ARCH).into()))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child(format!("程序身份：{}", crate::product::APP_ID))))
                .child(div().flex().flex_col().gap_2()
                    .child(div().font_semibold().child("独立数据目录"))
                    .child(information("about-data-directory", format!("数据目录：{root}").into()))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("配置、核心库存、日志、缓存与 MITM 证书保存在当前实例的独立数据根。现行 Tauri 与 Android 数据不会自动读取或迁移；迁移须在“迁移与更新”中显式预览并确认。")))
                .child(div().flex().flex_col().gap_2()
                    .child(div().font_semibold().child("已选择的核心"))
                    .children(self.selection.as_ref().map(|selection| {
                        information("about-selected-core", selection.clone())
                    }))
                    .children(self.error.as_ref().map(|error| {
                        let message = format!("无法读取当前核心信息：{error}{}", if self.selection.is_some() {
                            "；上方保留上次成功读取的核心快照，请重新载入。"
                        } else { "；请修复后重新载入。" });
                        div().id("about-error").role(Role::Status).aria_label(message.clone())
                            .accessibility_id("about-error")
                            .text_sm().text_color(cx.theme().danger).child(message)
                    }))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("版本来自所选本地库存，不在本页执行核心二进制。选中项不是运行状态证明；可重新载入当前持久选择。"))
                    .child(div().child(Button::new("about-reload")
                        .accessibility_id("about-reload").label("重新载入核心信息")
                        .disabled(self.locked || self.closing)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))))))
                .child(div().flex().flex_col().gap_2()
                    .child(div().font_semibold().child("独立目录与系统权限"))
                    .child(div().text_sm().child("Lanpath GPUI 以签名 ZIP 分发，默认便携；使用独立的数据目录与更新通道，与 Tauri 并行使用，不自动替换其配置或发行身份。"))
                    .child(div().text_sm().child("系统代理与 TUN 在“系统网络”中配置；证书信任在 MITM 页显式确认。涉及系统权限的操作会显示授权请求，不会由本页触发。"))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("本地库存版本与选中状态不代表运行兼容性已验证；请结合实际启动记录、诊断与日志判断。目录租约和资源检查并非操作系统安全沙箱。"))))
    }
}
