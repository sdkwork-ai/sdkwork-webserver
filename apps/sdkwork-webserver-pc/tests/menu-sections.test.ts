import { groupMenuEntries, MENU_SECTIONS } from "@sdkwork/webserver-pc-commons";
import { describe, expect, it } from "vitest";

/**
 * The sidebar groups a sub-set of the workspace menu under a titled section
 * (AI 生态). These assertions pin the three properties that make the grouping
 * safe: ungrouped entries keep their original order and position, grouped
 * entries keep the caller's `order`-sorted order, and an operator without the
 * grouped resources never sees a dangling section heading.
 */
describe("workspace menu sections", () => {
  it("declares the AI ecosystem section over plugins, plugin categories, skills, and mcp", () => {
    const section = MENU_SECTIONS.find((candidate) => candidate.id === "aiEcosystem");

    expect(section?.labelKey).toBe("menuSection.aiEcosystem");
    // Plugin Categories sits next to Plugins: it is the catalog the plugin
    // create form reads, so the two belong in the same section.
    expect(section?.resources).toEqual(["plugins", "plugin-categories", "skills", "mcp"]);
  });

  it("renders ungrouped entries first and in their supplied order", () => {
    const groups = groupMenuEntries([
      { resource: "apps" },
      { resource: "domains" },
      { resource: "certificates" },
      { resource: "plugins" },
      { resource: "skills" },
      { resource: "mcp" },
    ]);

    expect(groups.map((group) => group.id)).toEqual([null, "aiEcosystem"]);
    expect(groups[0].entries.map((entry) => entry.resource)).toEqual([
      "apps",
      "domains",
      "certificates",
    ]);
    expect(groups[1].entries.map((entry) => entry.resource)).toEqual([
      "plugins",
      "skills",
      "mcp",
    ]);
  });

  it("follows the declared section order, not the incoming entry order", () => {
    const groups = groupMenuEntries([
      { resource: "mcp" },
      { resource: "certificates" },
      { resource: "plugins" },
      { resource: "skills" },
    ]);

    // The section declares plugins → skills → mcp even though mcp arrived first.
    expect(groups[1].entries.map((entry) => entry.resource)).toEqual([
      "plugins",
      "skills",
      "mcp",
    ]);
    expect(groups[0].entries.map((entry) => entry.resource)).toEqual(["certificates"]);
  });

  it("drops an empty section so no dangling heading is rendered", () => {
    const groups = groupMenuEntries([{ resource: "apps" }, { resource: "domains" }]);

    expect(groups.map((group) => group.id)).toEqual([null]);
    expect(groups[0].entries.map((entry) => entry.resource)).toEqual(["apps", "domains"]);
  });

  it("keeps an ungrouped-only workspace in a single leading group", () => {
    const groups = groupMenuEntries([{ resource: "nginx" }, { resource: "servers" }]);

    expect(groups).toHaveLength(1);
    expect(groups[0].id).toBeNull();
    expect(groups[0].labelKey).toBeNull();
  });
});
