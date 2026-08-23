import type { PluginContributionKind, PluginHostToolId } from "./plugin-tool-catalog.ts";
import type { PluginRecord } from "./plugin-model.ts";

export interface PluginListFilters {
  hostTools: readonly PluginHostToolId[];
  capabilities: readonly PluginContributionKind[];
}

export const EMPTY_PLUGIN_LIST_FILTERS: PluginListFilters = {
  hostTools: [],
  capabilities: [],
};

export function hasActivePluginFilters(filters: PluginListFilters): boolean {
  return filters.hostTools.length > 0 || filters.capabilities.length > 0;
}

export function filterPluginRecords(
  items: readonly PluginRecord[],
  filters: PluginListFilters,
): PluginRecord[] {
  let result = items;
  if (filters.hostTools.length > 0) {
    result = result.filter((item) =>
      filters.hostTools.some((tool) => item.supportedHostTools.includes(tool)),
    );
  }
  if (filters.capabilities.length > 0) {
    result = result.filter((item) =>
      filters.capabilities.some((kind) => item.contributedCapabilities.includes(kind)),
    );
  }
  return [...result];
}
