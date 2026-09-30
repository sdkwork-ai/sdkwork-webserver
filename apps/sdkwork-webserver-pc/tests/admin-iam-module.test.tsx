// @vitest-environment jsdom

import { webserverModule as storageModule } from "@sdkwork/webserver-pc-admin-storage";
import {
  ADMIN_MODULES,
  groupAdminModuleEntries,
  groupMenuEntries,
  resolveAdminEntryPath,
  resolveAdminModuleForEntry,
  resolveAdminModuleFromPath,
  resolveAdminModuleLandingPath,
  translateWebserver,
  WebserverWorkspace,
  type WebserverLocale,
  type WebserverPcModuleDefinition,
  type WebserverResourceRegistry,
} from "@sdkwork/webserver-pc-commons";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";
import { adminModules, iamAdminResourceRenderers } from "../src/surfaces/WebserverAuthorizedWorkspace.tsx";

afterEach(cleanup);

/**
 * The backend-admin Identity & Access tab hosts the IAM-owned admin planes by
 * mounting the same `@sdkwork/iam-pc-admin-*` capability workspaces
 * sdkwork-cloudrouter's IAM admin mounts — alignment with that module is the
 * regression contract, so these assertions pin the wiring properties an
 * operator sees immediately: the header tab, the `/admin/iam` route subtree,
 * the five sidebar sections, and a renderer behind every menu entry.
 *
 * `adminModules` and `iamAdminResourceRenderers` are imported from the host
 * surface rather than restated: the failure this guards against is exactly a
 * module that was implemented and tested but never mounted, which a harness
 * carrying its own copy would reproduce instead of catching.
 */

const BACKEND_ADMIN_BASE = "/admin";
const LOCALES: readonly WebserverLocale[] = ["zh-CN", "en-US"];

const foundIamHostModule = adminModules.find((module) => module.id === "iam");
if (!foundIamHostModule) {
  throw new Error("the host adminModules list does not mount the iam module");
}
// A narrowed alias: the closure guards above do not flow into the test
// callbacks below, so the render helpers need the definite type up front.
const iamHostModule: WebserverPcModuleDefinition = foundIamHostModule;

/** Sidebar text a resource renders on the backend-admin surface. */
function adminMenuLabel(locale: WebserverLocale, resource: string): string {
  for (const suffix of ["admin.label", "label"]) {
    const key = `resource.${resource}.${suffix}` as Parameters<typeof translateWebserver>[1];
    const translated = translateWebserver(locale, key);
    if (translated && translated !== key) return translated;
  }
  throw new Error(`no admin menu label for ${resource}`);
}

describe("backend-admin iam module registration", () => {
  it("declares the iam header tab right after the overview with the /admin/iam prefix", () => {
    const ids = ADMIN_MODULES.map((module) => module.id);
    // The overview leads and Identity & Access follows it — the same position
    // sdkwork-cloudrouter's header gives its IAM module.
    expect(ids).toEqual(["home", "iam", "storageCenter", "clusterCenter"]);
    const iamModule = ADMIN_MODULES.find((module) => module.id === "iam");
    expect(iamModule?.labelKey).toBe("module.iam");
    expect(iamModule?.pathPrefixes).toEqual(["/admin/iam"]);
  });

  it("routes every iam entry to the iam tab, strictly below the tab's own prefix", () => {
    const prefix = ADMIN_MODULES.find((module) => module.id === "iam")?.pathPrefixes ?? [];
    expect(prefix).toEqual(["/admin/iam"]);
    for (const entry of iamHostModule.entries) {
      expect(resolveAdminModuleForEntry(BACKEND_ADMIN_BASE, entry)).toBe("iam");
      const path = resolveAdminEntryPath(BACKEND_ADMIN_BASE, entry);
      // `/admin/iam` itself is the tab landing route; an entry owning it would
      // give one operator two URL spellings for the same page.
      expect(path).not.toBe("/admin/iam");
      expect(path.startsWith("/admin/iam/")).toBe(true);
      // And the resolved route agrees with the prefix-based owner.
      expect(resolveAdminModuleFromPath(path)).toBe("iam");
    }
  });

  it("lands the tab on the users directory, like the aligned cloudrouter module", () => {
    const [, iamGroup] = groupAdminModuleEntries(BACKEND_ADMIN_BASE, iamHostModule.entries);
    expect(iamGroup?.moduleId).toBe("iam");
    expect(iamGroup?.entries).toHaveLength(iamHostModule.entries.length);
    const landing = resolveAdminModuleLandingPath(BACKEND_ADMIN_BASE, iamGroup?.entries ?? []);
    expect(landing).toBe("/admin/iam/users");
  });

  it("keeps the sub-path pages inside their entry's own splat route", () => {
    // Organization structure and OAuth custom menus are sub-paths of their
    // entries' routes (`/iam/organizations/*`, `/iam/oauth/official-accounts/*`);
    // they must not collide with a sibling entry's route.
    expect(resolveAdminModuleFromPath("/admin/iam/organizations/org-1/structure")).toBe("iam");
    expect(resolveAdminModuleFromPath("/admin/iam/oauth/official-accounts/acc-1/custom-menus")).toBe("iam");
  });
});

describe("backend-admin iam renderer coverage", () => {
  it("mounts exactly one renderer per iam menu entry, and nothing else", () => {
    const rendererKeys = Object.keys(iamAdminResourceRenderers({
      locale: "zh-CN",
      permissionScope: [],
      tenantId: "100001",
    })).sort();
    const entryKeys = iamHostModule.entries.map((entry) => entry.resource).sort();
    expect(rendererKeys).toEqual(entryKeys);
  });
});

describe("backend-admin iam sidebar sections", () => {
  it("claims every iam entry into a section so no entry renders ungrouped", () => {
    const groups = groupMenuEntries(iamHostModule.entries);
    // The leading (ungrouped) group is emitted empty; the five sections hold
    // everything, mirroring sdkwork-cloudrouter's IAM admin menu groups.
    expect(groups[0]?.entries).toEqual([]);
    expect(groups.slice(1).map((group) => group.id)).toEqual([
      "iamDirectory",
      "iamAccessControl",
      "iamOauth",
      "iamFederation",
      "iamSecurity",
    ]);
  });

  it("never draws two sidebar entries with the same label", () => {
    for (const locale of LOCALES) {
      const labels = iamHostModule.entries.map((entry) => adminMenuLabel(locale, entry.resource));
      expect(new Set(labels).size).toBe(labels.length);
    }
  });

  it("gives every section heading a label that differs from the entries beneath it", () => {
    // The sidebar draws the heading *and* the links, so a heading repeating an
    // entry label prints the same words twice on adjacent lines.
    for (const locale of LOCALES) {
      const entryLabels = new Set(iamHostModule.entries.map((entry) => adminMenuLabel(locale, entry.resource)));
      for (const group of groupMenuEntries(iamHostModule.entries).slice(1)) {
        const heading = translateWebserver(locale, group.labelKey ?? "nav.workspace");
        expect(heading).not.toBe(group.labelKey);
        expect(entryLabels.has(heading)).toBe(false);
      }
    }
  });
});

describe("backend-admin iam sidebar (rendered)", () => {
  // Every iam entry-gate code, so the full fourteen-entry menu renders; the
  // filter test below renders with a minimal scope instead.
  const FULL_IAM_PERMISSION_SCOPE = [
    "iam.users.read",
    "iam.organizations.read",
    "iam.tenants.read",
    "iam.tenant_applications.update",
    "iam.roles.read",
    "iam.permissions.read",
    "iam.policies.read",
    "iam.role_bindings.read",
    "iam.oauth.read",
    "iam.account_binding_policy.read",
    "iam.audit_events.read",
    "drive.storage.admin",
  ];

  function renderAdminWorkspace(path: string, locale: WebserverLocale, permissionScope: readonly string[] = FULL_IAM_PERMISSION_SCOPE) {
    return render(
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route
            path="/admin/*"
            element={(
              <WebserverWorkspace
                locale={locale}
                modules={[iamHostModule, storageModule]}
                permissionScope={permissionScope}
                registry={{} as WebserverResourceRegistry}
                resourceRenderers={iamAdminResourceRenderers({
                  locale,
                  permissionScope,
                  tenantId: "100001",
                })}
                surface="backend-admin"
                userLabel="ops@example.test"
              />
            )}
          />
        </Routes>
      </MemoryRouter>,
    );
  }

  it("lists the iam tab and keeps it active on the module's landing path", () => {
    const { container } = renderAdminWorkspace("/admin/iam/users", "zh-CN");
    const nav = container.querySelector("nav.workspace-header-nav");
    if (!nav) throw new Error("workspace header module nav not rendered");

    const tabs = Array.from(nav.querySelectorAll<HTMLAnchorElement>("a"));
    expect(tabs.map((tab) => tab.getAttribute("href"))).toEqual(["/admin/iam/users", "/admin/storage/providers"]);
    const active = nav.querySelector<HTMLAnchorElement>("a[aria-current='page']");
    expect(active?.getAttribute("href")).toBe("/admin/iam/users");
  });

  it("lists fourteen distinct iam links at the tab landing path", () => {
    for (const locale of LOCALES) {
      const { container } = renderAdminWorkspace("/admin/iam/users", locale);
      const sidebar = container.querySelector("aside.sidebar");
      if (!sidebar) throw new Error("workspace sidebar not rendered");
      const labels = Array.from(sidebar.querySelectorAll<HTMLAnchorElement>("a"))
        .map((anchor) => anchor.textContent?.trim() ?? "");

      expect(labels).toHaveLength(iamHostModule.entries.length);
      expect(new Set(labels).size).toBe(labels.length);
      cleanup();
    }
  });

  it("renders the section headings in declaration order", () => {
    const { container } = renderAdminWorkspace("/admin/iam/users", "zh-CN");
    const headings = Array.from(container.querySelectorAll("aside.sidebar .sidebar-section-label"))
      .map((element) => element.textContent?.trim() ?? "");
    expect(headings).toEqual(["身份目录", "访问控制", "OAuth", "连接与联合", "安全与审计"]);
  });

  it("filters entries the permission scope does not grant", () => {
    const { container } = renderAdminWorkspace("/admin/iam/users", "zh-CN", [
      "iam.users.read",
      "drive.storage.admin",
    ]);
    const sidebar = container.querySelector("aside.sidebar");
    if (!sidebar) throw new Error("workspace sidebar not rendered");
    const hrefs = Array.from(sidebar.querySelectorAll<HTMLAnchorElement>("a")).map((anchor) => anchor.getAttribute("href"));
    // The scope grants `iam.users.read` only among the iam codes: the users
    // entry stays, every other entry's own code filters it out.
    expect(hrefs).toEqual(["/admin/iam/users"]);
  });

  it("resolves the organization structure deep link to the iam tab", () => {
    const { container } = renderAdminWorkspace("/admin/iam/organizations/org-1/structure", "zh-CN");
    const nav = container.querySelector("nav.workspace-header-nav");
    const active = nav?.querySelector<HTMLAnchorElement>("a[aria-current='page']");
    expect(active?.getAttribute("href")).toBe("/admin/iam/users");
    // The page itself lazy-loads the shared workspace, so the loading state is
    // what a unit environment can assert on.
    expect(screen.getByRole("status")).toBeTruthy();
  });
});
