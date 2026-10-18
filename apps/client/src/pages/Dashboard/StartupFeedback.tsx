import { LoaderCircle } from "lucide-react";

/** Actual backend stage, shared by both targets; no estimated percentage. */
export function StartupFeedback({ stage }: { stage: string | null }) {
  if (!stage) return null;
  return (
    <output
      aria-live="polite"
      className="flex items-start gap-3 rounded-2xl border border-border bg-surface p-4 text-sm"
    >
      <LoaderCircle aria-hidden="true" className="mt-0.5 size-4 shrink-0 motion-safe:animate-spin" />
      <span className="min-w-0">
        <span className="block font-medium">正在启动代理</span>
        <span className="block break-words text-muted">{stage}</span>
        <span className="mt-1 block text-xs text-muted">
          首次启动或恢复旧备份后，缺失的资源需要联网下载，请保持应用开启。
        </span>
      </span>
    </output>
  );
}
