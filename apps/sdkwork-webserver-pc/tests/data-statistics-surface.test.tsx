// @vitest-environment jsdom

import { createTokenManager } from "@sdkwork/sdk-common";
import { DashboardAdminSurface, TrafficStatisticsAdminSurface } from "@sdkwork/webserver-pc-admin-data-statistics";
import {
  DashboardPlatformSurface,
  DashboardSurface,
  TrafficStatisticsSurface,
  webserverModule as dataStatisticsModule,
} from "@sdkwork/webserver-pc-console-data-statistics";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * The data-statistics pages.
 *
 * Two properties are worth pinning here, and both are the kind a reader would
 * otherwise have to take on trust:
 *
 * 1. **Which operation answers a page.** The tenant reading and the platform
 *    reading are one page with two reaches, so nothing in the markup tells the
 *    two mounts apart. A test that only checked "a request was sent" would pass
 *    while the operations page quietly showed one tenant's traffic as the
 *    platform total, so each symbol is asserted against the operation it must
 *    call *and* the one it must not.
 * 2. **The three states.** A reading the edge could not produce (`503`) renders
 *    as an unavailable capability; a reading that answered with no facts renders
 *    as an empty window; and the two must not render each other's copy.
 */
const sdk = vi.hoisted(() => ({
  retrieve: vi.fn(),
  retrievePlatform: vi.fn(),
}));

vi.mock("@sdkwork/webserver-pc-admin-core", () => ({
  createWebserverAdminSdkClient: () => ({
    trafficUsage: {
      retrieve: sdk.retrieve,
      platformTrafficUsages: { retrieve: sdk.retrievePlatform },
    },
  }),
}));

const tokenManager = createTokenManager({ accessToken: "test-access-token", authToken: "test-auth-token" });

function reading(overrides: Record<string, unknown> = {}) {
  return {
    dateFrom: "2026-08-25",
    dateTo: "2026-09-24",
    platformScope: false,
    totals: [{ dimension: "traffic.requests", quantity: "1200", unit: "REQUEST" }],
    daily: [{ usageDate: "2026-09-01", dimension: "traffic.requests", quantity: "40" }],
    apps: [{ appSlug: "edge", dimension: "traffic.requests", quantity: "1200", unit: "REQUEST" }],
    tenants: [],
    ...overrides,
  };
}

function surfaceProps(permissionScope: readonly string[] = ["web.traffic.read"]) {
  return {
    backendApiBaseUrl: "/",
    locale: "en-US" as const,
    permissionScope,
    tokenManager,
  };
}

beforeEach(() => {
  sdk.retrieve.mockReset();
  sdk.retrievePlatform.mockReset();
  sdk.retrieve.mockResolvedValue(reading());
  sdk.retrievePlatform.mockResolvedValue(reading({ platformScope: true }));
});

afterEach(() => {
  cleanup();
});

describe("data statistics reaches", () => {
  it("answers the console pages from the caller's own tenant", async () => {
    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    await waitFor(() => expect(sdk.retrieve).toHaveBeenCalledTimes(1));
    expect(sdk.retrievePlatform).not.toHaveBeenCalled();
    // The scope is reported back by the response, not chosen by the page.
    expect(await screen.findByText(/Your tenant/)).toBeTruthy();

    view.unmount();
    render(<TrafficStatisticsSurface {...surfaceProps()} resource="traffic-usage" />);

    await waitFor(() => expect(sdk.retrieve).toHaveBeenCalledTimes(2));
    expect(sdk.retrievePlatform).not.toHaveBeenCalled();
  });

  it("answers the operations pages from every tenant, through the admin re-export", async () => {
    // The admin names are aliases of this package's platform-reach symbols, so
    // this assertion is what keeps the alias from being repointed at the
    // tenant-scoped operation.
    const view = render(<DashboardAdminSurface {...surfaceProps()} resource="dashboard" />);

    await waitFor(() => expect(sdk.retrievePlatform).toHaveBeenCalledTimes(1));
    expect(sdk.retrieve).not.toHaveBeenCalled();
    expect(await screen.findByText(/Every tenant this edge serves/)).toBeTruthy();

    view.unmount();
    render(<TrafficStatisticsAdminSurface {...surfaceProps()} resource="traffic-usage" />);

    await waitFor(() => expect(sdk.retrievePlatform).toHaveBeenCalledTimes(2));
    expect(sdk.retrieve).not.toHaveBeenCalled();
  });

  it("keeps the platform symbols distinct from the tenant ones", () => {
    // A host picks a symbol; it never passes a scope. If the two ever collapsed
    // into one component the reach would become a prop again.
    expect(DashboardSurface).not.toBe(DashboardPlatformSurface);
    expect(TrafficStatisticsSurface).not.toBe(DashboardPlatformSurface);
    expect(DashboardSurface).not.toBe(DashboardAdminSurface);
  });

  it("declares both menu entries under the one read permission", () => {
    expect(dataStatisticsModule.entries.map((entry) => entry.resource)).toEqual([
      "dashboard",
      "traffic-usage",
    ]);
    expect(dataStatisticsModule.entries.map((entry) => entry.permission)).toEqual([
      "web.traffic.read",
      "web.traffic.read",
    ]);
  });
});

describe("data statistics states", () => {
  it("renders an unavailable capability rather than an empty chart on 503", async () => {
    sdk.retrieve.mockRejectedValue({ code: "SERVICE_UNAVAILABLE", httpStatus: 503 });

    render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    expect(
      await screen.findByText("The traffic reading is unavailable in this deployment"),
    ).toBeTruthy();
    // The empty copy asserts something the response never said, so it must not
    // be the state a failed read lands in.
    expect(screen.queryByText("No traffic recorded in this window")).toBeNull();
  });

  it("renders an empty window when the reading answered with no facts", async () => {
    sdk.retrieve.mockResolvedValue(reading({ totals: [], daily: [], apps: [] }));

    render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    expect(await screen.findByText("No traffic recorded in this window")).toBeTruthy();
    expect(screen.queryByText("The traffic reading is unavailable in this deployment")).toBeNull();
  });

  it("renders an actionable error for a failure that is not 503", async () => {
    sdk.retrieve.mockRejectedValue(new Error("network down"));

    render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    expect(await screen.findByText("The traffic reading could not be loaded")).toBeTruthy();
    expect(screen.queryByText("No traffic recorded in this window")).toBeNull();
  });

  it("names the reach even when no reading answers", async () => {
    // The toolbar used to take its scope from the response, so a failed read
    // fell through to the tenant caption — which had the operations page telling
    // an operator they were looking at their own tenant while it was asking for
    // every tenant this edge serves. With no response the reach is still known:
    // it is which symbol was mounted.
    sdk.retrievePlatform.mockRejectedValue({ code: "SERVICE_UNAVAILABLE", httpStatus: 503 });

    render(<DashboardAdminSurface {...surfaceProps()} resource="dashboard" />);

    const meta = await screen.findByText(/Every tenant this edge serves/);
    expect(meta.textContent).not.toContain("Your tenant");
    expect(await screen.findByText("The traffic reading is unavailable in this deployment")).toBeTruthy();
  });

  it("titles the trend and the daily table apart", async () => {
    // Both blocks are built from `daily`, so one heading for both reads as a
    // duplicated block on the ledger view.
    render(<TrafficStatisticsSurface {...surfaceProps()} resource="traffic-usage" />);

    expect(await screen.findByText("Daily trend")).toBeTruthy();
    expect(await screen.findByText("Daily detail")).toBeTruthy();
  });

  it("shows the reads the caller is not permitted and sends nothing", async () => {
    render(<DashboardSurface {...surfaceProps([])} resource="dashboard" />);

    expect(await screen.findByText("This feature is not authorized")).toBeTruthy();
    expect(sdk.retrieve).not.toHaveBeenCalled();
    expect(sdk.retrievePlatform).not.toHaveBeenCalled();
  });
});
