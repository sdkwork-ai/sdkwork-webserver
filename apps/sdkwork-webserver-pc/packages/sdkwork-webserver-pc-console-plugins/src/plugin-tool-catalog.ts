/**
 * Canonical host-tool and contribution identifiers aligned with Codex, Claude Code,
 * Cursor, DeepSeek Harness, Hermes, and ModelKit bundle manifests.
 */

export const PLUGIN_HOST_TOOL_IDS = [
  "cursor",
  "codex",
  "claude_code",
  "gemini",
  "deepseek_harness",
  "hermes",
  "openclaw",
  "opencode",
  "continue",
  "cline",
  "windsurf",
  "aider",
  "sdkwork",
] as const;

export type PluginHostToolId = (typeof PLUGIN_HOST_TOOL_IDS)[number];

export const PLUGIN_CONTRIBUTION_KINDS = [
  "skills",
  "commands",
  "hooks",
  "mcpServers",
  "apps",
  "scripts",
  "agents",
  "rules",
  "tools",
] as const;

export type PluginContributionKind = (typeof PLUGIN_CONTRIBUTION_KINDS)[number];

const HOST_TOOL_SET = new Set<string>(PLUGIN_HOST_TOOL_IDS);
const CONTRIBUTION_SET = new Set<string>(PLUGIN_CONTRIBUTION_KINDS);

export function isPluginHostToolId(value: string): value is PluginHostToolId {
  return HOST_TOOL_SET.has(value);
}

export function isPluginContributionKind(value: string): value is PluginContributionKind {
  return CONTRIBUTION_SET.has(value);
}

export function normalizePluginHostTools(values: readonly string[] | undefined): PluginHostToolId[] {
  if (!values?.length) return [];
  const seen = new Set<PluginHostToolId>();
  for (const value of values) {
    const trimmed = value.trim();
    if (isPluginHostToolId(trimmed)) seen.add(trimmed);
  }
  return PLUGIN_HOST_TOOL_IDS.filter((id) => seen.has(id));
}

export function normalizePluginContributions(
  values: readonly string[] | undefined,
): PluginContributionKind[] {
  if (!values?.length) return [];
  const seen = new Set<PluginContributionKind>();
  for (const value of values) {
    const trimmed = value.trim();
    if (isPluginContributionKind(trimmed)) seen.add(trimmed);
  }
  return PLUGIN_CONTRIBUTION_KINDS.filter((id) => seen.has(id));
}

export function toggleCatalogSelection<T extends string>(
  current: readonly T[],
  value: T,
  selected: boolean,
): T[] {
  if (selected) {
    return current.includes(value) ? [...current] : [...current, value];
  }
  return current.filter((item) => item !== value);
}
