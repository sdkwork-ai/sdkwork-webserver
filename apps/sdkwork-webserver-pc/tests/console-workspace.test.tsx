// @vitest-environment jsdom

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
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
 *
 * Domains and Certificates are console entries at the *per-user* level: the
 * operator manages the domains and certificates they own, with ownership decided
 * server-side by the Deployments module. The operations surface carries the same
 * two menu labels over a different plane — the whole-tenant edge inventory the
 * Web Server reconciles from its own configuration — which is why the console
 * mounts the Deployments pages here rather than the admin ones.
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
    plugins: <PluginsConsoleSurface driveAppApiBaseUrl="/" ownerKey="user-test" resource="plugins" tokenManager={tokenManager} />,
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

  it("mounts the canonical deployments domain page for the domains and certificates entries", async () => {
    stubEmptyListResponse();
    renderWorkspace("/console/domains", {}, appUserPermissionScope, vi.fn(), "en-US", bridgedRenderers());

    // The console pair is the per-user Deployments plane. The tenant-level
    // inventory (the served root domains / subdomains this edge reconciles from
    // its own configuration) is a separate module on the operations surface, so
    // what must not appear here is that admin table, not the menu entry.
    expect(await screen.findByRole("heading", { name: "Domains" })).toBeTruthy();
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

  /**
   * 新增应用对话框的空间预算（渲染 + CSS 预算双重断言）。
   *
   * 回归背景：应用类型网格曾用 `repeat(auto-fill, minmax(150px, 1fr))` 排 9 张
   * 卡片。抽屉 `lg` 档宽 680px、左右内边距各 20px ⇒ 可用 640px ⇒
   * 640/150 = 4.26 排成 4 列，9 张卡片占 3 行；叠加「应用类型」标题与提示行，
   * 用户在到达「应用名称」输入框之前必须先滚过一整屏卡片。
   *
   * 修复：`lg` 档 880px ⇒ 可用 840px；单卡最小轨道上调到 210px ⇒
   * 840/210 = 4 列 ⇒ 9 张卡片压到 2 行（+1 个空位）。
   *
   * 为什么分两段断言：列数由 **CSS** 决定，jsdom 不跑布局
   * （`getBoundingClientRect()` 恒为 0，且 Vite 在 vitest 管线里不把
   * CSS Modules 注入 jsdom，`getComputedStyle()` 拿不到 `grid-template-columns`）。
   * 所以「结构」用真实渲染断言（卡片数量、网格是 radiogroup），
   * 「预算」用样式表源码断言宽度/最小轨道两个具体数字 —— 两者都能被变异检出。
   */
  it("bounds the create-app type grid so the drawer is not consumed by the picker", async () => {
    stubEmptyListResponse();
    renderWorkspace("/console/apps", {}, appUserPermissionScope, vi.fn(), "en-US", bridgedRenderers());

    // 打开「新增应用」抽屉（deployments 拥有的页面里的入口按钮）。
    const createButtons = await screen.findAllByRole("button", { name: /New application/ });
    fireEvent.click(createButtons[0] as HTMLElement);

    // 9 张卡片全部渲染（靠换行省空间，不是把卡片删掉）。
    const radios = await screen.findAllByRole("radio");
    expect(radios).toHaveLength(9);
    // 卡片必须挂在一个 radiogroup 网格里，而不是各自散落成块级元素。
    expect(radios[0]?.parentElement?.getAttribute("role")).toBe("radiogroup");
  });

  it("keeps the create-app drawer wide enough for single-line type-card hints", () => {
    // 预算：可用宽 = 面板宽 − 2×20px 内边距；列数 = floor((可用宽 + gap) / (最小轨道 + gap))；
    // 列宽 = (可用宽 − (列数−1)×gap) / 列数。
    const drawerWidth = readPxDeclaration(STYLE_SOURCE, /\.drawerPanelLg\s*\{[^}]*?width:\s*min\(([\d.]+)px/u);
    const minTrack = readPxDeclaration(STYLE_SOURCE, /\.typeCardGrid\s*\{[^}]*?minmax\(([\d.]+)px/u);
    const bodyPadding = readPxDeclaration(STYLE_SOURCE, /\.drawerPanel\s+\.body\s*\{[^}]*?padding:\s*[\d.]+px\s+([\d.]+)px/u);
    const tilePadding = readPxDeclaration(STYLE_SOURCE, /\.typeCardTile\s*\{[^}]*?padding:\s*([\d.]+)px\s*;/u);

    expect({ drawerWidth, minTrack, bodyPadding, tilePadding }).toStrictEqual({
      drawerWidth: 880,
      minTrack: 210,
      bodyPadding: 20,
      tilePadding: 10,
    });

    const usable = drawerWidth - 2 * bodyPadding;
    const columns = Math.floor((usable + TYPE_GRID_GAP) / (minTrack + TYPE_GRID_GAP));
    const tileWidth = (usable - (columns - 1) * TYPE_GRID_GAP) / columns;

    // 关键判据：单列宽必须装下最长的卡片提示（Android / iOS 的
    // 「原生、Flutter、React Native 或 uni-app」），否则提示折成两行、
    // 每张卡片高 17px、网格整体多出 51px —— 正是本次回归的形态。
    expect(columns).toBe(3);
    expect(tileWidth).toBeGreaterThanOrEqual(TYPE_CARD_HINT_SINGLE_LINE_WIDTH + 2 * tilePadding + 2);
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

/**
 * 新增应用对话框样式表的磁盘路径。
 *
 * 该文件属于 **sdkwork-deployments**（webserver 通过
 * `@sdkwork/webserver-pc-console-delivery` 桥接消费 deployments 的 console 包），
 * 不是本仓文件 —— 路径按工作区相对位置固定。
 */
const STYLE_SOURCE = readFileSync(resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../../../sdkwork-deployments/apps/sdkwork-deployments-pc/packages/sdkwork-deployments-pc-console-publishing/src/components/create-deploy-app.module.css",
), "utf8");

/** `.typeCardGrid` 的 `gap`，与应用类型网格的列数计算绑定。 */
const TYPE_GRID_GAP = 10;

/**
 * 最长的应用类型卡片提示一行所需的文本宽度（px）。
 *
 * 来源：Android / iOS 卡片的提示「原生、Flutter、React Native 或 uni-app」，
 * 在 11px 字号下按 CJK 11px/字 + ASCII 6.6px/字符（0.6 em）估算 ≈ 240px。
 * 这个数字是 `.typeCardGrid` 最小轨道的推导依据，改文案必须同步改这里。
 */
const TYPE_CARD_HINT_SINGLE_LINE_WIDTH = 240;

/** 从样式表源码里按捕获组第 1 段读出像素数值；读不到时抛错而不是静默返回 NaN。 */
function readPxDeclaration(source: string, pattern: RegExp): number {
  const match = pattern.exec(source);
  if (match === null || match[1] === undefined) {
    throw new Error(`declaration not found: ${String(pattern)}`);
  }
  return Number(match[1]);
}
