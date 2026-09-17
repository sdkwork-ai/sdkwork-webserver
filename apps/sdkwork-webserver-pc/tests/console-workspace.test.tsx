// @vitest-environment jsdom

import { createTokenManager } from "@sdkwork/sdk-common";
import {
  hasWebserverAdminAccess,
  hasPlatformSuperAdminAccess,
  hasWebserverSuperAdminAccess,
  WebserverWorkspace,
  type WebserverResourceKey,
  type WebserverResourceRegistry,
} from "@sdkwork/webserver-pc-commons";
import {
  DeployAppsManagementSurface,
  DeployDomainManagementSurface,
  webserverModule as deliveryModule,
} from "@sdkwork/webserver-pc-console-delivery";
import { webserverModule as mcpModule, McpConsoleSurface } from "@sdkwork/webserver-pc-console-mcp";
import { webserverModule as pluginsModule, PluginsConsoleSurface } from "@sdkwork/webserver-pc-console-plugins";
import { webserverModule as skillsModule, SkillsConsoleSurface } from "@sdkwork/webserver-pc-console-skills";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

/**
 * Every console entry is mounted through `resourceRenderers`: applications,
 * domains, and certificates are the canonical sdkwork-deployments pages, and
 * plugins/skills/mcp are the owning packages' own surfaces. Nothing on the
 * console surface is registry-driven any more, so a bridged page must beat a
 * registry source for the same resource key (asserted below).
 */
const consoleModules = [deliveryModule, pluginsModule, skillsModule, mcpModule];
const appUserPermissionScope = ["web.applications.*", "web.certificates.*"];

function bridgedRenderers(): Partial<Record<WebserverResourceKey, ReactNode>> {
  const tokenManager = createTokenManager({ accessToken: "test-access-token", authToken: "test-auth-token" });
  const deploy = { deployBaseUrl: "/", driveBaseUrl: "/", locale: "en-US" as const, tokenManager };
  return {
    apps: <DeployAppsManagementSurface {...deploy} />,
    domains: <DeployDomainManagementSurface {...deploy} resource="domains" />,
    certificates: <DeployDomainManagementSurface {...deploy} resource="certificates" />,
    plugins: <PluginsConsoleSurface driveAppApiBaseUrl="/" resource="plugins" tokenManager={tokenManager} />,
    skills: (
      <SkillsConsoleSurface
        appApiBaseUrl="/"
        backendApiBaseUrl="/"
        driveAppApiBaseUrl="/"
        resource="skills"
        tokenManager={tokenManager}
      />
    ),
    mcp: (
      <McpConsoleSurface
        appApiBaseUrl="/"
        backendApiBaseUrl="/"
        driveAppApiBaseUrl="/"
        resource="mcp"
        tokenManager={tokenManager}
      />
    ),
  };
}

/** Every bridged page issues its own list request; an empty page is enough. */
function stubEmptyListResponse(): void {
  vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({
    code: 0,
    data: { items: [], pageInfo: { mode: "offset", page: 1, pageSize: 20, hasMore: false } },
    traceId: "trace-navigation-1",
  }), {
    headers: { "content-type": "application/json" },
    status: 200,
  })));
}

afterEach(() => {
  cleanup();
  sessionStorage.clear();
  vi.unstubAllGlobals();
});

describe("console workspace access", () => {
  it.each([
    ["/console/apps", "Applications"],
    ["/console/domains", "Domains"],
    ["/console/certificates", "Certificates"],
    ["/console/plugins", "My Plugins"],
    ["/console/skills", "My Skills"],
    ["/console/mcp", "My MCP"],
  ])("authorizes the app_user role for %s", async (path, label) => {
    stubEmptyListResponse();
    renderWorkspace(path, {}, appUserPermissionScope, vi.fn(), "en-US", bridgedRenderers());

    expect(screen.getByRole("link", { name: label })).toBeTruthy();
    expect(screen.queryByText("This feature is not authorized")).toBeNull();
  });

  it("mounts the canonical deployments publishing page for the applications entry", async () => {
    stubEmptyListResponse();
    renderWorkspace("/console/apps", {}, appUserPermissionScope, vi.fn(), "en-US", bridgedRenderers());

    // Rendered by sdkwork-deployments' PublishingAppsPage, not by this repo.
    expect(await screen.findByRole("heading", { name: "Applications" })).toBeTruthy();
    expect(screen.queryByText("This feature is not authorized")).toBeNull();
  });

  it("prefers a bridged renderer over a registry source for the same resource", async () => {
    const registry: WebserverResourceRegistry = {
      skills: {
        actions: [],
        async load() {
          return { items: [], pageInfo: { page: 1, pageSize: 20, hasMore: false } };
        },
      },
    };

    renderWorkspace("/console/skills", registry, appUserPermissionScope, vi.fn(), "en-US", {
      skills: <div>bridged skills surface</div>,
    });

    expect(screen.getByText("bridged skills surface")).toBeTruthy();
  });

  it("keeps the console shell and sign-out available for an app user", () => {
    const onSignOut = vi.fn();

    renderWorkspace("/console/apps", {}, appUserPermissionScope, onSignOut);

    expect(screen.getByRole("link", { name: "Applications" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Back to Portal" }).getAttribute("href")).toBe("/");
    expect(screen.getByRole("link", { name: "Notification center" }).getAttribute("href")).toBe("/notifications");
    expect(screen.getByTitle("user@example.test account")).toBeTruthy();
    expect(screen.queryByText("This feature is not authorized")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(onSignOut).toHaveBeenCalledOnce();
  });

  it("allows any authenticated user to browse console pages without IAM resource scopes", () => {
    stubEmptyListResponse();
    renderWorkspace("/console/apps", {}, [], vi.fn(), "en-US", bridgedRenderers());

    expect(screen.getByRole("link", { name: "Applications" })).toBeTruthy();
    expect(screen.queryByText("This feature is not authorized")).toBeNull();
  });

  it("renders a registry-driven table when a renderer is absent", async () => {
    const registry: WebserverResourceRegistry = {
      skills: {
        actions: [],
        async load() {
          return {
            items: [{ id: "skill-1", name: "Release notes" }],
            pageInfo: { page: 1, pageSize: 20, hasMore: false },
          };
        },
      },
    };

    renderWorkspace("/console/skills", registry, appUserPermissionScope);

    expect(await screen.findByText("skill-1")).toBeTruthy();
  });
});

describe("admin access classification", () => {
  it("recognizes module wildcards without treating a normal app user as an admin", () => {
    expect(hasWebserverAdminAccess(["web.*"])).toBe(true);
    expect(hasWebserverAdminAccess(["*"])).toBe(true);
    expect(hasWebserverAdminAccess(["web.applications.*"])).toBe(false);
    expect(hasWebserverAdminAccess([])).toBe(false);
  });

  it("distinguishes tenant and platform super administrators from partial operators", () => {
    expect(hasWebserverSuperAdminAccess(["web.*"])).toBe(true);
    expect(hasWebserverSuperAdminAccess(["*"])).toBe(true);
    expect(hasWebserverSuperAdminAccess(["web.applications.*"])).toBe(false);
    expect(hasWebserverSuperAdminAccess(["web.nginx.write", "web.servers.read"])).toBe(false);
    expect(hasPlatformSuperAdminAccess(["*"])).toBe(true);
    expect(hasPlatformSuperAdminAccess(["web.*"])).toBe(false);
  });
});

function renderWorkspace(
  path: string,
  registry: WebserverResourceRegistry,
  permissionScope: readonly string[],
  onSignOut = vi.fn(),
  locale: "en-US" | "zh-CN" = "en-US",
  resourceRenderers: Partial<Record<WebserverResourceKey, ReactNode>> = {},
) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route
          path="/console/*"
          element={(
            <WebserverWorkspace
              locale={locale}
              modules={consoleModules}
              notificationsHref="/notifications"
              onSignOut={onSignOut}
              permissionScope={permissionScope}
              portalHref="/"
              registry={registry}
              resourceRenderers={resourceRenderers}
              surface="app-console"
              userLabel="user@example.test"
            />
          )}
        />
      </Routes>
    </MemoryRouter>,
  );
}
