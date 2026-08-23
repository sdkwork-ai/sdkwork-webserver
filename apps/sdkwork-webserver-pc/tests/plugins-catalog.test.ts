import { describe, expect, it } from "vitest";

import {
  parsePluginCatalog,
  serializePluginCatalog,
  isValidPluginKey,
  createPluginId,
  normalizePluginRecord,
  type PluginRecord,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-model.ts";
import {
  upsertPluginRecord,
  removePluginRecord,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-catalog.ts";
import {
  filterPluginRecords,
  hasActivePluginFilters,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-filter.ts";
import {
  normalizePluginHostTools,
  normalizePluginContributions,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/plugin-tool-catalog.ts";

function sample(overrides: Partial<PluginRecord> = {}): PluginRecord {
  const now = "2026-08-22T00:00:00.000Z";
  return {
    id: createPluginId(),
    pluginKey: "plugin.workspace.sample",
    displayName: "Sample",
    summary: "",
    version: "1.0.0",
    supportedHostTools: ["cursor", "codex"],
    contributedCapabilities: ["skills", "hooks"],
    sourceKind: "git",
    gitRepository: "https://github.com/org/plugin.git",
    gitRef: "main",
    artifactRef: null,
    checksumSha256: null,
    archiveFileName: null,
    status: "active",
    createdAt: now,
    updatedAt: now,
    ...overrides,
  };
}

describe("plugin catalog model", () => {
  it("validates plugin keys", () => {
    expect(isValidPluginKey("plugin.workspace.sample")).toBe(true);
    expect(isValidPluginKey("plugin.a")).toBe(false);
    expect(isValidPluginKey("skill.workspace.sample")).toBe(false);
  });

  it("round-trips catalog snapshots and upserts by plugin key", () => {
    const first = sample({ id: "1", pluginKey: "plugin.workspace.one", updatedAt: "2026-08-22T01:00:00.000Z" });
    const second = sample({ id: "2", pluginKey: "plugin.workspace.two", updatedAt: "2026-08-22T02:00:00.000Z" });
    const replaced = sample({
      id: "3",
      pluginKey: "plugin.workspace.one",
      displayName: "Replaced",
      supportedHostTools: ["claude_code"],
      updatedAt: "2026-08-22T03:00:00.000Z",
    });
    const stored = serializePluginCatalog([first, second]);
    const loaded = parsePluginCatalog(stored);
    expect(loaded).toHaveLength(2);
    expect(loaded[0]?.supportedHostTools).toEqual(["cursor", "codex"]);

    const next = upsertPluginRecord(loaded, replaced);
    expect(next).toHaveLength(2);
    expect(next[0]?.displayName).toBe("Replaced");
    expect(next[0]?.supportedHostTools).toEqual(["claude_code"]);
    expect(next.find((item) => item.pluginKey === "plugin.workspace.one")?.id).toBe("3");
    expect(removePluginRecord(next, "3")).toHaveLength(1);
  });

  it("migrates legacy records without tool fields", () => {
    const legacy = {
      id: "legacy-1",
      pluginKey: "plugin.workspace.legacy",
      displayName: "Legacy",
      summary: "",
      version: "1.0.0",
      sourceKind: "git",
      gitRepository: "https://github.com/org/legacy.git",
      gitRef: "main",
      status: "active",
      createdAt: "2026-08-22T00:00:00.000Z",
      updatedAt: "2026-08-22T00:00:00.000Z",
    };
    const normalized = normalizePluginRecord(legacy);
    expect(normalized?.supportedHostTools).toEqual([]);
    expect(normalized?.contributedCapabilities).toEqual([]);

    const loaded = parsePluginCatalog(JSON.stringify({ version: 1, items: [legacy] }));
    expect(loaded[0]?.supportedHostTools).toEqual([]);
  });

  it("drops unknown tool identifiers during normalization", () => {
    expect(normalizePluginHostTools(["cursor", "unknown_host", "codex"])).toEqual(["cursor", "codex"]);
    expect(normalizePluginContributions(["skills", "not_real", "tools"])).toEqual(["skills", "tools"]);
  });
});

describe("plugin list filters", () => {
  it("filters by host tools and capabilities with OR semantics within each group", () => {
    const cursorOnly = sample({
      id: "a",
      pluginKey: "plugin.workspace.a",
      supportedHostTools: ["cursor"],
      contributedCapabilities: ["skills"],
    });
    const codexHooks = sample({
      id: "b",
      pluginKey: "plugin.workspace.b",
      supportedHostTools: ["codex"],
      contributedCapabilities: ["hooks"],
    });
    const both = sample({
      id: "c",
      pluginKey: "plugin.workspace.c",
      supportedHostTools: ["cursor", "deepseek_harness"],
      contributedCapabilities: ["skills", "tools"],
    });
    const items = [cursorOnly, codexHooks, both];

    expect(filterPluginRecords(items, { hostTools: ["deepseek_harness"], capabilities: [] })).toHaveLength(1);
    expect(filterPluginRecords(items, { hostTools: ["cursor"], capabilities: [] })).toHaveLength(2);
    expect(filterPluginRecords(items, { hostTools: [], capabilities: ["tools"] })).toHaveLength(1);
    expect(filterPluginRecords(items, { hostTools: ["codex"], capabilities: ["hooks"] })).toHaveLength(1);
    expect(hasActivePluginFilters({ hostTools: [], capabilities: [] })).toBe(false);
    expect(hasActivePluginFilters({ hostTools: ["cursor"], capabilities: [] })).toBe(true);
  });
});
