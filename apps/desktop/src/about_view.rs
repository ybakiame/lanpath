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
            return Ok("尚未选择核心；请在“核心”页下载或选择实验目录中的核心。".into());
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
            None => Err("已保存的核心未出现在有效实验库存中；请在“核心”页重新选择。".into()),
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
            .unwrap_or_else(|| "实验数据目录未获准打开；未读取生产数据。".into());
        div().flex().flex_col().flex_1().min_h_0().min_w_0()
            .overflow_y_scrollbar()
            .child(div().flex().flex_col().gap_4().p_1()
                .child(div().text_lg().child("关于 Lanpath 实验版"))
                .child(div().flex().flex_col().gap_2()
                    .child("GPUI 实验性并行目标")
                    .child(information("about-app-version", concat!("应用版本：", env!("CARGO_PKG_VERSION")).into()))
                    .child(information("about-platform", format!("操作系统 / 架构：{} / {}", std::env::consts::OS, std::env::consts::ARCH).into()))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child(concat!("程序身份：", env!("CARGO_PKG_NAME"), "；不是现行 Tauri 客户端的替代发行版。"))))
                .child(div().flex().flex_col().gap_2()
                    .child("隔离数据目录")
                    .child(information("about-data-directory", format!("隔离数据目录：{root}").into()))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("配置、核心库存、日志、缓存与隔离证书均使用本实例租用的实验目录。不自动读取或迁移生产数据；不应把生产目录作为实验目录。")))
                .child(div().flex().flex_col().gap_2()
                    .child("已选择的实验核心")
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
                    .child("实验边界")
                    .child(div().text_sm().child("现行 Tauri 桌面端与 Android 继续维护。此目标不改变安装器、更新器、签名或发布身份，不提供安装或更新操作。"))
                    .child(div().text_sm().child("本实验不接管系统代理或 TUN，不自动安装 CA。证书信任变更需要单独授权；此页面没有系统设置、外部链接或打开目录操作。"))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("目录租约与资源检查用于防止误用生产数据，并非抵抗同用户恶意运行期文件替换的操作系统沙箱。"))))
    }
}
