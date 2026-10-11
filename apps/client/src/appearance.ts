/** 外观偏好只属于 UI，不进入核心配置或 IPC。 */
export const ACCENT_OPTIONS = [
  { value: "iris", label: "鸢尾紫" },
  { value: "teal", label: "湖水青" },
  { value: "rose", label: "玫瑰粉" },
  { value: "blue", label: "品牌蓝" },
] as const;

export type AccentPreference = (typeof ACCENT_OPTIONS)[number]["value"];
export const ACCENT_STORAGE_KEY = "lanpath-ui-accent";

export function readStoredAccent(fallback: AccentPreference): AccentPreference {
  try {
    const stored = localStorage.getItem(ACCENT_STORAGE_KEY);
    return ACCENT_OPTIONS.find((option) => option.value === stored)?.value ?? fallback;
  } catch {
    return fallback;
  }
}
