//! Appearance preferences use NSH storage backed by app.toml in the native host.
//! Applying tokens, color scheme, window backdrop, opacity or locale preserves
//! the component tree and business state.

use nsh_base::theme;
use nsh_core::desktop::{Backdrop, WindowAttributes};
use nsh_core::element::desktop::main_window;
use nsh_core::i18n::set_locale;
use nsh_core::platform::Locale;
use nsh_core::prelude::*;
use nsh_core::storage::{Storage, StorageFull};

use super::theme::{self, ThemeId};

/// 外观模式的存储键，由原生偏好后端映射到 app.toml 的 [ui]。
const MODE_KEY: &str = "appearance";
/// 主题 ID 的存储键。
const THEME_KEY: &str = "theme";
/// 界面语言的存储键。
const LANGUAGE_KEY: &str = "language";
/// 动效偏好的存储键。
const MOTION_KEY: &str = "reduce_motion";
/// 窗口背景材质的存储键。
const MATERIAL_KEY: &str = "window_material";
/// 背景不透明度的存储键（百分比 45..=100）。
const OPACITY_KEY: &str = "window_opacity";

pub const MIN_OPACITY: u8 = 45;
pub const MAX_OPACITY: u8 = 100;
pub const DEFAULT_OPACITY: u8 = 82;

/// 窗口背景材质：不透明纯色、Windows 11 云母（Mica）或亚克力磨砂（Acrylic）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMaterial {
    Solid,
    Mica,
    Acrylic,
}

impl WindowMaterial {
    pub const ALL: [WindowMaterial; 3] = [
        WindowMaterial::Acrylic,
        WindowMaterial::Mica,
        WindowMaterial::Solid,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            WindowMaterial::Solid => "appearance.backdrop.solid",
            WindowMaterial::Mica => "appearance.backdrop.mica",
            WindowMaterial::Acrylic => "appearance.backdrop.acrylic",
        }
    }

    fn storage_value(self) -> &'static str {
        match self {
            WindowMaterial::Solid => "solid",
            WindowMaterial::Mica => "mica",
            WindowMaterial::Acrylic => "acrylic",
        }
    }

    pub fn backdrop(self) -> Backdrop {
        match self {
            WindowMaterial::Solid => Backdrop::None,
            WindowMaterial::Mica => Backdrop::Window,
            WindowMaterial::Acrylic => Backdrop::Popover,
        }
    }

    fn load() -> Self {
        match Storage::get_item(MATERIAL_KEY).as_deref() {
            Some("solid") => WindowMaterial::Solid,
            Some("mica") => WindowMaterial::Mica,
            _ => WindowMaterial::Acrylic,
        }
    }
}

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

fn load_opacity() -> u8 {
    Storage::get_item(OPACITY_KEY)
        .and_then(|v| v.parse::<u8>().ok())
        .map(|v| v.clamp(MIN_OPACITY, MAX_OPACITY))
        .unwrap_or(DEFAULT_OPACITY)
}

/// 全局外观偏好（`global` 单例）：读取处随切换更新。
#[derive(Clone, Copy)]
pub struct UiPrefs {
    pub theme: Signal<ThemeId>,
    pub mode: Signal<Mode>,
    pub language: Signal<LanguagePref>,
    pub motion: Signal<MotionPref>,
    pub material: Signal<WindowMaterial>,
    pub opacity: Signal<u8>,
    /// 有效减少动效：平台设置或本应用偏好任一要求即成立（响应式）。
    pub reduced: Memo<bool>,
}

impl Default for UiPrefs {
    fn default() -> Self {
        let theme = signal(ThemeId::from_id_name(
            &Storage::get_item(THEME_KEY).unwrap_or_else(|| "ocean".into()),
        ));
        let mode = signal(Mode::load());
        let language = signal(LanguagePref::from_storage(Storage::get_item(LANGUAGE_KEY)));
        let motion = signal(MotionPref::load());
        let material = signal(WindowMaterial::load());
        let opacity = signal(load_opacity());
        let reduced =
            computed(move || reduce_motion() || matches!(motion.get(), MotionPref::Reduce));
        Self {
            theme,
            mode,
            language,
            motion,
            material,
            opacity,
            reduced,
        }
    }
}

/// 全局外观偏好单例。
pub fn prefs() -> UiPrefs {
    global::<UiPrefs>().get()
}

/// 根据当前材质与不透明度计算窗口主背景色；纯色模式始终 100% 不透明，
/// 磨砂/云母模式按自定义透明度透出底层原生 DWM 材质。
pub fn window_bg() -> AdaptiveColor {
    let p = prefs();
    match p.material.get() {
        WindowMaterial::Solid => theme().background,
        WindowMaterial::Mica | WindowMaterial::Acrylic => {
            let alpha = (p.opacity.get().clamp(MIN_OPACITY, MAX_OPACITY) as f32) / 100.0;
            theme().background.opacity(alpha)
        }
    }
}

/// 侧栏背景：磨砂模式下略高于主画布不透明度以保持清晰导航层级。
pub fn sidebar_bg() -> AdaptiveColor {
    let p = prefs();
    match p.material.get() {
        WindowMaterial::Solid => theme().sidebar,
        WindowMaterial::Mica | WindowMaterial::Acrylic => {
            let base = (p.opacity.get().clamp(MIN_OPACITY, MAX_OPACITY) as f32) / 100.0;
            theme().sidebar.opacity((base + 0.06).min(0.96))
        }
    }
}

/// 卡片表面背景：磨砂模式下保持较高不透明度（+12%），确保正文和按钮高可读。
pub fn card_bg() -> AdaptiveColor {
    let p = prefs();
    match p.material.get() {
        WindowMaterial::Solid => theme().card,
        WindowMaterial::Mica | WindowMaterial::Acrylic => {
            let base = (p.opacity.get().clamp(MIN_OPACITY, MAX_OPACITY) as f32) / 100.0;
            theme().card.opacity((base + 0.12).min(0.96))
        }
    }
}

/// 标题栏与底栏背景：与主画布协同的轻微加厚磨砂层。
pub fn chrome_bg() -> AdaptiveColor {
    let p = prefs();
    match p.material.get() {
        WindowMaterial::Solid => theme().card,
        WindowMaterial::Mica | WindowMaterial::Acrylic => {
            let base = (p.opacity.get().clamp(MIN_OPACITY, MAX_OPACITY) as f32) / 100.0;
            theme().card.opacity((base + 0.08).min(0.96))
        }
    }
}

fn apply_window_backdrop(material: WindowMaterial) {
    let mut attributes = WindowAttributes::default();
    attributes.backdrop = Some(material.backdrop());
    main_window().set(attributes);
}

impl UiPrefs {
    /// 应用主题并持久化：先确认写入成功，再更新信号与运行时。
    pub fn apply_theme(&self, id: ThemeId) -> Result<(), StorageFull> {
        Storage::set_item(THEME_KEY, id.id_name())?;
        self.theme.set(id);
        theme::apply(id);
        Ok(())
    }

    /// 应用外观模式并持久化（失败不改信号/运行时）。
    pub fn apply_mode(&self, mode: Mode) -> Result<(), StorageFull> {
        Storage::set_item(MODE_KEY, mode.storage_value())?;
        self.mode.set(mode);
        set_color_scheme(mode.appearance());
        Ok(())
    }

    /// 应用界面语言并持久化。
    pub fn apply_language(&self, language: LanguagePref) -> Result<(), StorageFull> {
        Storage::set_item(LANGUAGE_KEY, language.storage_value())?;
        self.language.set(language);
        self.set_language_runtime(language);
        Ok(())
    }

    /// 应用动效偏好并持久化（失败不改信号）。
    pub fn apply_motion(&self, motion: MotionPref) -> Result<(), StorageFull> {
        Storage::set_item(MOTION_KEY, motion.storage_value())?;
        self.motion.set(motion);
        Ok(())
    }

    /// 应用窗口背景材质（Acrylic / Mica / Solid）并持久化。
    pub fn apply_material(&self, material: WindowMaterial) -> Result<(), StorageFull> {
        Storage::set_item(MATERIAL_KEY, material.storage_value())?;
        self.material.set(material);
        apply_window_backdrop(material);
        Ok(())
    }

    /// 拖动过程实时更新透明度预览（不频繁写盘），松手时再调用 `apply_opacity`。
    pub fn preview_opacity(&self, percent: u8) {
        self.opacity.set(percent.clamp(MIN_OPACITY, MAX_OPACITY));
    }

    /// 持久化背景不透明度（45%..=100%）。
    pub fn apply_opacity(&self, percent: u8) -> Result<(), StorageFull> {
        let clamped = percent.clamp(MIN_OPACITY, MAX_OPACITY);
        Storage::set_item(OPACITY_KEY, clamped.to_string())?;
        self.opacity.set(clamped);
        Ok(())
    }

    fn set_language_runtime(&self, language: LanguagePref) {
        let locale = match language {
            LanguagePref::System => None,
            LanguagePref::ZhCn => Some(Locale::parse("zh-CN").unwrap_or(Locale::EN_US)),
            LanguagePref::En => Some(Locale::parse("en").unwrap_or(Locale::EN_US)),
        };
        set_locale(locale);
    }

    /// 启动安装：把持久化的主题/模式/语言/窗口材质一次性应用到运行时。
    pub fn install(&self) {
        theme::apply(self.theme.get_untracked());
        if self.mode.get_untracked() != Mode::System {
            set_color_scheme(self.mode.get_untracked().appearance());
        }
        let language = self.language.get_untracked();
        if language != LanguagePref::System {
            self.set_language_runtime(language);
        }
        apply_window_backdrop(self.material.get_untracked());
    }
}
