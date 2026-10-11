import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { IS_MOBILE } from "@lanpath/ui";
import { useThemeState } from "@lanpath/client-core";
import type { ResolvedTheme, ThemePreference } from "@lanpath/client-core";
import { ACCENT_STORAGE_KEY, readStoredAccent, type AccentPreference } from "./appearance";

// 主题类型经本模块 re-export（消费方维持 `from "../theme"` 的既有导入路径）。
export type { ThemePreference } from "@lanpath/client-core";

interface ThemeContextValue {
  pureBlack: boolean;
  setPureBlack: (enabled: boolean) => void;
  accent: AccentPreference;
  setAccent: (accent: AccentPreference) => void;
  /** 用户偏好（可能为 `system`）。 */
  theme: ThemePreference;
  /** 实际生效的主题（`system` 解析后的具体值）。 */
  resolvedTheme: ResolvedTheme;
  /** 切换偏好（持久化到 localStorage 并同步 `<html>` 的 class/data-theme）。 */
  setTheme: (theme: ThemePreference) => void;
}

const ThemeContext = createContext<ThemeContextValue>({
  pureBlack: false,
  setPureBlack: () => undefined,
  accent: "iris",
  setAccent: () => undefined,
  theme: "system",
  resolvedTheme: "light",
  setTheme: () => undefined,
});

/** 共享主题管理负责深浅解析；应用层管理强调色与编译期平台的 DOM 呈现。 */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const { theme, resolvedTheme, setTheme, pureBlack, setPureBlack } = useThemeState();
  const [accent, setAccentState] = useState<AccentPreference>(() => readStoredAccent(IS_MOBILE ? "iris" : "blue"));

  useEffect(() => {
    document.documentElement.dataset.accent = accent;
  }, [accent]);

  useEffect(() => {
    document.documentElement.dataset.platform = IS_MOBILE ? "mobile" : "desktop";
  }, []);

  const value = useMemo<ThemeContextValue>(
    () => ({
      accent,
      setAccent: (next) => {
        setAccentState(next);
        try {
          localStorage.setItem(ACCENT_STORAGE_KEY, next);
        } catch {
          /* 存储不可用时仍支持会话内切换。 */
        }
      },
      theme,
      resolvedTheme,
      pureBlack,
      setPureBlack,
      setTheme,
    }),
    [theme, resolvedTheme, setTheme, accent, pureBlack, setPureBlack],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

/** 读取当前主题偏好与切换方法（设置页「外观」卡消费）。 */
export function useThemePreference(): ThemeContextValue {
  return useContext(ThemeContext);
}
