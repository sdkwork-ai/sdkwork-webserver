import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ApplicationsListReader } from "@sdkwork/webserver-h5-applications";
import type { DeployAppPageInfo, DeployAppResponse } from "@sdkwork/webserver-h5-core/sdk";

import { App } from "../src/App.tsx";
import { createWebserverH5MessageResolver } from "../src/i18n/index.ts";

const zhCn = createWebserverH5MessageResolver("zh-CN");

function applicationFixture(overrides: Partial<DeployAppResponse> = {}): DeployAppResponse {
  return {
    appKind: "SPA_WEB",
    appStatus: "ACTIVE",
    createdAt: "2026-09-01T00:00:00Z",
    defaultEnvironment: "production",
    id: "app-1",
    name: "Sample app",
    slug: "sample-app",
    updatedAt: "2026-09-02T00:00:00Z",
    version: "0.1.0",
    ...overrides,
  };
}

function pageInfoFixture(overrides: Partial<DeployAppPageInfo> = {}): DeployAppPageInfo {
  return { mode: "offset", page: 1, pageSize: 20, totalItems: "1", totalPages: 1, ...overrides };
}

/** Deterministic double for the generated deployments client. */
function createDeployClientDouble(
  respond: (params: { page: number; pageSize: number }) => {
    items: DeployAppResponse[];
    pageInfo: DeployAppPageInfo;
  },
): { calls: { page: number; pageSize: number }[]; reader: ApplicationsListReader } {
  const calls: { page: number; pageSize: number }[] = [];
  return {
    calls,
    reader: {
      app: {
        list: async (params) => {
          calls.push(params);
          return respond(params);
        },
      },
    },
  };
}

describe("webserver h5 application root", () => {
  it("renders the brand app bar and the applications tab from the catalog", async () => {
    const { reader } = createDeployClientDouble(() => ({
      items: [],
      pageInfo: pageInfoFixture({ totalItems: "0", totalPages: 0 }),
    }));

    render(<App clients={{ deploy: reader }} resolveMessage={zhCn} />);

    expect(screen.getByText("SDKWork Web Server")).toBeTruthy();
    expect(screen.getByRole("link", { name: "应用" })).toBeTruthy();
    await waitFor(() => {
      expect(screen.getByText("当前租户还没有应用。")).toBeTruthy();
    });
  });

  it("requests exactly one canonical page and renders the tenant applications", async () => {
    const { calls, reader } = createDeployClientDouble(() => ({
      items: [applicationFixture()],
      pageInfo: pageInfoFixture(),
    }));

    render(<App clients={{ deploy: reader }} resolveMessage={zhCn} />);

    await waitFor(() => {
      expect(screen.getByText("Sample app")).toBeTruthy();
    });
    expect(screen.getByText("sample-app")).toBeTruthy();
    expect(calls).toEqual([{ page: 1, pageSize: 20 }]);
  });

  it("surfaces a failed list without falling back to an empty state", async () => {
    const { reader } = createDeployClientDouble(() => {
      throw new Error("HTTP 401");
    });

    render(<App clients={{ deploy: reader }} resolveMessage={zhCn} />);

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain("应用列表加载失败。");
    });
    expect(screen.getByText("HTTP 401")).toBeTruthy();
    expect(screen.queryByText("当前租户还没有应用。")).toBeNull();
  });
});
