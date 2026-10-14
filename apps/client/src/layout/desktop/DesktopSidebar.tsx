import { BrandIcon } from "../../components/BrandIcon";
import clsx from "clsx";
import { NavLink } from "react-router-dom";
import { useCapabilities } from "@lanpath/client-core";
import { DESKTOP_NAV_ITEMS } from "../config/nav/desktop";
import type { DesktopNavItem } from "../config/nav/desktop";

function visibleNavItems(capabilities: ReturnType<typeof useCapabilities>["data"]) {
  if (!capabilities) return DESKTOP_NAV_ITEMS;
  const caps = capabilities.capabilities;
  return DESKTOP_NAV_ITEMS.filter((item: DesktopNavItem) => {
    if (item.requiresMitm && !caps.mitm) return false;
    if (item.requiresScriptsRemote && !caps.scripts_remote) return false;
    if (item.requiresCronTasks && !caps.cron_tasks) return false;
    return true;
  });
}

/** 宽窗口常驻，窄窗口由导航对话框承载同一菜单。 */
export function DesktopSidebar({ compact = false, onNavigate }: { compact?: boolean; onNavigate?: () => void }) {
  const { data: capabilities } = useCapabilities();
  const items = visibleNavItems(capabilities);

  return (
    <aside
      className={
        compact
          ? "flex flex-col gap-4"
          : "hidden min-h-0 w-56 shrink-0 flex-col gap-4 overflow-y-auto border-r border-border/60 bg-surface p-4 lg:flex"
      }
    >
      {!compact && (
        <div className="flex items-center gap-2 px-2">
          <BrandIcon />
          <span className="text-sm font-semibold">岚径 · Lanpath</span>
        </div>
      )}

      <nav className="flex flex-col gap-1" aria-label="主导航">
        {items.map((item: DesktopNavItem) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.to === "/"}
            onClick={onNavigate}
            className={({ isActive }) =>
              clsx(
                "flex items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors",
                isActive ? "bg-primary/10 text-primary" : "text-muted hover:bg-surface-secondary hover:text-foreground",
              )
            }
          >
            <item.icon className="size-4" aria-hidden="true" />
            {item.label}
          </NavLink>
        ))}
      </nav>
    </aside>
  );
}
