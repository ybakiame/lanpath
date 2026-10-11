/**
 * Spinner（T0 纯样式）：SVG 旋转加载指示。
 * `color`: `accent` = 主题主色（默认）；`current` / `white` = 跟随/固定文字色
 * （实心按钮内嵌 spinner 用 `white`）。
 */
import { LoaderCircle } from "lucide-react";
import { cx } from "../utils.ts";

interface SpinnerProps {
  size?: "sm" | "md" | "lg";
  color?: "accent" | "current" | "white";
  className?: string;
  "aria-hidden"?: boolean | "true" | "false";
}

const SIZES = { sm: "size-4", md: "size-6", lg: "size-8" } as const;
const COLORS = { accent: "text-accent", current: "text-current", white: "text-white" } as const;

export function Spinner({ size = "md", color = "accent", className, ...rest }: SpinnerProps) {
  return (
    <LoaderCircle
      role="status"
      aria-label={rest["aria-hidden"] ? undefined : "加载中"}
      className={cx("animate-spin", SIZES[size], COLORS[color], className)}
      {...rest}
    />
  );
}
