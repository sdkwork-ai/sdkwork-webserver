// @vitest-environment jsdom

import { webserverModule as clusterModule } from "@sdkwork/webserver-pc-admin-cluster";
import { webserverModule as storageModule } from "@sdkwork/webserver-pc-admin-storage";
import {
  resolveAdminModuleFromPath,
  WebserverWorkspace,
  type WebserverLocale,
  type WebserverResourceRegistry,
} from "@sdkwork/webserver-pc-commons";
import { cleanup, render } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";

afterEach(cleanup);

/**
 * Header module tabs lost their selected state the moment the operator opened
 * any entry other than the module's landing page: the selection was NavLink's
 * URL match against the tab `href`, and that `href` is only the module's first
 * entry (Storage Center → `/admin/storage/providers`, Cluster Center →
 * `/admin/cluster/overview`). Opening a sibling entry — still squarely inside
 * the module — deselected the tab. These assertions pin the fixed behavior:
 * the tab stays selected for the whole module subtree, per the same
 * longest-prefix ownership the workspace already uses for the sidebar.
 */

const PERMISSION_SCOPE = ["web.cluster.read", "drive.storage.admin"];

function renderAdminWorkspace(path: string, locale: WebserverLocale = "zh-CN") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route
          path="/admin/*"
          element={(
            <WebserverWorkspace
              locale={locale}
              modules={[storageModule, clusterModule]}
              permissionScope={PERMISSION_SCOPE}
              registry={{} as WebserverResourceRegistry}
              resourceRenderers={{
                "cluster-hosts": <div>cluster hosts surface</div>,
                "storage-kinds": <div>storage kinds surface</div>,
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

function moduleTabClasses(container: HTMLElement): Record<string, string> {
  const nav = container.querySelector("nav.workspace-header-nav");
  if (!nav) throw new Error("workspace header module nav not rendered");
  const classes: Record<string, string> = {};
  for (const anchor of nav.querySelectorAll<HTMLAnchorElement>("a")) {
    classes[anchor.getAttribute("href") ?? ""] = anchor.className;
  }
  return classes;
}

describe("backend-admin header module tab selection", () => {
  it("keeps the storageCenter tab selected on a non-landing storage entry", () => {
    const { container } = renderAdminWorkspace("/admin/storage/kinds");
    const tabs = moduleTabClasses(container);

    expect(tabs["/admin/storage/providers"]).toContain("is-active");
    expect(tabs["/admin/cluster/overview"]).not.toContain("is-active");
  });

  it("keeps the clusterCenter tab selected on a non-landing cluster entry", () => {
    const { container } = renderAdminWorkspace("/admin/cluster/hosts");
    const tabs = moduleTabClasses(container);

    expect(tabs["/admin/cluster/overview"]).toContain("is-active");
    expect(tabs["/admin/storage/providers"]).not.toContain("is-active");
  });

  it("marks the selected tab with aria-current=page on the whole subtree", () => {
    const { container } = renderAdminWorkspace("/admin/cluster/hosts");
    const nav = container.querySelector("nav.workspace-header-nav");
    const current = nav?.querySelector<HTMLAnchorElement>("a[aria-current='page']");

    expect(current?.getAttribute("href")).toBe("/admin/cluster/overview");
  });

  it("keeps resolving unclaimed admin paths to the home catch-all module", () => {
    expect(resolveAdminModuleFromPath("/admin/domains")).toBe("home");
    expect(resolveAdminModuleFromPath("/admin")).toBe("home");
  });
});
