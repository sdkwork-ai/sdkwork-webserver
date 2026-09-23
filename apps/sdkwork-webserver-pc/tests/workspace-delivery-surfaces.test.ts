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
});
