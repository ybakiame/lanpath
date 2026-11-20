//! `ui` 语义组件层：功能页只从这里取组件，
//! 传业务语义、内容与事件，不传颜色、圆角或局部字体。展示代码经
//! `theme()` 读取语义 token；颜色字面量只允许出现在 `theme.rs` 的
//! 调色板构建中。
//!
//! - `theme`：14 组命名主题注册与运行时切换（`set_tokens`），包含高饱和清新主题 `mint`。
//! - `motion`：动效 token（时长/距离/press 缩放）与减少动效折算。
//! - `prefs`：主题/模式/语言/动效偏好的持久化与运行时应用。
//! - `button`/`input`/`select`/`layout`/`feedback`/`overlay`/`data`：
//!   语义组件。
//! - `theme_preview`：主题预览块。
//! - `appearance_page`：外观设置页。
//! - `gallery`：仅开发验收的组件画廊（`--ui-gallery`，无用户导航
//!   入口）。

pub mod appearance_page;
pub mod button;
pub mod cards;
pub mod code_editor;
pub mod data;
pub mod dialog;
pub mod feedback;
pub mod gallery;
pub mod icon;
pub mod input;
pub mod layout;
pub mod motion;
pub mod overlay;
pub mod prefs;
pub mod select;
pub mod sheet;
pub mod slider;
pub mod theme;
pub mod theme_preview;
pub mod title_bar;

pub use button::{ButtonSize, ButtonVariant, IconButton, button, icon_button};
pub use data::table;
pub use feedback::{
    BadgeVariant, badge, badge_with_icon, empty, empty_with_icon, show_toast, skeleton,
    skeleton_line, spinner, status_bar, toast_host,
};
pub use input::{Field, Input, Textarea, field, input, textarea};
pub use layout::{
    AlertTone, ListItem, SurfaceLevel, alert_banner, list_item, page_header, section_title,
    separator, surface, toolbar,
};
pub use overlay::{ConfirmDialog, confirm_dialog, popover, tooltip};
pub use prefs::{LanguagePref, Mode, MotionPref, UiPrefs, WindowMaterial, prefs};
pub use select::{checkbox, radio_row, segmented_group, select, switch, tab_bar};
pub use slider::slider;
pub use theme::{ThemeId, apply as apply_theme, theme_tokens};
pub use theme_preview::theme_preview;
