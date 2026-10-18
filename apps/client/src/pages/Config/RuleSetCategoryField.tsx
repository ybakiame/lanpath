import { useId, useState } from "react";
import { inputClassName, SelectField } from "@lanpath/ui";

export function RuleSetCategoryField({
  value,
  onChange,
  categories,
  disabled = false,
}: {
  value: string;
  onChange: (value: string) => void;
  categories: string[];
  disabled?: boolean;
}) {
  const [creating, setCreating] = useState(false);
  const id = useId();
  const names = [...new Set([...categories, value.trim()].filter(Boolean))];
  return (
    <div className="flex flex-col gap-2">
      <span className="text-sm font-medium">归入分类</span>
      <SelectField
        label="规则集分类"
        disabled={disabled}
        value={creating ? "new" : `category:${value}`}
        onChange={(next) => {
          setCreating(next === "new");
          onChange(next === "new" ? "" : next.slice(9));
        }}
        options={[
          { value: "category:", label: "未分类" },
          ...names.map((name) => ({ value: `category:${name}`, label: name })),
          { value: "new", label: "新建分类…" },
        ]}
      />
      {creating && (
        <>
          <label htmlFor={id} className="text-xs text-muted">
            新分类名称
          </label>
          <input
            id={id}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            disabled={disabled}
            maxLength={64}
            placeholder="例如：影音、工作、广告过滤"
            className={`${inputClassName} min-h-12 shrink-0`}
          />
        </>
      )}
    </div>
  );
}
