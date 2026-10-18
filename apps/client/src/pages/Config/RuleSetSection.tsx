import { Card, Chip } from "@lanpath/ui";
import type { CustomRuleSetView } from "@lanpath/client-core";
import { CustomRuleSetCard } from "./CustomRuleSetCard";

/** 分区渲染描述：社区 = 远程 URL（remote），自定义 = 手动 JSON（manual）。 */
interface RuleSetSectionSpec {
  /** 分区标题（按来源类型命名）。 */
  title: string;
  /** 空分区引导文案。 */
  emptyCopy: string;
  sets: CustomRuleSetView[];
}

interface RuleSetSectionProps extends RuleSetSectionSpec {
  /** 正在更新的规则集 id 集合（批量更新时含全部 Remote）。 */
  hideEmpty?: boolean;
  updatingIds: Set<string>;
  selectedIds: Set<string>;
  onSelect: (id: string) => void;
  onEdit: (ruleSet: CustomRuleSetView) => void;
  onDelete: (ruleSet: CustomRuleSetView) => void;
  onUpdate: (ruleSet: CustomRuleSetView) => void;
}

/**
 * 单来源分区（社区 / 自定义）：区头标题 + 计数，条目为 `CustomRuleSetCard`；
 * 空分区渲染简短引导，引导新用户走顶部「添加」入口。
 */
export function RuleSetSection({
  hideEmpty = false,
  title,
  emptyCopy,
  sets,
  updatingIds,
  selectedIds,
  onSelect,
  onEdit,
  onDelete,
  onUpdate,
}: RuleSetSectionProps) {
  if (hideEmpty && sets.length === 0) return null;
  return (
    <section className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="min-w-0 truncate text-sm font-medium text-zinc-900 dark:text-zinc-100">{title}</span>
        <Chip color="default" className="shrink-0">
          {sets.length}
        </Chip>
      </div>
      {sets.length === 0 ? (
        <Card>
          <Card.Content className="flex flex-col items-center justify-center px-6 py-8 text-center">
            <span className="text-sm text-zinc-500 dark:text-zinc-400">{emptyCopy}</span>
          </Card.Content>
        </Card>
      ) : (
        <div className="flex flex-col gap-2">
          {sets.map((ruleSet) => (
            <CustomRuleSetCard
              key={ruleSet.id}
              ruleSet={ruleSet}
              selected={selectedIds.has(ruleSet.id)}
              onSelect={() => onSelect(ruleSet.id)}
              updating={updatingIds.has(ruleSet.id)}
              onEdit={() => onEdit(ruleSet)}
              onDelete={() => onDelete(ruleSet)}
              onUpdate={() => onUpdate(ruleSet)}
            />
          ))}
        </div>
      )}
    </section>
  );
}
