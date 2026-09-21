// @vitest-environment jsdom

import { webserverModule as clusterModule } from "@sdkwork/webserver-pc-admin-cluster";
import {
  ADMIN_MODULES,
  groupAdminModuleEntries,
  resolveAdminEntryPath,
  resolveAdminModuleForEntry,
  resolveAdminModuleFromPath,
  resolveAdminModuleLandingPath,
  translateWebserver,
  WebserverWorkspace,
  type WebserverLocale,
  type WebserverResourceRegistry,
} from "@sdkwork/webserver-pc-commons";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";

afterEach(cleanup);

/**
 * The backend-admin cluster tab had two defects an operator sees immediately:
 * the sidebar drew two entries both named "集群" / "Cluster", and the overview
 * claimed the bare `/admin/cluster` route that the tab itself uses as its
 * landing path. Neither is visible to a unit test that only checks registry
 * wiring, so these assertions pin the two properties directly.
 */

const BACKEND_ADMIN_BASE = "/admin";
const LOCALES: readonly WebserverLocale[] = ["zh-CN", "en-US"];

/** Sidebar text a resource renders on the backend-admin surface. */
function adminMenuLabel(locale: WebserverLocale, resource: string): string {
  for (const suffix of ["admin.label", "label"]) {
    const key = `resource.${resource}.${suffix}` as Parameters<typeof translateWebserver>[1];
    const translated = translateWebserver(locale, key);
    if (translated && translated !== key) return translated;
  }
  throw new Error(`no admin menu label for ${resource}`);
}

describe("backend-admin cluster menu", () => {
  it("never draws two sidebar entries with the same label", () => {
    for (const locale of LOCALES) {
      const labels = clusterModule.entries.map((entry) =>
        adminMenuLabel(locale, entry.resource),
      );
      // "Clusters" is the resource list, "Cluster Overview" the dashboard: they
      // must not collapse onto the same word, or the sidebar reads as a
      // duplicate menu.
      expect(new Set(labels).size).toBe(labels.length);
    }
  });

  it("keeps every cluster entry strictly below the tab's own path prefix", () => {
    const prefix = ADMIN_MODULES.find((module) => module.id === "clusterCenter")?.pathPrefixes ?? [];
    expect(prefix).toEqual(["/admin/cluster"]);
    for (const entry of clusterModule.entries) {
      const path = resolveAdminEntryPath(BACKEND_ADMIN_BASE, entry);
      // `/admin/cluster` itself is the tab landing route; an entry owning it
      // would give one operator two URL spellings for the same page.
      expect(path).not.toBe("/admin/cluster");
      expect(path.startsWith("/admin/cluster/")).toBe(true);
    }
  });

  it("routes every cluster entry to the clusterCenter tab", () => {
    for (const entry of clusterModule.entries) {
      expect(resolveAdminModuleForEntry(BACKEND_ADMIN_BASE, entry)).toBe("clusterCenter");
      // And the resolved route agrees with the prefix-based owner.
      expect(resolveAdminModuleFromPath(resolveAdminEntryPath(BACKEND_ADMIN_BASE, entry)))
        .toBe("clusterCenter");
    }
  });

  it("lands the tab on the overview without colliding with the bare prefix", () => {
    const [group] = groupAdminModuleEntries(BACKEND_ADMIN_BASE, clusterModule.entries)
      .filter((candidate) => candidate.moduleId === "clusterCenter");
    expect(group?.entries).toHaveLength(clusterModule.entries.length);
    const landing = resolveAdminModuleLandingPath(BACKEND_ADMIN_BASE, group?.entries ?? []);
    expect(landing).toBe("/admin/cluster/overview");
  });
});

/**
 * The three assertions above reason about the menu model. This last one renders
 * the real backend-admin workspace at the tab's own landing path, so the
 * sidebar markup an operator actually sees is what gets checked — a model that
 * looks right but renders duplicated labels would still fail here.
 */
describe("backend-admin cluster sidebar (rendered)", () => {
  function renderAdminWorkspace(path: string, locale: WebserverLocale) {
    return render(
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route
            path="/admin/*"
            element={(
              <WebserverWorkspace
                locale={locale}
                modules={[clusterModule]}
                permissionScope={["web.cluster.read", "web.cluster.write"]}
                registry={{} as WebserverResourceRegistry}
                resourceRenderers={{
                  "cluster-overview": <div>overview surface</div>,
                }}
                surface="backend-admin"
                userLabel="ops@example.test"
              />
            )}
          />
        </Routes>
      </MemoryRouter>,
    );
  }

  it("lists five distinct cluster links at the tab landing path", () => {
    for (const locale of LOCALES) {
      const { container } = renderAdminWorkspace("/admin/cluster/overview", locale);
      const labels = sidebarLinks(container).map((anchor) => anchor.textContent?.trim() ?? "");

      expect(labels).toHaveLength(clusterModule.entries.length);
      expect(new Set(labels).size).toBe(labels.length);
      cleanup();
    }
  });

  it("links the overview entry at /admin/cluster/overview", () => {
    const { container } = renderAdminWorkspace("/admin/cluster/overview", "zh-CN");
    const hrefs = sidebarLinks(container).map((anchor) => anchor.getAttribute("href"));

    expect(hrefs).toContain("/admin/cluster/overview");
    // The bare prefix is the tab's landing route, never a sidebar entry.
    expect(hrefs).not.toContain("/admin/cluster");
  });
});

/** Sidebar anchors only: the header's module tabs are a `<nav>` too. */
function sidebarLinks(container: HTMLElement): HTMLAnchorElement[] {
  const sidebar = container.querySelector("aside.sidebar");
  if (!sidebar) throw new Error("workspace sidebar not rendered");
  return Array.from(sidebar.querySelectorAll<HTMLAnchorElement>("a"));
}
