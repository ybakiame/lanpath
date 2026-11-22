//! 主题注册（OKLCH 感知均匀色彩体系）：
//! 3 组精选命名预设（默认品牌 `lanpath`，清澈海蓝 `ocean`，深海青绿 `teal`）
//! 与 1 组用户自定义 4 色板（`custom`），每组完整浅色/深色配对并经 `set_tokens` 运行时热切换。
//! 正文/按钮对比度 ≥4.5:1、聚焦环与输入边界 ≥3:1，由构建器与单测逐项保证。

mod assemble;
pub mod custom;
pub mod oklch;
pub mod presets;

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use nsh_core::element::{AdaptiveColor, Tokens};
use nsh_core::{Hsla, element::set_tokens};

pub use custom::CustomPalette;
pub use presets::preset_seed;

/// 命名主题 ID（3 套精选预设 + 自定义；`Lanpath` 为默认首位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThemeId {
    Lanpath,
    Ocean,
    Teal,
    Custom,
}

impl ThemeId {
    /// 全部内置精选预设（不含 `Custom`，供预设导入与缓存构建使用）。
    pub const PRESETS: [ThemeId; 3] = [ThemeId::Lanpath, ThemeId::Ocean, ThemeId::Teal];

    /// 全部主题（3 预设 + 1 自定义），网格与校验顺序。
    pub const ALL: [ThemeId; 4] = [
        ThemeId::Lanpath,
        ThemeId::Ocean,
        ThemeId::Teal,
        ThemeId::Custom,
    ];

    /// 稳定存储/配置用 ID（小写连字符；不随语言变化）。
    pub fn id_name(self) -> &'static str {
        match self {
            ThemeId::Lanpath => "lanpath",
            ThemeId::Ocean => "ocean",
            ThemeId::Teal => "teal",
            ThemeId::Custom => "custom",
        }
    }

    /// 存储值对应的主题；未知/历史已下线主题回退默认 `lanpath`。
    pub fn from_id_name(name: &str) -> Self {
        ThemeId::ALL
            .into_iter()
            .find(|id| id.id_name() == name)
            .unwrap_or(ThemeId::Lanpath)
    }
}

static CUSTOM_SEED: LazyLock<RwLock<CustomPalette>> =
    LazyLock::new(|| RwLock::new(CustomPalette::default()));

/// 读取当前全局自定义 4 色种子。
pub fn current_custom_palette() -> CustomPalette {
    CUSTOM_SEED
        .read()
        .map(|g| *g)
        .unwrap_or_else(|_| CustomPalette::default())
}

/// 更新全局自定义 4 色种子（用于启动恢复或自定义配色器预览/保存）。
pub fn set_custom_palette(seed: CustomPalette) {
    if let Ok(mut guard) = CUSTOM_SEED.write() {
        *guard = seed;
    }
}

/// 计算指定主题在单外观下的全部 60 个语义色 token。
fn palette(id: ThemeId, dark: bool) -> Vec<(&'static str, Hsla)> {
    match id {
        ThemeId::Custom => custom::derive_custom_palette(current_custom_palette(), dark),
        _ => assemble::assemble_palette(presets::preset_spec(id, dark), dark),
    }
}

fn build_tokens_from(light: Vec<(&'static str, Hsla)>, dark: Vec<(&'static str, Hsla)>) -> Tokens {
    let mut tokens = nsh_base::Theme::default().into_tokens();
    let light_map: HashMap<&str, Hsla> = light.into_iter().collect();
    for (name, dark_color) in dark {
        let light_color = light_map
            .get(name)
            .unwrap_or_else(|| panic!("浅色配对缺 {name}"));
        tokens.set_color(
            name,
            AdaptiveColor {
                light: *light_color,
                dark: dark_color,
            },
        );
    }
    tokens.set_number("radius", 6.0);
    tokens.set_number("radius-sm", 4.0);
    tokens.set_number("radius-lg", 8.0);
    tokens
}

fn build_preset_tokens(id: ThemeId) -> Tokens {
    build_tokens_from(palette(id, false), palette(id, true))
}

/// 从给定自定义 4 色种子即时构建完整浅/深双外观 `Tokens`（供预览卡片与运行时热切换使用）。
pub fn custom_tokens(seed: CustomPalette) -> Tokens {
    build_tokens_from(
        custom::derive_custom_palette(seed, false),
        custom::derive_custom_palette(seed, true),
    )
}

static PRESET_TOKENS: LazyLock<Vec<Tokens>> = LazyLock::new(|| {
    ThemeId::PRESETS
        .iter()
        .map(|id| build_preset_tokens(*id))
        .collect()
});

/// 主题的完整 token（预设命中构建期缓存；`Custom` 按当前自定义种子实时派生）。
pub fn theme_tokens(id: ThemeId) -> Tokens {
    if id == ThemeId::Custom {
        return custom_tokens(current_custom_palette());
    }
    PRESET_TOKENS[id as usize].clone()
}

/// 运行时切换主题：所有 `theme()`/`tokens()` 读取方收到新 token，
/// 组件树不重挂、业务运行状态不重建。
pub fn apply(id: ThemeId) {
    set_tokens(theme_tokens(id));
}

/// 实时应用自定义 4 色板（同时更新全局自定义种子与运行时 token）。
pub fn apply_custom(seed: CustomPalette) {
    set_custom_palette(seed);
    set_tokens(custom_tokens(seed));
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
