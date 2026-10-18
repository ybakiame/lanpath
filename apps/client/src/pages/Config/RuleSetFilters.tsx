import { inputClassName, SelectField } from "@lanpath/ui";

export function RuleSetFilters({
  keyword,
  onKeyword,
  category,
  onCategory,
  categories,
}: {
  keyword: string;
  onKeyword: (value: string) => void;
  category: string | null;
  onCategory: (value: string | null) => void;
  categories: string[];
}) {
  const names = [...new Set([...categories, ...(category ? [category] : [])])];
  return (
    <div className="flex flex-col gap-2 sm:flex-row">
      <input
        aria-label="搜索规则集名称或 tag"
        placeholder="搜索名称或 tag"
        value={keyword}
        onChange={(e) => onKeyword(e.target.value)}
        className={`${inputClassName} min-h-12 min-w-0 shrink-0 sm:min-h-10 sm:flex-1`}
      />
      <div className="sm:w-48">
        <SelectField
          label="筛选规则集分类"
          value={category === null ? "all" : `category:${category}`}
          onChange={(value) => onCategory(value === "all" ? null : value.slice(9))}
          options={[
            { value: "all", label: "全部分类" },
            { value: "category:", label: "未分类" },
            ...names.map((name) => ({ value: `category:${name}`, label: name })),
          ]}
        />
      </div>
    </div>
  );
}
