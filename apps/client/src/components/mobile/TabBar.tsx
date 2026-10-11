import { SlidersHorizontal, Settings, LayoutGrid } from "lucide-react";
import { NavLink } from "react-router-dom";

export const TABS = [
  { to: "/", label: "首页", icon: LayoutGrid, end: true },
  { to: "/config", label: "配置", icon: SlidersHorizontal, end: false },
  { to: "/settings", label: "设置", icon: Settings, end: false },
] as const;

export function TabBar() {
  return (
    <nav
      aria-label="底部导航"
      className="lanpath-tabbar fixed inset-x-0 bottom-0 z-30 px-[max(1rem,env(safe-area-inset-left))] pb-[max(0.5rem,env(safe-area-inset-bottom))] pt-2"
    >
      <div className="lanpath-tabbar-items flex min-h-16 items-center bg-surface/90 p-1 backdrop-blur-xl">
        {TABS.map(({ to, label, icon: Icon, end }) => (
          <NavLink
            key={to}
            to={to}
            end={end}
            className={({ isActive }) =>
              `flex min-h-14 flex-1 items-center justify-center gap-2 rounded-full text-sm font-medium outline-none transition-colors active:opacity-70 focus-visible:outline-2 focus-visible:outline-focus ${isActive ? "bg-primary/10 text-primary" : "text-muted"}`
            }
          >
            <Icon className="size-6" aria-hidden="true" />
            {label}
          </NavLink>
        ))}
      </div>
    </nav>
  );
}
