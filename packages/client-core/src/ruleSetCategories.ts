/** User categories are independent of GeoIP/GeoSite source formats. */
export function ruleSetCategories(items: readonly { category?: string }[]): string[] {
  return [...new Set(items.map((item) => (item.category ?? "").trim()).filter(Boolean))].sort((a, b) =>
    a.localeCompare(b),
  );
}

export function matchesRuleSet(
  item: { category?: string; name?: string; tag?: string; label?: string; value?: string; hint?: string },
  keyword: string,
  category: string | null,
): boolean {
  if (category !== null && (item.category ?? "") !== category) return false;
  const query = keyword.trim().toLocaleLowerCase();
  return (
    !query ||
    [item.name, item.tag, item.label, item.value, item.hint].some((text) => text?.toLocaleLowerCase().includes(query))
  );
}
