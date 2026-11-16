//! 动效 token（DESIGN.md 7）：时长 instant/fast/base/slow/slower、距离
//! 4/8/16pt、press 缩放 0.97（按钮）/0.95（图标按钮）。组件层一律经
//! [`motion_duration`] 取时长——用户或系统要求减少动效（平台
//! `reduce_motion()` 或本应用偏好）时返回 0，过渡立即可读。

/// Hover、颜色与透明度变化。
pub const INSTANT_MS: f32 = 120.0;
/// Press、按钮与选择状态。
pub const FAST_MS: f32 = 180.0;
/// Tab/侧栏选择、glyph 交换、菜单退出。
pub const BASE_MS: f32 = 240.0;
/// 菜单、弹窗、对话框进入。
pub const SLOW_MS: f32 = 360.0;
/// 页面首次列表进入（前 10 行错峰，之后不重复）。
pub const SLOWER_MS: f32 = 500.0;

/// 标签级位移。
pub const DISTANCE_LABEL_PT: f32 = 4.0;
/// 弹层位移。
pub const DISTANCE_POPOVER_PT: f32 = 4.0;
/// 对话框内容上升位移。
pub const DISTANCE_DIALOG_PT: f32 = 8.0;
/// 页面内容位移。
pub const DISTANCE_PAGE_PT: f32 = 16.0;

/// 普通按钮按下缩放。
pub const PRESS_SCALE: f32 = 0.97;
/// 图标按钮按下缩放。
pub const PRESS_SCALE_ICON: f32 = 0.95;

/// `token` 的实际过渡时长：`reduced` 时立即完成（0ms），组件以此驱动
/// `.duration(...)`，样式层过渡即刻可读。
pub fn motion_duration(token: f32, reduced: bool) -> f32 {
    if reduced { 0.0 } else { token }
}
