import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw, Plus, Sparkles } from "lucide-react";
import { InlineAlert } from "@lanpath/ui";
import { BottomSheet, Button, Card, ConfirmDialog, Spinner } from "@lanpath/ui";
import {
  BASELINE_VIEW_KEY,
  baselineViewGet,
  buildSaveInput,
  LOCAL_OVERRIDE_KEY,
  localOverrideGet,
  localOverrideSave,
  localOverrideSetRuleSetCategories,
  localOverrideUpdateRuleSet,
  localOverrideUpdateRulesetsNow,
  markRestartRequired,
  toastError,
  toastSuccess,
  toastWarning,
  toErrorMessage,
  useProxyStatus,
} from "@lanpath/client-core";
import type { BaselineView, CustomRuleSetInput, CustomRuleSetView, LocalOverrideView } from "@lanpath/client-core";
import { useNavigate } from "react-router-dom";
import { SubPageShell } from "../../components/PageShell";
import { asArray, isLocalOverrideView } from "@lanpath/client-core";
import { RuleSetSection } from "./RuleSetSection";
import { RuleSetFilters } from "./RuleSetFilters";
import { RuleSetCategoryField } from "./RuleSetCategoryField";
import { matchesRuleSet, ruleSetCategories } from "@lanpath/client-core";
import { RuleSetFormSheet } from "./RuleSetFormSheet";

/**
 * 规则集管理子页（ADR-0003 M5.4 拆分，路由 `/config/route/rulesets`）。
 *
 * 自「废弃内置规则集订阅」起页面只管理**用户自控的规则集**，并按**来源类型**分区：
 * - 「社区规则集」区：`source.kind === "remote"`（远程 URL，如 geoip/geosite 社区资源）；
 * - 「自定义规则集」区：`source.kind === "manual"`（手动 JSON）。
 *
 * 自「规则集移除 enabled」起规则集是**纯资源**：
 * - 卡片不再有启停 Switch（名称 / tag / 来源 chip / 缓存状态 / 本地与远程更新时间 / 编辑 / 删除）；
 * - 顶部「立即更新」智能更新**全部** Remote（`localOverrideUpdateRulesetsNow`：
 *   进行条件 GET/内容比较跳过未变更项），toast 汇总「更新 N，已最新 M，失败 K」；
 * - 每张 Remote 卡片另有单卡更新按钮（`localOverrideUpdateRuleSet`），同样智能跳过；
 * - 更新进行中由 `updatingIds` 驱动：单卡仅该卡、批量全部 Remote 卡同时显示旋转
 *   更新图标与 indeterminate 进度条；
 * - invalidate 触发重拉即见 cached / last_updated / remote_updated_at 变化；
 * - 是否注入仍由引用它的规则决定（引用它的规则被注入时才注入该规则集）。
 */
export default function RuleSetsPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: status } = useProxyStatus();
  // 本地规则 / 规则集在核心启动时注入，运行中变更不热更新：核心运行中成功 toast 追加「重启代理后生效」。
  const coreRunning = status?.core_running ?? false;
  const {
    data: rawOverride,
    isLoading: overrideLoading,
    error: overrideError,
  } = useQuery<LocalOverrideView>({
    queryKey: LOCAL_OVERRIDE_KEY,
    queryFn: localOverrideGet,
  });
  // 内置 CN 分流基线（纯静态只读）；始终展示，用户规则集为空时仍可见。
  const { data: baseline } = useQuery<BaselineView>({
    queryKey: BASELINE_VIEW_KEY,
    queryFn: baselineViewGet,
    staleTime: Infinity,
    retry: false,
  });

  // 结构守卫：缓存残留异构形态（历史复合查询）时视为未加载，渲染空态而非崩溃。
  const overrideData = isLocalOverrideView(rawOverride) ? rawOverride : null;
  const customSets = asArray(overrideData?.custom_rule_sets);
  // 分区 = 来源类型：remote（社区）与 manual（自定义）。
  const [keyword, setKeyword] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [organizeOpen, setOrganizeOpen] = useState(false);
  const [targetCategory, setTargetCategory] = useState("");
  const categoryNames = ruleSetCategories(customSets);
  const filteredSets = customSets.filter((rs) => matchesRuleSet(rs, keyword, category));
  const toggleSelected = (id: string) =>
    setSelectedIds((previous) => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const remoteSets = customSets.filter((rs) => rs.source.kind === "remote");
  const invalidate = () => void queryClient.invalidateQueries({ queryKey: LOCAL_OVERRIDE_KEY });

  // ---- 局部 UI 状态 ----
  const [formOpen, setFormOpen] = useState(false);
  const [editingSet, setEditingSet] = useState<CustomRuleSetView | null>(null);
  const [pendingDelete, setPendingDelete] = useState<CustomRuleSetView | null>(null);
  const [updating, setUpdating] = useState(false);
  // 正在更新的规则集 id 集合：单卡仅含该 id；批量含全部 Remote（供卡片旋转 / 进度条）。
  const [updatingIds, setUpdatingIds] = useState<Set<string>>(new Set());

  const toastRuleSaved = (base: string) => {
    markRestartRequired("rulesets", coreRunning);
    toastSuccess(coreRunning ? `${base}，重启代理后生效` : base);
  };

  /** 规则集段整段替换落盘（其余段透传当前视图）。 */
  const persistCustom = async (next: CustomRuleSetInput[]): Promise<boolean> => {
    if (!overrideData) return false;
    try {
      await localOverrideSave({ ...buildSaveInput(overrideData), custom_rule_sets: next });
      invalidate();
      return true;
    } catch (err) {
      toastError(toErrorMessage(err));
      invalidate();
      return false;
    }
  };

  const organize = useMutation({
    mutationFn: () => localOverrideSetRuleSetCategories([...selectedIds], targetCategory.trim()),
    onSuccess: (next) => {
      queryClient.setQueryData(LOCAL_OVERRIDE_KEY, next);
      setSelectedIds(new Set());
      setOrganizeOpen(false);
      toastSuccess("分类已整理");
    },
    onError: (error) => toastError(toErrorMessage(error)),
  });

  // ---- 立即更新（全部 Remote，智能跳过）：返回 updated / skipped / failed 汇总 ----
  const handleUpdateNow = async (): Promise<boolean> => {
    if (updating || updatingIds.size > 0) return false;
    if (remoteSets.length === 0) {
      toastWarning("暂无远程规则集可更新");
      return true;
    }
    setUpdating(true);
    // 批量更新时全部 Remote 卡进入更新态（旋转 + 进度条）。
    setUpdatingIds(new Set(remoteSets.map((rs) => rs.id)));
    try {
      const { updated, skipped, failed } = await localOverrideUpdateRulesetsNow();
      const suffix = coreRunning ? "，重启代理后生效" : "";
      const summary = `更新 ${updated}，已最新 ${skipped}，失败 ${failed}${suffix}`;
      if (failed > 0) {
        toastWarning(summary);
      } else {
        toastSuccess(summary);
      }
      invalidate();
      return true;
    } catch (err) {
      toastError(toErrorMessage(err));
      invalidate();
      return false;
    } finally {
      setUpdating(false);
      setUpdatingIds(new Set());
    }
  };

  // ---- 单卡更新（同样智能跳过）：updated=已更新 / skipped=已是最新 / failed=失败 ----
  const handleUpdateOne = async (ruleSet: CustomRuleSetView): Promise<void> => {
    if (updating || updatingIds.size > 0) return;
    setUpdatingIds(new Set([ruleSet.id]));
    const label = ruleSet.name.trim() || ruleSet.tag;
    try {
      const { updated, skipped } = await localOverrideUpdateRuleSet(ruleSet.id);
      if (updated > 0) {
        toastSuccess(coreRunning ? `规则集「${label}」已更新，重启代理后生效` : `规则集「${label}」已更新`);
      } else if (skipped > 0) {
        toastSuccess(`规则集「${label}」已是最新`);
      } else {
        toastWarning(`规则集「${label}」更新失败`);
      }
      invalidate();
    } catch (err) {
      toastError(toErrorMessage(err));
      invalidate();
    } finally {
      setUpdatingIds(new Set());
    }
  };

  // ---- 规则集 增改删 ----
  const openAdd = () => {
    setEditingSet(null);
    setFormOpen(true);
  };

  const openEdit = (ruleSet: CustomRuleSetView) => {
    setEditingSet(ruleSet);
    setFormOpen(true);
  };

  const handleSaveCustom = async (input: CustomRuleSetInput): Promise<boolean> => {
    if (!overrideData) return false;
    const previous = overrideData.custom_rule_sets.find((rs) => rs.id === input.id);
    const exists = previous !== undefined;
    const runtimeChanged =
      !previous || previous.tag !== input.tag || JSON.stringify(previous.source) !== JSON.stringify(input.source);
    const next = exists
      ? overrideData.custom_rule_sets.map((rs) => (rs.id === input.id ? input : rs))
      : [...overrideData.custom_rule_sets, input];
    const ok = await persistCustom(next);
    if (ok) {
      const base = exists ? "规则集已更新" : "规则集已添加";
      let text = coreRunning && runtimeChanged ? `${base}，重启代理后生效` : base;
      if (!exists && input.source.kind === "remote") {
        text += "；点击「立即更新」下载规则集";
      }
      toastSuccess(text);
    }
    return ok;
  };

  /** 恢复内置规则集：按基线模板补回缺失的内置条目（已存在的用户条目不动）。 */
  const handleRestoreBuiltin = async () => {
    if (!overrideData || !baseline) return;
    const existing = new Set(overrideData.custom_rule_sets.map((rs) => rs.id));
    const missing = baseline.rule_sets.filter((rs) => !existing.has(rs.id));
    if (missing.length === 0) {
      toastSuccess("内置规则集已完整");
      return;
    }
    const restored: CustomRuleSetInput[] = missing.map((rs) => ({
      id: rs.id,
      name: rs.name,
      tag: rs.tag,
      source: { kind: "remote", url: rs.url, format: "binary" },
      last_updated: 0,
      builtin: true,
    }));
    if (await persistCustom([...overrideData.custom_rule_sets, ...restored])) {
      toastSuccess(`已恢复 ${missing.length} 个内置规则集，点击「立即更新」下载内容`);
    }
  };

  const handleDeleteConfirm = async () => {
    const target = pendingDelete;
    if (!target || !overrideData) return;
    setPendingDelete(null);
    const next = overrideData.custom_rule_sets.filter((rs) => rs.id !== target.id);
    if (await persistCustom(next)) {
      toastRuleSaved(`已删除规则集「${target.tag}」`);
    }
  };

  return (
    <SubPageShell title="规则集管理">
      {overrideLoading && !overrideData && (
        <Card>
          <Card.Content className="flex flex-col items-center justify-center gap-3 py-12 text-center">
            <Spinner aria-hidden="true" />
            <span className="text-sm text-zinc-500 dark:text-zinc-400">正在加载规则集…</span>
          </Card.Content>
        </Card>
      )}

      {!overrideData && !overrideLoading && overrideError && (
        <Card>
          <Card.Content className="flex flex-col items-center gap-2 py-8 text-center">
            <InlineAlert kind="danger" title="加载失败">
              {toErrorMessage(overrideError)}
            </InlineAlert>
          </Card.Content>
        </Card>
      )}

      {!overrideData && !overrideLoading && !overrideError && (
        <Card>
          <Card.Content className="flex flex-col items-center gap-3 py-12 text-center">
            <span className="text-sm text-zinc-500 dark:text-zinc-400">规则集数据不可用</span>
            <Button variant="secondary" className="min-h-11 shrink-0 px-4" onPress={() => void invalidate()}>
              重新加载
            </Button>
          </Card.Content>
        </Card>
      )}

      {overrideData && (
        <div className="flex flex-col gap-4">
          {/* 顶部说明 */}
          <span className="text-xs text-zinc-500 dark:text-zinc-400">
            远程规则集缓存到本地后使用；手动 JSON 保存即写入本地文件。分类仅用于整理，不改变分流规则。
          </span>

          {/* 顶部统一操作：添加（单一入口）+ 立即更新 */}
          <div className="flex items-center gap-2">
            <Button aria-label="添加规则集" variant="primary" className="h-11 min-w-0 flex-1 px-4" onPress={openAdd}>
              <Plus className="size-4" aria-hidden="true" />
              添加
            </Button>
            <Button
              variant="secondary"
              className="h-11 min-w-0 flex-1 px-4"
              isDisabled={updating || updatingIds.size > 0}
              isPending={updating}
              aria-label="检查并更新全部规则集"
              onPress={() => void handleUpdateNow()}
            >
              <RefreshCw className="size-4" aria-hidden="true" />
              更新
            </Button>
            <Button
              variant="secondary"
              className="h-11 shrink-0 px-3"
              onPress={() => navigate("/config/route/rulesets/market")}
            >
              <Sparkles className="size-4 pr-1" aria-hidden="true" />
              市场
            </Button>
            <Button
              variant="secondary"
              className="h-11 shrink-0 px-3"
              isDisabled={!baseline}
              onPress={() => void handleRestoreBuiltin()}
            >
              恢复默认
            </Button>
          </div>

          <RuleSetFilters
            keyword={keyword}
            onKeyword={setKeyword}
            category={category}
            onCategory={setCategory}
            categories={categoryNames}
          />
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs text-muted">
              显示 {filteredSets.length} / {customSets.length} 项 · 已选 {selectedIds.size} 项
            </span>
            <Button
              variant="tertiary"
              size="sm"
              onPress={() => setSelectedIds(new Set(filteredSets.map((rs) => rs.id)))}
            >
              选择当前结果
            </Button>
            <Button
              variant="tertiary"
              size="sm"
              onPress={() => setSelectedIds(new Set())}
              isDisabled={selectedIds.size === 0}
            >
              清空选择
            </Button>
            <Button
              variant="secondary"
              size="sm"
              isDisabled={selectedIds.size === 0 || updatingIds.size > 0}
              onPress={() => {
                setTargetCategory("");
                setOrganizeOpen(true);
              }}
            >
              整理分类
            </Button>
          </div>
          {filteredSets.length === 0 && (
            <p className="text-sm text-muted">没有匹配的规则集，试试其他名称或全部分类。</p>
          )}
          {/* 社区规则集：远程 URL（remote） */}
          <RuleSetSection
            title="社区规则集"
            emptyCopy="添加远程 URL 规则集，如 geoip/geosite 社区资源"
            sets={filteredSets.filter((rs) => rs.source.kind === "remote")}
            hideEmpty={keyword.trim() !== "" || category !== null}
            selectedIds={selectedIds}
            onSelect={toggleSelected}
            updatingIds={updatingIds}
            onEdit={openEdit}
            onDelete={setPendingDelete}
            onUpdate={(ruleSet) => void handleUpdateOne(ruleSet)}
          />

          {/* 自定义规则集：手动 JSON（manual） */}
          <RuleSetSection
            title="自定义规则集"
            emptyCopy="手动输入 JSON 规则内容"
            sets={filteredSets.filter((rs) => rs.source.kind === "manual")}
            hideEmpty={keyword.trim() !== "" || category !== null}
            selectedIds={selectedIds}
            onSelect={toggleSelected}
            updatingIds={updatingIds}
            onEdit={openEdit}
            onDelete={setPendingDelete}
            onUpdate={(ruleSet) => void handleUpdateOne(ruleSet)}
          />
        </div>
      )}

      <BottomSheet
        opened={organizeOpen}
        onClose={() => {
          if (!organize.isPending) setOrganizeOpen(false);
        }}
        title={`整理 ${selectedIds.size} 个规则集的分类`}
        footer={
          <Button isPending={organize.isPending} onPress={() => organize.mutate()}>
            保存分类
          </Button>
        }
      >
        <RuleSetCategoryField
          key={organizeOpen ? "open" : "closed"}
          value={targetCategory}
          onChange={setTargetCategory}
          categories={categoryNames}
          disabled={organize.isPending}
        />
      </BottomSheet>
      <RuleSetFormSheet
        categories={categoryNames}
        isOpen={formOpen}
        editing={editingSet}
        onClose={() => setFormOpen(false)}
        onSave={(ruleSet) => handleSaveCustom(ruleSet)}
      />
      <ConfirmDialog
        opened={pendingDelete !== null}
        title="删除规则集"
        danger
        confirmText="删除"
        onConfirm={() => void handleDeleteConfirm()}
        onClose={() => setPendingDelete(null)}
      >
        <p className="break-words">
          确定删除规则集「{pendingDelete ? pendingDelete.name.trim() || pendingDelete.tag : ""}
          」吗？ 将同时清理其缓存文件；引用该 tag 的规则需一并调整。
        </p>
      </ConfirmDialog>
    </SubPageShell>
  );
}
