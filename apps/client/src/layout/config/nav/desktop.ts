import type { ComponentType, SVGProps } from "react";
import {
  SlidersHorizontal,
  SlidersVertical,
  RefreshCw,
  FlaskConical,
  ChartColumn,
  Settings,
  FileText,
  Globe,
  Server,
  LayoutGrid,
  Wrench,
} from "lucide-react";

/** 桌面端导航项：目标路由、展示名与图标。 */
export interface DesktopNavItem {
  to: string;
  label: string;
  icon: ComponentType<SVGProps<SVGSVGElement>>;
  /** 该导航项是否要求 `capabilities.mitm` 为 true；Android 下隐藏。 */
  requiresMitm?: boolean;
  /** 该导航项是否要求 `capabilities.scripts_remote` 为 true；Android 下隐藏。 */
  requiresScriptsRemote?: boolean;
  /** 该导航项是否要求 `capabilities.cron_tasks` 为 true；阶段③后始终 true。 */
  requiresCronTasks?: boolean;
}

/** 桌面端主导航项（侧边栏展示）。
 *
 * 能力门控字段说明：
 * - `requiresMitm=true`：MITM 页，Android 下 `capabilities.mitm=false` 时隐藏
 * - `requiresScriptsRemote=true`：脚本页中依赖远程资源的功能，Android 下隐藏
 * - `requiresCronTasks=true`：脚本页（定时任务），阶段③后始终显示
 *
 * 阶段③解耦后：脚本页在 Android 显示，但仅展示「定时任务」Tab。
 */
export const DESKTOP_NAV_ITEMS: DesktopNavItem[] = [
  { to: "/", label: "仪表盘", icon: LayoutGrid },
  { to: "/proxies", label: "代理", icon: Globe },
  { to: "/subscriptions", label: "订阅", icon: Server },
  { to: "/config", label: "配置", icon: SlidersVertical },
  { to: "/connections", label: "连接", icon: RefreshCw },
  { to: "/stats", label: "统计", icon: ChartColumn },
  { to: "/mitm", label: "MITM", icon: FlaskConical, requiresMitm: true },
  { to: "/scripts", label: "脚本", icon: Wrench, requiresCronTasks: true },
  { to: "/override", label: "覆写", icon: SlidersHorizontal },
  { to: "/logs", label: "日志", icon: FileText },
  { to: "/settings", label: "设置", icon: Settings },
];
