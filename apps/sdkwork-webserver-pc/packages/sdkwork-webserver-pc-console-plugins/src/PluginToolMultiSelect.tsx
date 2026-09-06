import { useState } from "react";
import { CheckIcon, SearchIcon } from "./plugin-icons.tsx";
import { usePluginsT } from "./locale.tsx";
import type { PluginListFilters } from "./plugin-filter.ts";
import {
  PLUGIN_CONTRIBUTION_KINDS,
  PLUGIN_HOST_TOOL_GROUPS,
  PLUGIN_HOST_TOOL_IDS,
  PLUGIN_HOST_TOOL_MONOGRAMS,
  toggleCatalogSelection,
  type PluginContributionKind,
  type PluginHostToolId,
} from "./plugin-tool-catalog.ts";

interface OptionGridProps<T extends string> {
  ariaLabel: string;
  options: readonly T[];
  selected: readonly T[];
  labelKeyPrefix: "tool.host" | "tool.capability";
  onChange: (next: T[]) => void;
  groups?: readonly { id: string; ids: readonly T[] }[];
}

function OptionGrid<T extends string>({
  ariaLabel,
  options,
  selected,
  labelKeyPrefix,
  onChange,
  groups,
}: OptionGridProps<T>) {
  const t = usePluginsT();
  const showMonogram = labelKeyPrefix === "tool.host";
  const available = new Set<T>(options);
  const sections = groups?.length
    ? groups
        .map((group) => ({ id: group.id, ids: group.ids.filter((id) => available.has(id)) }))
        .filter((group) => group.ids.length > 0)
    : [{ id: "", ids: [...options] }];

  function toggleOption(value: T, checked: boolean) {
    onChange(toggleCatalogSelection(selected, value, checked));
  }

  function renderOption(value: T) {
    const checked = selected.includes(value);
    const label = t(`${labelKeyPrefix}.${value}` as never);
    return (
      <label
        key={value}
        className={`plugin-tool-option${checked ? " plugin-tool-option--selected" : ""}`}
      >
        <input
          type="checkbox"
          className="plugin-tool-option-check"
          checked={checked}
          onChange={(event) => toggleOption(value, event.target.checked)}
        />
        {showMonogram ? (
          <span className="plugin-tool-option-mono" aria-hidden="true">
            {PLUGIN_HOST_TOOL_MONOGRAMS[value as PluginHostToolId] ?? ""}
          </span>
        ) : null}
        <span className="plugin-tool-option-label">{label}</span>
        <span className="plugin-tool-option-mark" aria-hidden="true">
          <CheckIcon size={12} strokeWidth={2.4} />
        </span>
      </label>
    );
  }

  return (
    <div className="plugin-tool-grid-root" role="group" aria-label={ariaLabel}>
      {sections.map((section) => (
        <div key={section.id || "all"} className="plugin-tool-group">
          {section.id ? (
            <div className="plugin-tool-group-header">
              <span className="plugin-tool-group-title">{t(`tool.group.${section.id}` as never)}</span>
              <span className="plugin-tool-group-count">{section.ids.length}</span>
            </div>
          ) : null}
          <div className="plugin-tool-grid">{section.ids.map(renderOption)}</div>
        </div>
      ))}
    </div>
  );
}

export function PluginHostToolMultiSelect({
  value,
  onChange,
  error,
}: {
  value: readonly PluginHostToolId[];
  onChange: (next: PluginHostToolId[]) => void;
  error?: string | null;
}) {
  const t = usePluginsT();
  const [query, setQuery] = useState("");
  const normalized = query.trim().toLowerCase();
  const visible = normalized
    ? PLUGIN_HOST_TOOL_IDS.filter(
        (id) =>
          id.includes(normalized)
          || t(`tool.host.${id}` as never).toLowerCase().includes(normalized),
      )
    : PLUGIN_HOST_TOOL_IDS;

  return (
    <div className="plugin-tool-picker">
      <div className="plugin-tool-picker-toolbar">
        <span className="plugin-tool-picker-count">
          {t("create.field.hostToolsSelected", { count: value.length })}
        </span>
        <div className="plugin-tool-picker-search">
          <SearchIcon size={14} />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("create.search.placeholder")}
            aria-label={t("create.search.placeholder")}
          />
        </div>
        <div className="plugin-tool-picker-actions">
          <button
            type="button"
            className="plugin-tool-picker-action"
            onClick={() => onChange([...visible])}
          >
            {t("create.action.selectAllTools")}
          </button>
          <button
            type="button"
            className="plugin-tool-picker-action"
            onClick={() => onChange([])}
          >
            {t("create.action.clearTools")}
          </button>
        </div>
      </div>
      {visible.length > 0 ? (
        <OptionGrid
          ariaLabel={t("create.field.hostTools")}
          options={visible}
          selected={value}
          labelKeyPrefix="tool.host"
          groups={PLUGIN_HOST_TOOL_GROUPS}
          onChange={onChange}
        />
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

export function PluginContributionMultiSelect({
  value,
  onChange,
}: {
  value: readonly PluginContributionKind[];
  onChange: (next: PluginContributionKind[]) => void;
}) {
  const t = usePluginsT();

  return (
    <div className="plugin-tool-picker">
      <div className="plugin-tool-picker-toolbar">
        <span className="plugin-tool-picker-count">
          {t("create.field.capabilitiesSelected", { count: value.length })}
        </span>
        <div className="plugin-tool-picker-actions">
          <button
            type="button"
            className="plugin-tool-picker-action"
            onClick={() => onChange([...PLUGIN_CONTRIBUTION_KINDS])}
          >
            {t("create.action.selectAllCapabilities")}
          </button>
          <button
            type="button"
            className="plugin-tool-picker-action"
            onClick={() => onChange([])}
          >
            {t("create.action.clearCapabilities")}
          </button>
        </div>
      </div>
      <OptionGrid
        ariaLabel={t("create.field.capabilities")}
        options={PLUGIN_CONTRIBUTION_KINDS}
        selected={value}
        labelKeyPrefix="tool.capability"
        onChange={onChange}
      />
    </div>
  );
}

export function PluginListFilterBar({
  filters,
  onChange,
}: {
  filters: PluginListFilters;
  onChange: (next: PluginListFilters) => void;
}) {
  const t = usePluginsT();
  const active = filters.hostTools.length > 0 || filters.capabilities.length > 0;

  return (
    <div className="plugin-filter-bar">
      <div className="plugin-filter-bar-heading">
        <strong>{t("mine.filter.title")}</strong>
        {active ? (
          <button
            type="button"
            className="plugin-filter-clear"
            onClick={() => onChange({ hostTools: [], capabilities: [] })}
          >
            {t("mine.filter.clear")}
          </button>
        ) : null}
      </div>
      <div className="plugin-filter-section">
        <span className="plugin-filter-label">{t("mine.filter.hostTools")}</span>
        <OptionGrid
          ariaLabel={t("mine.filter.hostTools")}
          options={PLUGIN_HOST_TOOL_IDS}
          selected={filters.hostTools}
          labelKeyPrefix="tool.host"
          groups={PLUGIN_HOST_TOOL_GROUPS}
          onChange={(hostTools) => onChange({ ...filters, hostTools })}
        />
      </div>
      <div className="plugin-filter-section">
        <span className="plugin-filter-label">{t("mine.filter.capabilities")}</span>
        <OptionGrid
          ariaLabel={t("mine.filter.capabilities")}
          options={PLUGIN_CONTRIBUTION_KINDS}
          selected={filters.capabilities}
          labelKeyPrefix="tool.capability"
          onChange={(capabilities) => onChange({ ...filters, capabilities })}
        />
      </div>
    </div>
  );
}

export function PluginToolBadges({
  hostTools,
  capabilities,
  maxVisible = 3,
}: {
  hostTools: readonly PluginHostToolId[];
  capabilities?: readonly PluginContributionKind[];
  maxVisible?: number;
}) {
  const t = usePluginsT();
  const labels = [
    ...hostTools.map((id) => t(`tool.host.${id}` as never)),
    ...(capabilities ?? []).map((id) => t(`tool.capability.${id}` as never)),
  ];
  if (labels.length === 0) {
    return <span className="plugin-tool-badges-empty">{t("mine.tools.none")}</span>;
  }
  const visible = labels.slice(0, maxVisible);
  const overflow = labels.length - visible.length;
  return (
    <span className="plugin-tool-badges">
      {visible.map((label) => (
        <span key={label} className="plugin-tool-badge">
          {label}
        </span>
      ))}
      {overflow > 0 ? <span className="plugin-tool-badge plugin-tool-badge--more">+{overflow}</span> : null}
    </span>
  );
}
