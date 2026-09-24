import { groupMenuEntries, MENU_SECTIONS } from "@sdkwork/webserver-pc-commons";
import { describe, expect, it } from "vitest";

/**
 * The sidebar groups a sub-set of the workspace menu under a titled section
 * (交付 / AI 生态 / 云接入 / 数据统计 / 审计追踪). These assertions pin the properties that make the grouping
 * safe: ungrouped entries keep their original order and position, grouped
 * entries follow the section's own declared order rather than the caller's
 * `order`-sorted one, and an operator without the grouped resources never sees
 * a dangling section heading.
 */
describe("workspace menu sections", () => {
  it("declares the delivery section over applications, domains, and certificates", () => {
    const section = MENU_SECTIONS.find((candidate) => candidate.id === "delivery");

    expect(section?.labelKey).toBe("menuSection.delivery");
    // Applications (bridged from sdkwork-deployments) plus the two domain menus:
    // the per-user pair the console bridges from the Deployments plane and the
    // tenant-level pair the operations surface reads off the Web Server's own
    // planes. They need a section rather than relying on `order` because `order`
    // only ranks an entry within its own surface: `certificates` is the second
    // entry of its module, so on the operations surface it sorts below every
    // first entry and would render after Audit — several entries away from the
    // `domains` inventory it belongs to. A surface supplying only some of the
    // three renders just what it has; the grouping drops the rest.
    expect(section?.resources).toEqual(["apps", "domains", "certificates"]);
  });

  it("declares the AI ecosystem section over plugins, plugin categories, skills, mcp, and sandbox instances", () => {
    const section = MENU_SECTIONS.find((candidate) => candidate.id === "aiEcosystem");

    expect(section?.labelKey).toBe("menuSection.aiEcosystem");
    // Plugin Categories sits next to Plugins: it is the catalog the plugin
    // create form reads, so the two belong in the same section. VM
    // instances close the list because a VM is the runtime the skills and
    // MCP servers above it execute in — an asset the account owns, not a key left
    // ungrouped, which would render it above the Dashboard.
    expect(section?.resources).toEqual([
      "plugins",
      "plugin-categories",
      "skills",
      "mcp",
      "sandbox-instances",
    ]);
  });

  it("declares the cloud access section over the cloud accounts only", () => {
    const section = MENU_SECTIONS.find((candidate) => candidate.id === "cloudAccess");

    expect(section?.labelKey).toBe("menuSection.cloudAccess");
    // The provider accounts federated from SDKWork IAM are neither a delivery
    // target nor an AI asset; left ungrouped they rendered above the Dashboard.
    // The heading must differ from the label of the single entry it holds
    // (云接入 vs 云账号), because the sidebar draws both.
    expect(section?.resources).toEqual(["cloud-accounts"]);
  });

  it("renders the cloud access group after the AI assets and before the readings", () => {
    const groups = groupMenuEntries([
      { resource: "cloud-accounts" },
      { resource: "traffic-usage" },
      { resource: "skills" },
      { resource: "apps" },
    ]);

    // Every supplied entry is claimed, so the leading group is emitted empty.
    expect(groups.map((group) => group.id)).toEqual([
      null,
      "delivery",
      "aiEcosystem",
      "cloudAccess",
      "dataStatistics",
    ]);
    expect(groups[3].entries.map((entry) => entry.resource)).toEqual(["cloud-accounts"]);
  });

  it("declares the data statistics section over the traffic reading only", () => {
    const section = MENU_SECTIONS.find((candidate) => candidate.id === "dataStatistics");

    expect(section?.labelKey).toBe("menuSection.dataStatistics");
    // The Dashboard entry is deliberately absent: the overview leads the
    // sidebar in the unlabelled leading group rather than belonging to a
    // grouping of readings. A section that claimed it would move the landing
    // entry down into the measurement group.
    expect(section?.resources).toEqual(["traffic-usage"]);
  });

  it("places the traffic reading in its own section and the overview ungrouped", () => {
    const groups = groupMenuEntries([
      { resource: "dashboard" },
      { resource: "traffic-usage" },
      { resource: "apps" },
    ]);

    expect(groups.map((group) => group.id)).toEqual([null, "delivery", "dataStatistics"]);
    expect(groups[0].entries.map((entry) => entry.resource)).toEqual(["dashboard"]);
    expect(groups[2].entries.map((entry) => entry.resource)).toEqual(["traffic-usage"]);
  });

  it("declares the audit section over the audit trail only", () => {
    const section = MENU_SECTIONS.find((candidate) => candidate.id === "audit");

    expect(section?.labelKey).toBe("menuSection.audit");
    expect(section?.resources).toEqual(["audit"]);
    // The trail is the record the groups above it are read back through rather
    // than an asset group of its own, so it is declared last and closes the
    // sidebar. Left ungrouped it took the leading, unlabelled slot — **above
    // the Dashboard** — which is neither its place nor the position the
    // operator asked to move it out of.
    expect(MENU_SECTIONS[MENU_SECTIONS.length - 1]?.id).toBe("audit");
  });

  it("renders the audit group after every other section", () => {
    const groups = groupMenuEntries([
      { resource: "dashboard" },
      { resource: "audit" },
      { resource: "traffic-usage" },
      { resource: "apps" },
    ]);

    expect(groups.map((group) => group.id)).toEqual([
      null,
      "delivery",
      "dataStatistics",
      "audit",
    ]);
    // The overview keeps the leading slot and the trail closes the list: the two
    // ends of the sidebar are exactly what this regrouping was asked to fix.
    expect(groups[0].entries.map((entry) => entry.resource)).toEqual(["dashboard"]);
    expect(groups[groups.length - 1].entries.map((entry) => entry.resource)).toEqual(["audit"]);
  });

  it("drops the audit section when the surface supplies no entry for it", () => {
    // Only the operations surface mounts `audit`; the tenant console supplies
    // none. An empty section must not leave a dangling heading behind.
    const groups = groupMenuEntries([{ resource: "dashboard" }, { resource: "apps" }]);

    expect(groups.map((group) => group.id)).toEqual([null, "delivery"]);
  });

  it("renders ungrouped entries first and in their supplied order", () => {
    const groups = groupMenuEntries([
      { resource: "nginx" },
      { resource: "servers" },
      { resource: "apps" },
      { resource: "domains" },
      { resource: "certificates" },
      { resource: "skills" },
    ]);

    expect(groups.map((group) => group.id)).toEqual([null, "delivery", "aiEcosystem"]);
    // Ungrouped entries lead and keep the order the caller supplied.
    expect(groups[0].entries.map((entry) => entry.resource)).toEqual(["nginx", "servers"]);
    expect(groups[1].entries.map((entry) => entry.resource)).toEqual([
      "apps",
      "domains",
      "certificates",
    ]);
    expect(groups[2].entries.map((entry) => entry.resource)).toEqual(["skills"]);
  });

  it("follows the declared section order, not the incoming entry order", () => {
    const groups = groupMenuEntries([
      { resource: "mcp" },
      { resource: "certificates" },
      { resource: "plugins" },
      { resource: "skills" },
    ]);

    // The AI section declares plugins → skills → mcp even though mcp arrived first.
    expect(groups[2].entries.map((entry) => entry.resource)).toEqual([
      "plugins",
      "skills",
      "mcp",
    ]);
    // The delivery section declares apps → domains → certificates. `apps` and
    // `domains` are absent from this menu, so the section renders the one entry
    // it does have rather than padding the gap.
    expect(groups[1].entries.map((entry) => entry.resource)).toEqual(["certificates"]);
    // Every supplied entry was claimed by a section, so the leading group is
    // present but empty — it is always emitted, and the caller decides whether an
    // empty leading group still deserves the slot.
    expect(groups[0].entries).toEqual([]);
  });

  it("drops an empty section so no dangling heading is rendered", () => {
    const groups = groupMenuEntries([{ resource: "apps" }, { resource: "domains" }]);

    // `aiEcosystem` has no visible entry here and must not render a heading.
    expect(groups.map((group) => group.id)).toEqual([null, "delivery"]);
    expect(groups[1].entries.map((entry) => entry.resource)).toEqual(["apps", "domains"]);
  });

  it("keeps an ungrouped-only workspace in a single leading group", () => {
    const groups = groupMenuEntries([{ resource: "nginx" }, { resource: "servers" }]);

    expect(groups).toHaveLength(1);
    expect(groups[0].id).toBeNull();
    expect(groups[0].labelKey).toBeNull();
  });
});
