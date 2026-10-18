import { useState } from "react";
import { Check } from "lucide-react";
import { matchesRuleSet, ruleSetCategories } from "@lanpath/client-core";
import type { RuleSetOption } from "@lanpath/client-core";
import { Button } from "@lanpath/ui";
import { RuleSetFilters } from "./RuleSetFilters";

/** Filtering never mutates the selected tag set, including unavailable old references. */
export function RuleSetPicker({
  options,
  selected,
  onToggle,
  disabled = false,
}: {
  options: RuleSetOption[];
  selected: string[];
  onToggle: (tag: string) => void;
  disabled?: boolean;
}) {
  const [keyword, setKeyword] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  const visible = options.filter((option) => matchesRuleSet(option, keyword, category));
  return (
    <div className="flex flex-col gap-2">
      <RuleSetFilters
        keyword={keyword}
        onKeyword={setKeyword}
        category={category}
        onCategory={setCategory}
        categories={ruleSetCategories(options)}
      />
      <span className="text-xs text-muted">
        已选 {selected.length} 项 · 当前显示 {visible.length} / {options.length} 项
      </span>
      {selected.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {selected.map((tag) => (
            <Button
              key={tag}
              size="sm"
              variant="secondary"
              isDisabled={disabled}
              onPress={() => onToggle(tag)}
              aria-label={`取消选择 ${tag}`}
              className="max-w-full min-h-11"
            >
              <span className="truncate">{tag} ×</span>
            </Button>
          ))}
        </div>
      )}
      <div className="flex max-h-72 flex-col gap-1.5 overflow-y-auto">
        {visible.map((option) => {
          const checked = selected.includes(option.value);
          return (
            <button
              key={option.value}
              type="button"
              aria-pressed={checked}
              disabled={disabled}
              onClick={() => onToggle(option.value)}
              className={`flex min-h-12 items-center justify-between gap-3 rounded-xl border px-4 py-2 text-left disabled:opacity-60 ${checked ? "border-primary/50 bg-primary/5" : "border-border active:bg-surface-secondary"}`}
            >
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-sm font-medium">{option.hint || option.label}</span>
                <span className="truncate text-xs text-muted">
                  {option.label} · {option.category || "未分类"}
                </span>
              </span>
              {checked && <Check className="size-5 shrink-0 text-primary" aria-hidden="true" />}
            </button>
          );
        })}
      </div>
      {visible.length === 0 && <p className="text-sm text-muted">没有匹配的规则集，试试其他名称或切换到全部分类。</p>}
    </div>
  );
}
