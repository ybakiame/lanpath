import { Monitor, Moon, Sun } from "lucide-react";
import { Segmented, SegmentedButton, IS_MOBILE } from "@lanpath/ui";
import { Card } from "@lanpath/ui";
import { useThemePreference } from "../../theme";
import type { ThemePreference, UiStylePreference } from "../../theme";
import { ACCENT_OPTIONS } from "../../appearance";

/** 主题选项（顺序即展示顺序；跟随系统为无记录时的默认值）。 */
const THEME_OPTIONS: { value: ThemePreference; label: string; icon: typeof Sun }[] = [
  { value: "system", label: "跟随系统", icon: Monitor },
  { value: "light", label: "浅色", icon: Sun },
  { value: "dark", label: "深色", icon: Moon },
];

/** UI 风格选项（@lanpath/ui 双主题；iOS 为无记录时的默认值）。 */
const UI_STYLE_OPTIONS: { value: UiStylePreference; label: string }[] = [
  { value: "ios", label: "iOS" },
  { value: "material", label: "Material" },
];

/**
 * 外观设置卡（设置主页直挂，非二级页）。
 *
 * 两组三选/二选分段控件（@lanpath/ui Segmented）：
 * - 主题：跟随系统 / 浅色 / 深色，选择持久化到 localStorage 并即时同步 `<html>`
 *   （index.html 预置脚本在首帧前读取同一键，冷启动无闪变）；
 * - UI 风格：iOS / Material（@lanpath/ui 双主题，经 html 令牌域响应式切换，
 *   无需重启）。
 */
export function AppearanceCard() {
  const { theme, setTheme, uiStyle, setUiStyle, accent, setAccent } = useThemePreference();

  return (
    <Card>
      <Card.Header>
        <Card.Title>外观</Card.Title>
        <Card.Description>让岚径呈现你的风格，修改即时生效</Card.Description>
      </Card.Header>
      <Card.Content className="flex flex-col gap-4">
        <div className="flex flex-col gap-2">
          <span className="text-sm font-medium text-zinc-900 dark:text-zinc-100">主题</span>
          <Segmented strong>
            {THEME_OPTIONS.map((option) => {
              const Icon = option.icon;
              return (
                <SegmentedButton
                  key={option.value}
                  active={theme === option.value}
                  onClick={() => setTheme(option.value)}
                  className="min-h-12"
                  aria-pressed={theme === option.value}
                >
                  <span className="inline-flex items-center gap-1.5">
                    <Icon className="size-4" aria-hidden="true" />
                    <span>{option.label}</span>
                  </span>
                </SegmentedButton>
              );
            })}
          </Segmented>
        </div>

        {IS_MOBILE && (
          <div className="flex flex-col gap-2">
            <span className="text-sm font-medium text-zinc-900 dark:text-zinc-100">界面风格</span>
            <Segmented strong>
              {UI_STYLE_OPTIONS.map((option) => (
                <SegmentedButton
                  key={option.value}
                  active={uiStyle === option.value}
                  onClick={() => setUiStyle(option.value)}
                  className="min-h-12"
                  aria-pressed={uiStyle === option.value}
                >
                  {option.label}
                </SegmentedButton>
              ))}
            </Segmented>
          </div>
        )}
        <div className="flex flex-col gap-2">
          <span className="text-sm font-medium text-foreground">强调色</span>
          <fieldset aria-label="强调色" className="grid grid-cols-2 gap-2">
            {ACCENT_OPTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                aria-pressed={accent === option.value}
                onClick={() => setAccent(option.value)}
                className={`flex min-h-12 items-center gap-3 rounded-2xl border px-3 text-sm outline-none transition-colors active:opacity-70 focus-visible:outline-2 focus-visible:outline-focus ${accent === option.value ? "border-primary bg-accent-soft text-accent-soft-foreground" : "border-border text-foreground"}`}
              >
                <span
                  data-accent={option.value}
                  className="size-5 shrink-0 rounded-full bg-accent"
                  aria-hidden="true"
                />
                <span className="flex-1 text-left">{option.label}</span>
                {accent === option.value && <span aria-hidden="true">✓</span>}
              </button>
            ))}
          </fieldset>
          <p className="text-xs text-muted">用于按钮、选中状态与焦点；运行警告仍使用独立状态色。</p>
        </div>
      </Card.Content>
    </Card>
  );
}
