//! 外观偏好：主题、外观模式、界面语言、动效——统一持久化到 NSH
//! storage 并经信号驱动运行时切换（`set_tokens` / `set_color_scheme` /
//! `set_locale`），切换不重建组件树与业务运行状态。
//! 界面缩放是明确的不支持项：NSH 0.5.0 的 `font_scale` 仅平台可读，
//! 没有应用级覆盖 API，因此不提供存而不生效的缩放选项（见
//! `appearance.scale_note` 与 README）。

use nsh_core::i18n::set_locale;
use nsh_core::platform::Locale;
use nsh_core::prelude::*;
use nsh_core::storage::Storage;

use super::theme::{self, ThemeId};

/// 外观模式的存储键（沿用既有 `appearance` 键，兼容已保存值）。
const MODE_KEY: &str = "appearance";
/// 主题 ID 的存储键。
const THEME_KEY: &str = "theme";
/// 界面语言的存储键。
const LANGUAGE_KEY: &str = "language";
/// 动效偏好的存储键。
const MOTION_KEY: &str = "reduce_motion";

/// 窗口外观模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
    System,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::System, Mode::Light, Mode::Dark];

    pub fn label_key(self) -> &'static str {
        match self {
            Mode::System => "appearance.mode.system",
            Mode::Light => "appearance.mode.light",
            Mode::Dark => "appearance.mode.dark",
        }
    }

    fn storage_value(self) -> &'static str {
        match self {
            Mode::Light => "light",
            Mode::Dark => "dark",
            Mode::System => "system",
        }
    }

    fn appearance(self) -> Appearance {
        match self {
            Mode::Light => Appearance::Light,
            Mode::Dark => Appearance::Dark,
            Mode::System => Appearance::System,
        }
    }

    fn load() -> Self {
        match Storage::get_item(MODE_KEY).as_deref() {
            Some("light") => Mode::Light,
            Some("dark") => Mode::Dark,
            _ => Mode::System,
        }
    }
}

/// 界面语言偏好（zh-TW 结构预留：新增语言只加 yml 与这里的选项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguagePref {
    System,
    ZhCn,
    En,
}

impl LanguagePref {
    pub const ALL: [LanguagePref; 3] = [LanguagePref::System, LanguagePref::ZhCn, LanguagePref::En];

    pub fn label_key(self) -> &'static str {
        match self {
            LanguagePref::System => "appearance.language.system",
            LanguagePref::ZhCn => "appearance.language.zh_cn",
            LanguagePref::En => "appearance.language.en",
        }
    }

    fn storage_value(self) -> &'static str {
        match self {
            LanguagePref::System => "system",
            LanguagePref::ZhCn => "zh-CN",
            LanguagePref::En => "en",
        }
    }

    fn from_storage(value: Option<String>) -> Self {
        match value.as_deref() {
            Some("zh-CN") => LanguagePref::ZhCn,
            Some("en") => LanguagePref::En,
            _ => LanguagePref::System,
        }
    }
}

/// 动效偏好：跟随系统或始终减少动效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionPref {
    System,
    Reduce,
}

impl MotionPref {
    pub const ALL: [MotionPref; 2] = [MotionPref::System, MotionPref::Reduce];

    pub fn label_key(self) -> &'static str {
        match self {
            MotionPref::System => "appearance.motion.system",
            MotionPref::Reduce => "appearance.motion.reduce",
        }
    }

    fn storage_value(self) -> &'static str {
        match self {
            MotionPref::System => "system",
            MotionPref::Reduce => "reduce",
        }
    }

    fn load() -> Self {
        match Storage::get_item(MOTION_KEY).as_deref() {
            Some("reduce") => MotionPref::Reduce,
            _ => MotionPref::System,
        }
    }
}

/// 全局外观偏好（`global` 单例）：读取处随切换更新。
#[derive(Clone)]
pub struct UiPrefs {
    pub theme: Signal<ThemeId>,
    pub mode: Signal<Mode>,
    pub language: Signal<LanguagePref>,
    pub motion: Signal<MotionPref>,
    /// 有效减少动效：平台设置或本应用偏好任一要求即成立（响应式）。
    pub reduced: Memo<bool>,
}

impl Default for UiPrefs {
    fn default() -> Self {
        let theme = signal(ThemeId::from_id_name(
            &Storage::get_item(THEME_KEY).unwrap_or_default(),
        ));
        let mode = signal(Mode::load());
        let language = signal(LanguagePref::from_storage(Storage::get_item(LANGUAGE_KEY)));
        let motion = signal(MotionPref::load());
        let reduced =
            computed(move || reduce_motion() || matches!(motion.get(), MotionPref::Reduce));
        Self {
            theme,
            mode,
            language,
            motion,
            reduced,
        }
    }
}

/// 全局外观偏好单例。
pub fn prefs() -> UiPrefs {
    global::<UiPrefs>().get()
}

impl UiPrefs {
    /// 应用主题并持久化：只换 token，不重挂界面。
    pub fn apply_theme(&self, id: ThemeId) {
        self.theme.set(id);
        let _ = Storage::set_item(THEME_KEY, id.id_name());
        theme::apply(id);
    }

    /// 应用外观模式并持久化。
    pub fn apply_mode(&self, mode: Mode) {
        self.mode.set(mode);
        let _ = Storage::set_item(MODE_KEY, mode.storage_value());
        set_color_scheme(mode.appearance());
    }

    /// 应用界面语言并持久化：`None` 表示跟随系统。语言切换只改
    /// `t!` 读取结果，组件树与业务运行状态不重建。
    pub fn apply_language(&self, language: LanguagePref) {
        self.language.set(language);
        let _ = Storage::set_item(LANGUAGE_KEY, language.storage_value());
        let locale = match language {
            LanguagePref::System => None,
            LanguagePref::ZhCn => Some(Locale::parse("zh-CN").unwrap_or(Locale::EN_US)),
            LanguagePref::En => Some(Locale::parse("en").unwrap_or(Locale::EN_US)),
        };
        set_locale(locale);
    }

    /// 应用动效偏好并持久化。
    pub fn apply_motion(&self, motion: MotionPref) {
        self.motion.set(motion);
        let _ = Storage::set_item(MOTION_KEY, motion.storage_value());
    }

    /// 启动安装：把持久化的主题/模式/语言一次性应用到运行时
    /// （mode 非 System 时覆盖系统外观；语言 System 时不调用
    /// set_locale，跟随 `locale_settings()` 的首选语言）。
    pub fn install(&self) {
        theme::apply(self.theme.get_untracked());
        if self.mode.get_untracked() != Mode::System {
            set_color_scheme(self.mode.get_untracked().appearance());
        }
        let language = self.language.get_untracked();
        if language != LanguagePref::System {
            self.apply_language(language);
        }
    }
}
