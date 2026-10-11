import { Monitor, Moon, Sun, Check } from "lucide-react";
import { Segmented, SegmentedButton, Switch } from "@lanpath/ui";
import { Card } from "@lanpath/ui";
import { useThemePreference } from "../../theme";
import type { ThemePreference } from "../../theme";
import { ACCENT_OPTIONS } from "../../appearance";

/** 主题选项（顺序即展示顺序；跟随系统为无记录时的默认值）。 */
const THEME_OPTIONS: { value: ThemePreference; label: string; icon: typeof Sun }[] = [
  { value: "system", label: "跟随系统", icon: Monitor },
  { value: "light", label: "浅色", icon: Sun },
  { value: "dark", label: "深色", icon: Moon },
];

/** 主题二级页：主题偏好与强调色即时保存，纯黑背景附属于深色外观。 */
export function AppearanceCard() {
  const { theme, setTheme, accent, setAccent, pureBlack, setPureBlack, resolvedTheme } = useThemePreference();

  return (
    <Card>
      <Card.Header>
        <Card.Title>主题模式</Card.Title>
        <Card.Description>让岚径呈现你的风格，修改即时生效</Card.Description>
      </Card.Header>
      <Card.Content className="flex flex-col gap-4">
        <div className="flex flex-col gap-2">
          <span className="text-sm font-medium text-zinc-900 dark:text-zinc-100">主题</span>
          <Segmented strong aria-label="主题模式">
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

        <div className="flex flex-col gap-2">
          <span className="text-sm font-medium text-foreground">深色外观</span>
          <div className="flex min-h-14 items-center justify-between gap-4 rounded-2xl bg-surface-secondary p-3">
            <span className="text-sm text-foreground">纯黑背景</span>
            <Switch
              aria-label="纯黑背景"
              isSelected={pureBlack}
              onValueChange={setPureBlack}
              isDisabled={resolvedTheme !== "dark"}
            />
          </div>
          <p className="text-xs leading-relaxed text-muted">
            适合 OLED 屏幕，仅在深色模式下生效；跟随系统进入深色时也会应用。切换浅色会保留此偏好。
          </p>
        </div>
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
                {accent === option.value && <Check className="size-4" aria-hidden="true" />}
              </button>
            ))}
          </fieldset>
          <p className="text-xs text-muted">用于按钮、选中状态与焦点；运行警告仍使用独立状态色。</p>
        </div>
      </Card.Content>
    </Card>
  );
}
