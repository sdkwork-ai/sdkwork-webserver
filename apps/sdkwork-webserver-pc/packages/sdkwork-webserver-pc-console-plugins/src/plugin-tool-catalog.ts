/**
 * Canonical host-tool and contribution identifiers aligned with WorkBuddy,
 * CodeBuddy, ZCode, Codex, Claude Code, Cursor, DeepSeek Harness, Hermes,
 * and ModelKit bundle manifests.
 */

export const PLUGIN_HOST_TOOL_IDS = [
  // Popular hosts first — this ordering is canonical for pickers and filters.
  "workbuddy",
  "codebuddy",
  "zcode",
  "cursor",
  "claude_code",
  "codex",
  // IDE / editor integrations.
  "windsurf",
  "trae",
  "copilot",
  "kiro",
  "continue",
  "cline",
  "aider",
  // CLI / harness agents.
  "gemini",
  "deepseek_harness",
  "hermes",
  "openclaw",
  "opencode",
  "amp",
  "sdkwork",
] as const;

export type PluginHostToolId = (typeof PLUGIN_HOST_TOOL_IDS)[number];

export type PluginHostToolGroupId = "popular" | "ide" | "agent";

export interface PluginHostToolGroup {
  id: PluginHostToolGroupId;
  ids: readonly PluginHostToolId[];
}

/**
 * Presentation-only grouping for pickers and filter bars. Groups must cover
 * PLUGIN_HOST_TOOL_IDS exactly once (asserted in plugins-catalog.test.ts).
 */
export const PLUGIN_HOST_TOOL_GROUPS: readonly PluginHostToolGroup[] = [
  {
    id: "popular",
    ids: ["workbuddy", "codebuddy", "zcode", "cursor", "claude_code", "codex"],
  },
  {
    id: "ide",
    ids: ["windsurf", "trae", "copilot", "kiro", "continue", "cline", "aider"],
  },
  {
    id: "agent",
    ids: ["gemini", "deepseek_harness", "hermes", "openclaw", "opencode", "amp", "sdkwork"],
  },
];

/** Short marks rendered inside host-tool chips. */
export const PLUGIN_HOST_TOOL_MONOGRAMS: Record<PluginHostToolId, string> = {
  workbuddy: "Wb",
  codebuddy: "Cb",
  zcode: "Zc",
  cursor: "Cu",
  claude_code: "Cc",
  codex: "Cx",
  windsurf: "Ws",
  trae: "Tr",
  copilot: "Co",
  kiro: "Ki",
  continue: "Ct",
  cline: "Cl",
  aider: "Ai",
  gemini: "Ge",
  deepseek_harness: "Dh",
  hermes: "He",
  openclaw: "Oc",
  opencode: "Oe",
  amp: "Am",
  sdkwork: "Sw",
};

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
