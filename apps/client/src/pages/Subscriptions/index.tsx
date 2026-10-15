import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw, Plus } from "lucide-react";
import { Button, Card, Spinner } from "@lanpath/ui";
import {
  CONFIG_KEY,
  SUBSCRIPTIONS_KEY,
  subscriptionSourceKey,
  addSubscription,
  listSubscriptions,
  markRestartRequired,
  refreshSubscription,
  removeSubscription,
  activateSubscription,
  toErrorMessage,
  toastError,
  toastSuccess,
  toastWarning,
  updateSubscription,
  useClientConfig,
  useProxyStatus,
} from "@lanpath/client-core";
import type { SubscriptionView } from "@lanpath/client-core";
import { SubPageShell } from "../../components/PageShell";
import { SubscriptionDeleteConfirm } from "../../components/mobile/SubscriptionDeleteConfirm";
import { SubscriptionForm } from "./SubscriptionForm";
import type { SubscriptionDraft } from "./SubscriptionForm";
import { SubscriptionSourceSheet } from "./SubscriptionSourceSheet";
import { SubscriptionCard } from "./SubscriptionCard";

/** 双端共享订阅 CRUD、生效与刷新；原文查看与导出。 */
export default function Subscriptions() {
  const queryClient = useQueryClient();
  const { data: config } = useClientConfig();
  const activeId = config?.active_subscription_id ?? null;
  const { data: proxyStatus } = useProxyStatus();
  const coreRunning = proxyStatus?.core_running ?? false;

  const {
    data: subscriptions = [],
    isLoading,
    error: queryError,
  } = useQuery<SubscriptionView[]>({
    queryKey: SUBSCRIPTIONS_KEY,
    queryFn: listSubscriptions,
    refetchInterval: 5000,
  });

  // ---- 局部 UI 状态 ----
  const [previewSub, setPreviewSub] = useState<SubscriptionView | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [editingSub, setEditingSub] = useState<SubscriptionView | null>(null);
  const [pendingDelete, setPendingDelete] = useState<SubscriptionView | null>(null);
  // 当前生效切换的写操作按卡禁用（savingId 单飞，避免同卡并发写）。
  const [savingId, setSavingId] = useState<string | null>(null);
  // 单卡刷新中集合（支持跨卡并发刷新）。
  const [refreshingIds, setRefreshingIds] = useState<Set<string>>(new Set());
  const [refreshingAll, setRefreshingAll] = useState(false);

  const invalidateSubs = () => {
    void queryClient.invalidateQueries({ queryKey: SUBSCRIPTIONS_KEY });
    for (const sub of subscriptions) {
      void queryClient.invalidateQueries({ queryKey: subscriptionSourceKey(sub.id) });
    }
  };
  const invalidateConfig = () => void queryClient.invalidateQueries({ queryKey: CONFIG_KEY });

  // ---- Mutations ----
  const addMutation = useMutation({
    mutationFn: (draft: SubscriptionDraft) =>
      addSubscription({
        name: draft.name,
        url: draft.url,
        user_agent: draft.userAgent || undefined,
      }),
  });
  const editMutation = useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: SubscriptionDraft }) =>
      updateSubscription(id, draft.name, draft.url, editingSub?.profile_id ?? null, draft.userAgent || undefined),
  });
  const removeMutation = useMutation({
    mutationFn: (id: string) => removeSubscription(id),
  });

  const formBusy = addMutation.isPending || editMutation.isPending;
  const refreshAllDisabled = refreshingAll || refreshingIds.size > 0 || subscriptions.length === 0;

  // ---- 操作 ----
  const openAdd = () => {
    setEditingSub(null);
    setFormOpen(true);
  };

  const openEdit = (sub: SubscriptionView) => {
    setEditingSub(sub);
    setFormOpen(true);
  };

  const closeForm = () => {
    setFormOpen(false);
    setEditingSub(null);
  };

  const handleFormSave = async (draft: SubscriptionDraft) => {
    try {
      if (editingSub) {
        await editMutation.mutateAsync({ id: editingSub.id, draft });
        toastSuccess(`订阅「${editingSub.name}」已更新`);
      } else {
        const created = await addMutation.mutateAsync(draft);
        if (created.error) {
          toastWarning("订阅已添加，但首次拉取失败（列表内可查看原因并手动刷新）");
        } else {
          toastSuccess(`订阅「${created.name}」已添加 · ${created.node_count} 个节点`);
        }
      }
      closeForm();
      invalidateSubs();
    } catch (err) {
      toastError(toErrorMessage(err));
    }
  };

  const handleActivate = async (sub: SubscriptionView) => {
    if (savingId !== null || (sub.id === activeId && sub.enabled)) return;
    setSavingId(sub.id);
    try {
      await activateSubscription(sub);
      invalidateSubs();
      // 核心运行中切换生效订阅：节点集变更需重启核心才生效，上报全局重启提示。
      markRestartRequired("subscription", coreRunning);
      toastSuccess(`已将「${sub.name}」设为生效订阅`);
      invalidateConfig();
    } catch (err) {
      toastError(toErrorMessage(err));
    }
    setSavingId(null);
  };

  const handleRefresh = async (sub: SubscriptionView) => {
    if (refreshingAll || refreshingIds.has(sub.id)) return;
    setRefreshingIds((prev) => new Set(prev).add(sub.id));
    try {
      const updated = await refreshSubscription(sub.id);
      if (updated.error) {
        toastWarning(`「${updated.name}」刷新失败，已保留上次数据`);
      } else {
        toastSuccess(`已刷新「${updated.name}」 · ${updated.node_count} 个节点`);
      }
      invalidateSubs();
    } catch (err) {
      toastError(toErrorMessage(err));
    }
    setRefreshingIds((prev) => {
      const next = new Set(prev);
      next.delete(sub.id);
      return next;
    });
  };

  const handleRefreshAll = async () => {
    if (refreshAllDisabled) return;
    setRefreshingAll(true);
    let ok = 0;
    let fail = 0;
    for (const sub of subscriptions) {
      try {
        const updated = await refreshSubscription(sub.id);
        if (updated.error) {
          fail += 1;
        } else {
          ok += 1;
        }
      } catch {
        fail += 1;
      }
    }
    invalidateSubs();
    setRefreshingAll(false);
    if (fail === 0) {
      toastSuccess(`已刷新全部 ${subscriptions.length} 个订阅`);
    } else {
      toastWarning(`刷新完成：${ok} 个成功，${fail} 个失败（请检查网络后重试）`);
    }
  };

  const handleDeleteConfirm = async () => {
    const sub = pendingDelete;
    if (!sub) return;
    const wasActive = sub.id === activeId;
    setPendingDelete(null);
    try {
      await removeMutation.mutateAsync(sub.id);
      invalidateSubs();
      invalidateConfig();
      if (wasActive) {
        toastWarning("生效订阅已删除，请重新选择");
      } else {
        toastSuccess(`已删除订阅「${sub.name}」`);
      }
    } catch (err) {
      toastError(toErrorMessage(err));
    }
  };

  const showList = !isLoading && !queryError && subscriptions.length > 0;

  return (
    <SubPageShell
      title="订阅管理"
      action={
        <Button variant="tertiary" className="h-11 shrink-0 px-2 font-semibold" onPress={openAdd}>
          <Plus className="size-5" aria-hidden="true" />
          添加
        </Button>
      }
    >
      {isLoading && (
        <Card>
          <Card.Content className="flex flex-col items-center justify-center gap-3 py-12 text-center">
            <Spinner aria-hidden="true" />
            <span className="text-sm text-muted">正在加载订阅…</span>
          </Card.Content>
        </Card>
      )}

      {!isLoading && queryError && (
        <Card>
          <Card.Content className="flex flex-col items-center gap-2 py-8 text-center">
            <span className="text-sm text-muted">订阅列表加载失败</span>
            <span className="text-xs text-muted">{toErrorMessage(queryError)}</span>
          </Card.Content>
        </Card>
      )}

      {!isLoading && !queryError && subscriptions.length === 0 && (
        <div className="flex flex-1 flex-col items-center justify-center gap-5 pb-16 text-center">
          <div className="flex flex-col gap-1">
            <span className="text-base font-medium text-foreground">添加你的第一个订阅</span>
            <span className="text-sm text-muted">粘贴机场订阅链接，拉取节点并用于首页启动代理</span>
          </div>
          <Button variant="primary" size="lg" className="min-h-12 px-8" onPress={openAdd}>
            添加订阅
          </Button>
        </div>
      )}

      {showList && (
        <>
          <div className="flex items-center justify-between">
            <span className="pl-1 text-xs text-muted">共 {subscriptions.length} 个订阅源</span>
            <button
              type="button"
              aria-label="刷新全部订阅"
              disabled={refreshAllDisabled}
              onClick={() => void handleRefreshAll()}
              className="flex size-11 shrink-0 items-center justify-center rounded-lg text-zinc-500 transition-opacity active:opacity-70 disabled:cursor-default disabled:opacity-40 dark:text-zinc-400"
            >
              <RefreshCw className={`size-5 ${refreshingAll ? "animate-spin" : ""}`} aria-hidden="true" />
            </button>
          </div>
          <div className="grid gap-4 xl:grid-cols-2">
            {subscriptions.map((sub) => (
              <SubscriptionCard
                key={sub.id}
                sub={sub}
                isActive={sub.id === activeId}
                busy={savingId !== null || refreshingAll || formBusy || removeMutation.isPending}
                refreshing={refreshingIds.has(sub.id)}
                onActivate={() => void handleActivate(sub)}
                onRefresh={() => void handleRefresh(sub)}
                onEdit={() => openEdit(sub)}
                onDelete={() => setPendingDelete(sub)}
                onSource={() => setPreviewSub(sub)}
              />
            ))}
          </div>
        </>
      )}

      <SubscriptionForm
        isOpen={formOpen}
        editing={editingSub}
        busy={formBusy}
        onClose={closeForm}
        onSave={(draft) => void handleFormSave(draft)}
      />
      <SubscriptionDeleteConfirm
        sub={pendingDelete}
        isActive={pendingDelete?.id === activeId}
        busy={removeMutation.isPending}
        onClose={() => setPendingDelete(null)}
        onConfirm={() => void handleDeleteConfirm()}
      />
      {previewSub && <SubscriptionSourceSheet sub={previewSub} onClose={() => setPreviewSub(null)} />}
    </SubPageShell>
  );
}
