/**
 * Alert（T0 复合）：内联提示条，对齐桌面 HeroUI `Alert` 复合用法
 * （`status` + `Alert.Indicator/Content/Title/Description`，桌面现存 38 处）。
 * Indicator 图标随 status 自动选择（default/accent→信息，success→对勾，
 * warning/danger→三角警示），也可经 children 自定义。
 */
import { createContext, useContext, type ReactNode } from "react";
import { CircleCheck, Info, TriangleAlert } from "lucide-react";
import { cx } from "../utils.ts";

export type AlertStatus = "default" | "accent" | "success" | "warning" | "danger";

const AlertStatusContext = createContext<AlertStatus>("default");

const ROOT_COLORS: Record<AlertStatus, string> = {
  default: "bg-default-soft text-default-soft-foreground",
  accent: "bg-accent-soft text-accent-soft-foreground",
  success: "bg-success-soft text-success-soft-foreground",
  warning: "bg-warning-soft text-warning-soft-foreground",
  danger: "bg-danger-soft text-danger-soft-foreground",
};

const INDICATOR_COLORS: Record<AlertStatus, string> = {
  default: "text-foreground",
  accent: "text-accent",
  success: "text-success",
  warning: "text-warning",
  danger: "text-danger",
};

interface AlertSectionProps {
  className?: string;
  children?: ReactNode;
}

interface AlertRootProps extends AlertSectionProps {
  status?: AlertStatus;
}

function AlertRoot({ status = "default", className, children }: AlertRootProps) {
  return (
    <AlertStatusContext.Provider value={status}>
      <div role="alert" className={cx("flex gap-3 rounded-xl p-4", ROOT_COLORS[status], className)}>
        {children}
      </div>
    </AlertStatusContext.Provider>
  );
}

function AlertIndicator({ className, children }: AlertSectionProps) {
  const status = useContext(AlertStatusContext);
  const Icon = status === "success" ? CircleCheck : status === "warning" || status === "danger" ? TriangleAlert : Info;
  return (
    <div className={cx("shrink-0", INDICATOR_COLORS[status], className)}>
      {children ?? <Icon className="size-5" aria-hidden="true" />}
    </div>
  );
}

function AlertContent({ className, children }: AlertSectionProps) {
  return <div className={cx("flex min-w-0 flex-col gap-1", className)}>{children}</div>;
}

function AlertTitle({ className, children }: AlertSectionProps) {
  return <p className={cx("text-sm font-medium", className)}>{children}</p>;
}

function AlertDescription({ className, children }: AlertSectionProps) {
  return <p className={cx("text-sm opacity-90", className)}>{children}</p>;
}

export const Alert = Object.assign(AlertRoot, {
  Indicator: AlertIndicator,
  Content: AlertContent,
  Title: AlertTitle,
  Description: AlertDescription,
});
