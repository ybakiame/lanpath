import { Chip, Card } from "@lanpath/ui";
import { RefreshCw, SquarePen, Trash2 } from "lucide-react";
import type { CustomRuleSetView } from "@lanpath/client-core";

interface CustomRuleSetCardProps {
  ruleSet: CustomRuleSetView;
  selected: boolean;
  onSelect: () => void;
  /** 更新进行中（按钮旋转禁用 + 底部 indeterminate 进度条；批量更新时全部 Remote 卡）。 */
  updating: boolean;
  onEdit: () => void;
  onDelete: () => void;
  onUpdate: () => void;
}

/** 来源类型 label（对齐 CustomRuleSetSource）。 */
function sourceLabel(ruleSet: CustomRuleSetView): string {
  if (ruleSet.source.kind === "manual") return "手动 JSON";
  return ruleSet.source.format === "binary" ? "远程 srs" : "远程 json";
}

function formatUpdated(lastUpdated: number): string {
  if (lastUpdated <= 0) return "未记录";
  return new Date(lastUpdated * 1000).toLocaleString();
}

/** 远程 `Last-Modified` 展示（0 = 未知）。 */
function formatRemote(remoteUpdatedAt: number): string {
  if (remoteUpdatedAt <= 0) return "上游未提供";
  return new Date(remoteUpdatedAt * 1000).toLocaleString();
}

/** User category, local availability and independent write/check/source timestamps.
 * Missing source dates are explicit; content freshness comes from conditional requests.
 */
export function CustomRuleSetCard({
  ruleSet,
  selected,
  onSelect,
  updating,
  onEdit,
  onDelete,
  onUpdate,
}: CustomRuleSetCardProps) {
  const { name, tag, cached, remote_updated_at: remoteUpdatedAt, last_updated: lastUpdated } = ruleSet;
  const isRemote = ruleSet.source.kind === "remote";
  return (
    <Card className="p-2">
      <div className="flex flex-wrap items-center justify-between gap-2 px-2">
        <label className="flex min-h-11 cursor-pointer items-center gap-2 text-xs text-muted">
          <input
            type="checkbox"
            checked={selected}
            onChange={onSelect}
            aria-label={`选择规则集 ${name.trim() || tag}`}
            className="size-4 accent-primary"
          />
          选择整理
        </label>
        <Chip className="max-w-full">
          <span className="truncate">{ruleSet.category || "未分类"}</span>
        </Chip>
      </div>
      <div className="flex items-center gap-1 px-2 py-1 pl-0">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5 px-2 py-2 pl-1">
          <span className="flex min-w-0 items-center gap-1.5">
            <span className="min-w-0 truncate text-sm font-medium text-zinc-900 dark:text-zinc-100">
              {name.trim() || tag}
            </span>
            <Chip color="accent" className="shrink-0">
              {sourceLabel(ruleSet)}
            </Chip>
            {cached ? (
              <Chip color="accent" className="shrink-0">
                已缓存
              </Chip>
            ) : (
              <Chip color="default" className="shrink-0">
                未缓存
              </Chip>
            )}
            {ruleSet.builtin && (
              <Chip color="default" className="shrink-0">
                内置
              </Chip>
            )}
          </span>
          <span className="truncate text-xs text-zinc-500 dark:text-zinc-400">
            规则集引用名：<span className="font-mono text-zinc-700 dark:text-zinc-300">{tag}</span>
          </span>
          <span className="flex min-w-0 flex-wrap items-center gap-1.5 text-xs text-zinc-500 dark:text-zinc-400">
            <span className="shrink-0">本地更新于 {formatUpdated(lastUpdated)}</span>
          </span>
          {isRemote && (
            <span className="truncate text-xs text-zinc-500 dark:text-zinc-400">
              上游声明时间：<span className="text-zinc-700 dark:text-zinc-300">{formatRemote(remoteUpdatedAt)}</span>
            </span>
          )}
          {isRemote && (
            <span className="text-xs text-muted">
              最近检查：{ruleSet.last_checked > 0 ? formatUpdated(ruleSet.last_checked) : "尚未检查"}
            </span>
          )}
          {updating && (
            /* 不确定进度条（Konsta Progressbar 无 indeterminate，纯 CSS 实现） */
            <div
              aria-label={`正在更新规则集 ${name.trim() || tag}`}
              className="mt-1 h-0.5 w-full overflow-hidden rounded-full bg-primary/20"
            >
              <div className="h-full w-full animate-pulse rounded-full bg-primary" />
            </div>
          )}
        </div>
      </div>
      <div className="flex items-center justify-end gap-1 border-t border-zinc-200 dark:border-zinc-700 py-0.5 pr-1">
        {isRemote && (
          <button
            type="button"
            onClick={onUpdate}
            disabled={updating}
            aria-label={`更新规则集 ${name.trim() || tag}`}
            className="flex min-h-11 shrink-0 items-center gap-1 rounded-lg px-3 text-sm text-zinc-900 dark:text-zinc-100 active:opacity-70 disabled:cursor-not-allowed disabled:opacity-40"
          >
            <RefreshCw className={`size-4 ${updating ? "animate-spin" : ""}`} aria-hidden="true" />
            {updating ? "更新中…" : "更新"}
          </button>
        )}
        <button
          type="button"
          onClick={onEdit}
          className="flex min-h-11 shrink-0 items-center gap-1 rounded-lg px-3 text-sm text-zinc-900 dark:text-zinc-100 active:opacity-70"
        >
          <SquarePen className="size-4" aria-hidden="true" />
          编辑
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="flex min-h-11 shrink-0 items-center gap-1 rounded-lg px-3 text-sm text-red-500 active:opacity-70"
        >
          <Trash2 className="size-4" aria-hidden="true" />
          删除
        </button>
      </div>
    </Card>
  );
}
