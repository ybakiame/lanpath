//! `ui` 语义组件层：功能页只从这里取组件，
//! 传业务语义、内容与事件，不传颜色、圆角或局部字体。展示代码经
//! `theme()` 读取语义 token；颜色字面量只允许出现在 `theme.rs` 的
//! 调色板构建中。
//!
//! - `theme`：13 组命名主题注册与运行时切换（`set_tokens`）。
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
pub mod data;
pub mod feedback;
pub mod gallery;
pub mod input;
pub mod layout;
pub mod motion;
pub mod overlay;
pub mod prefs;
pub mod select;
pub mod theme;
pub mod theme_preview;

pub use button::{ButtonSize, ButtonVariant, IconButton, button, icon_button};
pub use data::table;
pub use feedback::{
    BadgeVariant, badge, empty, show_toast, skeleton, spinner, status_bar, toast_host,
};
pub use input::{Field, Input, Textarea, field, input, textarea};
pub use layout::{
    ListItem, SurfaceLevel, list_item, page_header, section_title, separator, surface, toolbar,
};
pub use overlay::{ConfirmDialog, confirm_dialog, tooltip};
pub use prefs::{LanguagePref, Mode, MotionPref, UiPrefs, prefs};
pub use select::{checkbox, radio_row, select, switch, tab_bar};
pub use theme::{ThemeId, apply as apply_theme, theme_tokens};
pub use theme_preview::theme_preview;
