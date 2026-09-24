import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * Domains and Certificates appear on *two* surfaces, and they are not the same
 * thing: the tenant console mounts the per-user pair bridged from
 * sdkwork-deployments, while the operations surface mounts the tenant-level pair
 * over the Web Server's own `webserver_root_domain` / `webserver_domain` planes.
 *
 * Nothing else in the suite covers the surface module lists in
 * `WebserverAuthorizedWorkspace.tsx`, and the failure mode is silent: dropping a
 * module from one array, or pointing a `resourceRenderers` key at the wrong
 * surface's page, still builds, still type-checks, and only shows up as a missing
 * or wrong menu entry in a running browser. Hence the source-level assertions.
 */
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const workspace = readFileSync(resolve(root, "src/surfaces/WebserverAuthorizedWorkspace.tsx"), "utf8");

function moduleList(name: "adminModules" | "consoleModules"): string {
  const match = workspace.match(new RegExp(`const ${name} = \\[([\\s\\S]*?)\\] satisfies`));
  expect(match, `${name} is declared in the workspace host`).toBeTruthy();
  return match![1];
}

function rendererBlock(name: "resourceRenderers" | "adminResourceRenderers"): string {
  const match = workspace.match(new RegExp(`const ${name} = \\{([\\s\\S]*?)\\n  \\};`));
  expect(match, `${name} is declared in the workspace host`).toBeTruthy();
  return match![1];
}

function source(relativePath: string): string {
  return readFileSync(resolve(root, relativePath), "utf8");
}

describe("workspace delivery surfaces", () => {
  it("mounts the per-user delivery module on the tenant console only", () => {
    const consoleModules = moduleList("consoleModules");

    expect(consoleModules).toContain("deliveryModule");
    // The tenant-level module belongs to the operations surface; mounting it here
    // would offer the whole-tenant edge inventory to a tenant console.
    expect(consoleModules).not.toContain("deliveryAdminModule");
  });

  it("mounts the tenant-level delivery module on the operations surface", () => {
    expect(moduleList("adminModules")).toContain("deliveryAdminModule");
  });

  it("gives the tenant console the Deployments bridge for both domain menus", () => {
    // Both surfaces render the same `domains` / `certificates` resource keys, so a
    // renderer pointed at the wrong package would silently mount the tenant-level
    // pages on the console (or the per-user ones on the operations surface).
    const renderers = rendererBlock("resourceRenderers");

    expect(renderers).toMatch(/domains:\s*<DeployDomainManagementSurface\b/);
    expect(renderers).toMatch(/certificates:\s*<DeployDomainManagementSurface\b/);
    expect(renderers).not.toContain("ServedDomainAdminSurface");
  });

  it("gives the operations surface the Web Server's own domain and certificate pages", () => {
    const renderers = rendererBlock("adminResourceRenderers");

    expect(renderers).toMatch(/domains:\s*<ServedDomainAdminSurface\b/);
    expect(renderers).toMatch(/certificates:\s*<ServedCertificateAdminSurface\b/);
  });

  it("keeps the single-node menus and their pages off the operations surface", () => {
    const admin = moduleList("adminModules");

    // The edge is operated as a cluster: one machine's nginx runtime, a hand-kept
    // inventory of machines, SSH-scoped server files, and the node's own config
    // plane are the cluster plane's ground (`cluster-hosts` / `cluster-instances`
    // and their liveness). Re-mounting any of the four would put a single-node
    // control surface back beside `clusterModule`.
    for (const moduleName of [
      "nginxModule",
      "serversModule",
      "serversExplorerModule",
      "webserverConfigModule",
    ]) {
      expect(admin).not.toContain(moduleName);
    }
    // The control that keeps the four `not.toContain` assertions above honest:
    // `moduleList` extracts the array with a regex, and a regex that stopped
    // matching would leave it comparing against an empty string, where every
    // `not.toContain` passes vacuously.
    expect(admin).toContain("clusterModule");

    // The pages went with the menu entries. A renderer left behind is reachable
    // by deep link even though the sidebar no longer lists it — the half-wired
    // state this removal was meant to end — so the wiring is asserted too.
    const renderers = rendererBlock("adminResourceRenderers");
    expect(renderers).not.toContain("ServerFilesExplorerSurface");
    expect(renderers).not.toContain("WebserverConfigSurface");
    for (const pkg of [
      "@sdkwork/webserver-pc-admin-nginx",
      "@sdkwork/webserver-pc-admin-servers",
      "@sdkwork/webserver-pc-admin-servers-explorer",
      "@sdkwork/webserver-pc-admin-webserver-config",
    ]) {
      expect(workspace).not.toContain(pkg);
    }
  });

  it("keeps the diagnostics module and its wiring off the operations surface", () => {
    const admin = moduleList("adminModules");

    // Diagnostics was never a page of its own: it rendered from the admin
    // registry as a view over the nginx status call. Removing the menu entry
    // therefore has to take four more faces with it — the import, the registry
    // source, the `ResourceIcon` branch, and its i18n — because a source or an
    // icon branch left behind is reachable by deep link and reads as a live
    // resource that has simply lost its menu entry.
    expect(admin).not.toContain("diagnosticsModule");
    expect(workspace).not.toContain("@sdkwork/webserver-pc-admin-diagnostics");

    expect(source("packages/sdkwork-webserver-pc-admin-core/src/index.tsx"))
      .not.toContain("diagnostics:");
    expect(source("packages/sdkwork-webserver-pc-commons/src/WebserverWorkspaceChrome.tsx"))
      .not.toContain('case "diagnostics"');
    for (const locale of ["en-US", "zh-CN"]) {
      expect(source(`packages/sdkwork-webserver-pc-commons/src/i18n/${locale}/webserver/workspace/workspace.ts`))
        .not.toContain("resource.diagnostics");
    }

    // The control that keeps the `not.toContain` assertions above honest:
    // `moduleList` extracts the array with a regex, and a regex that stopped
    // matching would leave it comparing against an empty string, where every
    // `not.toContain` passes vacuously. Audit is the neighbour this change
    // *keeps*, so it is the sharpest control for a diagnostics removal.
    expect(admin).toContain("auditModule");
  });

  /**
   * The mirrored ledgers paint their chips by class name: `StatusBadge`
   * interpolates the DTO's own state into `status-<state>`, and a state the sheet
   * has no tone for falls through to `.status-badge`'s neutral base. That is a
   * quiet failure by construction — grey on grey reads as *unstyled*, not as
   * wrong, so it survives review and a visual pass.
   *
   * It shipped exactly that way. An issued certificate is `ACTIVE` on the
   * Deployments side and `ISSUED` on this one, the class-driven rewrite lost the
   * second spelling, and the mirror painted the one badge the console painted
   * green with the neutral base instead. Nothing in the suite noticed.
   */
  it("paints the Web Server's spelling of a state in the console's tone", () => {
    const sheet = readFileSync(resolve(root, "src/deploy-surface.css"), "utf8");
    const groupOf = (state: string): string => {
      const rule = sheet.match(new RegExp(`[^{}]*\\.status-${state}[^{}]*\\{[^}]*\\}`));
      expect(rule, `.status-${state} is given a tone`).toBeTruthy();
      return rule![0].replace(/\s+/g, " ").trim();
    };

    expect(groupOf("issued")).toEqual(groupOf("active"));
    // Control for the extraction above: a regex that stopped matching would have
    // thrown, and one that matched the wrong run would not know the success
    // group's other members.
    expect(groupOf("active")).toContain(".status-success");
  });

  /**
   * The other half of "reads like the console", and the same kind of quiet: the
   * console's ledger is a framework `DataTable` whose `table` carries Tailwind's
   * `text-sm` leading — a *unitless* `calc(1.25 / 0.875)` — so every cell there
   * recomputes that ratio against its own font size: 13px cells at 18.5714px, 12px
   * badges at 17.1429px. This host is not a Tailwind page, so without the
   * declaration a scoped cell inherits the preflight `1.5` and every mirrored row
   * renders a pixel taller per line than the console's.
   *
   * Unitless on purpose: a length would pin the cells and the badge (and the icon
   * buttons, which reset `font` to `inherit`) to one value instead of letting them
   * recurse the way the console's do.
   */
  it("gives the ledger cells the console's leading, as a ratio", () => {
    const sheet = readFileSync(resolve(root, "src/deploy-surface.css"), "utf8");
    const rule = sheet.match(/\.deploy-surface th, \.deploy-surface td\{[^}]*\}/);
    expect(rule, "the ledger cell rule is declared").toBeTruthy();
    expect(rule![0]).toContain("line-height:calc(1.25 / 0.875)");
    // Control: this really is the cell rule, not some selector that shares the
    // prefix and happens to carry a line-height.
    expect(rule![0]).toContain("font-size:13px");
  });
});
