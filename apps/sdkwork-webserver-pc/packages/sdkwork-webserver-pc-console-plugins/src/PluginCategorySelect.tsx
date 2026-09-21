import { useMemo, useState } from "react";
import { CheckIcon, SearchIcon } from "./plugin-icons.tsx";
import { usePluginsT } from "./locale.tsx";
import {
  findPluginCategory,
  selectablePluginCategories,
  type PluginCategoryRecord,
} from "./plugin-category.ts";

/**
 * Single-select category picker.
 *
 * Categories are a platform catalog, so this is a *choice among curated
 * options* rather than free text: it renders the offered categories grouped in
 * their declared sort order, keeps the current selection visible even when a
 * search filters it out, and renders an explicit empty state when an operator
 * has not published anything yet.
 */
export function PluginCategorySelect({
  categories,
  value,
  onChange,
  error,
  disabled = false,
}: {
  categories: readonly PluginCategoryRecord[];
  value: string;
  onChange: (categoryId: string) => void;
  error?: string | null;
  disabled?: boolean;
}) {
  const t = usePluginsT();
  const [query, setQuery] = useState("");
  const selectable = useMemo(() => selectablePluginCategories(categories), [categories]);
  const selected = findPluginCategory(categories, value);
  const normalized = query.trim().toLowerCase();
  const visible = normalized
    ? selectable.filter(
        (category) =>
          category.name.toLowerCase().includes(normalized)
          || category.code.includes(normalized),
      )
    : selectable;
  // A search must never hide the current pick, or the operator loses the
  // confirmed selection while narrowing the list.
  const pinned = selected && !visible.some((category) => category.id === selected.id)
    ? selected
    : null;
  const options = pinned ? [pinned, ...visible] : visible;

  if (selectable.length === 0) {
    return (
      <div className="plugin-category-picker">
        <div className="plugin-category-empty">
          <strong>{t("create.category.empty.title")}</strong>
          <p>{t("create.category.empty.description")}</p>
        </div>
        {error ? <p className="skills-console-error plugin-tool-picker-error">{error}</p> : null}
      </div>
    );
  }

  return (
    <div className="plugin-category-picker">
      <div className="plugin-tool-picker-toolbar">
        <span className="plugin-tool-picker-count">
          {selected
            ? t("create.field.categorySelected", { name: selected.name })
            : t("create.field.categorySelected", { name: "—" })}
        </span>
        <div className="plugin-tool-picker-search">
          <SearchIcon size={14} />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("create.field.categorySearch")}
            aria-label={t("create.field.categorySearch")}
            disabled={disabled}
          />
        </div>
      </div>
      {options.length > 0 ? (
        <div className="plugin-category-grid" role="radiogroup" aria-label={t("create.field.category")}>
          {options.map((category) => {
            const checked = category.id === value;
            return (
              <label
                key={category.id}
                className={`plugin-category-option${checked ? " plugin-category-option--selected" : ""}`}
              >
                <input
                  type="radio"
                  name={`plugin-category-${category.id}`}
                  className="plugin-category-option-radio"
                  checked={checked}
                  disabled={disabled}
                  onChange={() => onChange(category.id)}
                />
                <span className="plugin-category-option-body">
                  <span className="plugin-category-option-name">{category.name}</span>
                  <span className="plugin-category-option-code">{category.code}</span>
                  {category.description ? (
                    <span className="plugin-category-option-description">{category.description}</span>
                  ) : null}
                </span>
                <span className="plugin-tool-option-mark" aria-hidden="true">
                  <CheckIcon size={12} strokeWidth={2.4} />
                </span>
              </label>
            );
          })}
        </div>
      ) : (
        <p className="plugin-tool-search-empty">
          {t("create.search.noResults", { query: query.trim() })}
          <button type="button" className="plugin-tool-picker-action" onClick={() => setQuery("")}>
            {t("create.search.clear")}
          </button>
        </p>
      )}
      {error ? <p className="skills-console-error plugin-tool-picker-error">{error}</p> : null}
    </div>
  );
}
