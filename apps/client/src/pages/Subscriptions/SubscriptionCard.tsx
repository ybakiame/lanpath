import { FileText, RefreshCw, SquarePen, Trash2 } from "lucide-react";
import { Button, Chip, Meter } from "@lanpath/ui";
import type { SubscriptionView } from "@lanpath/client-core";
import { formatLabel, formatExpire, usageText, usagePercent, usageColor } from "./utils";

interface SubscriptionCardProps {
  sub: SubscriptionView;
  isActive: boolean;
  busy: boolean;
  refreshing: boolean;
  onActivate: () => void;
  onRefresh: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onSource: () => void;
}

/** Shared responsive subscription card; credentials remain in the edit form and source view. */
export function SubscriptionCard({
  sub,
  isActive,
  busy,
  refreshing,
  onActivate,
  onRefresh,
  onEdit,
  onDelete,
  onSource,
}: SubscriptionCardProps) {
  const disabled = busy || refreshing;
  const percent = usagePercent(sub.userinfo);
  let host = "订阅源";
  try {
    host = new URL(sub.url).hostname;
  } catch {
    /* Legacy malformed URL remains editable. */
  }
  return (
    <article
      className={`flex min-w-0 flex-col gap-4 rounded-2xl border p-4 ${isActive ? "border-accent/50 bg-accent/5" : "border-border bg-surface"}`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="truncate font-semibold text-foreground">{sub.name}</h2>
          <p className="mt-1 truncate text-xs text-muted">{host}</p>
        </div>
        {isActive ? (
          <Chip color="accent" className="shrink-0">
            生效中
          </Chip>
        ) : (
          <Button size="sm" variant="secondary" isDisabled={disabled} onPress={onActivate}>
            使用
          </Button>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
        <span>{sub.node_count} 个节点</span>
        <span aria-hidden="true">·</span>
        <span>{sub.format ? formatLabel(sub.format) : "尚未拉取"}</span>
        <span className="ml-auto">到期 {formatExpire(sub.userinfo?.expire)}</span>
      </div>
      {sub.userinfo && (
        <Meter
          aria-label={`${sub.name} 流量用量`}
          value={percent}
          size="sm"
          color={usageColor(percent)}
          valueLabel={usageText(sub.userinfo)}
        >
          <Meter.Output />
          <Meter.Track>
            <Meter.Fill />
          </Meter.Track>
        </Meter>
      )}
      {sub.error && <output className="line-clamp-2 text-xs text-warning">{sub.error}</output>}
      <div className="flex flex-wrap items-center gap-1 border-t border-border pt-2">
        <Button size="sm" variant="tertiary" className="min-h-11" isDisabled={disabled} onPress={onSource}>
          <FileText className="size-4" aria-hidden="true" />
          原文 / 导出
        </Button>
        <Button
          size="sm"
          variant="tertiary"
          className="ml-auto min-h-11 min-w-11 px-2"
          aria-label={`刷新 ${sub.name}`}
          isDisabled={disabled}
          isPending={refreshing}
          onPress={onRefresh}
        >
          <RefreshCw className="size-4" aria-hidden="true" />
        </Button>
        <Button
          size="sm"
          variant="tertiary"
          className="min-h-11 min-w-11 px-2"
          aria-label={`编辑 ${sub.name}`}
          isDisabled={disabled}
          onPress={onEdit}
        >
          <SquarePen className="size-4" aria-hidden="true" />
        </Button>
        <Button
          size="sm"
          variant="tertiary"
          className="min-h-11 min-w-11 px-2 text-danger"
          aria-label={`删除 ${sub.name}`}
          isDisabled={disabled}
          onPress={onDelete}
        >
          <Trash2 className="size-4" aria-hidden="true" />
        </Button>
      </div>
    </article>
  );
}
