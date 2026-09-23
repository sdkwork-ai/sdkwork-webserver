// @vitest-environment jsdom

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { ClusterOverviewSurface } from "@sdkwork/webserver-pc-admin-cluster";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

/**
 * The cluster overview is a monitoring **page**, not a table.
 *
 * Two of its defects were visual and neither was reachable from the data, so
 * they survived every assertion about *what* the page fetched:
 *
 * 1. It wore the registry page's frame class, `.data-surface`. That class is the
 *    framework table's frame — `grid-template-rows: minmax(0, 1fr) auto` — sized
 *    for exactly two children, a scrolling table and its pager. This page has
 *    four flow children, so the grid handed the header the entire leftover
 *    height and pushed the cards, the ledger heading and the ledger to the
 *    bottom of the panel.
 * 2. Its header used `resource-toolbar` / `toolbar-meta`, and neither class has
 *    a single rule anywhere in this app: the header rendered as a bare browser
 *    `h2` over a bare span.
 *
 * Both are pinned below. The class-to-stylesheet contract is the one that
 * generalises: a page may use any class name it likes, but this app ships one
 * stylesheet, and a name that appears in the markup and nowhere in the CSS is a
 * page that renders undressed.
 */

const here = dirname(fileURLToPath(import.meta.url));
const surfaceSource = readFileSync(
  resolve(here, "../packages/sdkwork-webserver-pc-admin-cluster/src/ClusterOverviewSurface.tsx"),
  "utf8",
);
const stylesheet = readFileSync(resolve(here, "../src/index.css"), "utf8");

// The client has to keep one identity across renders: the page's load effect
// depends on it, and a fresh object per call would re-run the load on every
// render and eat the `mockResolvedValueOnce` queue.
const sdk = vi.hoisted(() => {
  const retrieve = vi.fn();
  const list = vi.fn();
  return {
    retrieve,
    list,
    client: {
      cluster: {
        overview: { retrieve: (...args: unknown[]) => retrieve(...args) },
        events: { list: (...args: unknown[]) => list(...args) },
      },
    },
  };
});

vi.mock("@sdkwork/webserver-pc-admin-core", () => ({
  useWebserverAdminSdk: () => sdk.client,
}));

/** Every counter is a decimal string on the wire (API_SPEC §13.6). */
const OVERVIEW = {
  totalHosts: "2",
  onlineHosts: "1",
  totalInstances: "2",
  onlineInstances: "1",
  unhealthyInstances: "1",
  pendingPeerMessages: "0",
  generatedAt: "2026-09-23T22:00:00Z",
};

const HOST_REGISTERED = {
  id: "evt-1",
  clusterId: "clu-1",
  hostId: "host-1",
  eventType: "HOST_REGISTERED",
  severity: "INFO" as const,
  message: "host BRAINXBOOK joined cluster default",
  detail: {},
  occurredAt: "2026-09-23T20:56:49Z",
  createdAt: "2026-09-23T20:56:49Z",
};

const INSTANCE_UNHEALTHY = {
  ...HOST_REGISTERED,
  id: "evt-2",
  eventType: "INSTANCE_UNHEALTHY",
  severity: "WARNING" as const,
  message: "instance edge-a:3800 missed three heartbeats",
};

function satisfied() {
  sdk.retrieve.mockResolvedValue(OVERVIEW);
  sdk.list.mockResolvedValue({ items: [HOST_REGISTERED, INSTANCE_UNHEALTHY], pageInfo: {} });
}

function renderOverview() {
  return render(<ClusterOverviewSurface locale="zh-CN" resource="cluster-overview" />);
}

/** Let the resolve/reject continuations land. */
async function settle() {
  await act(async () => {});
}

/**
 * Intercept the page's refresh interval.
 *
 * Fake timers are not usable here: this app's React 19 + vitest pair deadlocks
 * an async `act()` whenever the clock is faked, so every assertion on the
 * refresh path would hang until the test timeout. Instead the real
 * `setInterval` keeps doing its job — the mock delegates, so `waitFor` and the
 * page's own 10s tick still work — and the handler is captured so the test can
 * fire one refresh on demand.
 */
function captureRefreshes() {
  const handlers: Array<() => void> = [];
  const tokens: unknown[] = [];
  const cleared: unknown[] = [];
  const startInterval = window.setInterval.bind(window);
  const stopInterval = window.clearInterval.bind(window);
  vi.spyOn(window, "setInterval").mockImplementation((handler, timeout) => {
    handlers.push(handler as () => void);
    const token = startInterval(handler, timeout);
    tokens.push(token);
    return token;
  });
  vi.spyOn(window, "clearInterval").mockImplementation((token) => {
    cleared.push(token);
    stopInterval(token as number);
  });
  return { handlers, tokens, cleared };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("cluster overview page frame", () => {
  it("wears its own frame rather than the framework table's", async () => {
    satisfied();
    const { container } = renderOverview();

    await screen.findByText("最近事件");

    const root = container.firstElementChild;
    expect(root?.className).toBe("cluster-overview-surface");
    expect(root?.getAttribute("data-resource")).toBe("cluster-overview");
    // `data-surface` is the table's frame. If the page is inside one, the grid
    // has swallowed it again.
    expect(container.querySelector(".data-surface")).toBeNull();
  });

  it("names no class the workspace stylesheet does not define", () => {
    // The page owns the `cluster-*` vocabulary; each of those names must have a
    // rule. Extracted from the source so a class added to the markup without a
    // matching rule fails here instead of shipping undressed.
    const owned = [...new Set(surfaceSource.match(/cluster-overview-[a-z-]+/g) ?? [])].sort();
    expect(owned.length).toBeGreaterThanOrEqual(8);

    const undefinedNames = owned.filter((name) => !stylesheet.includes(`.${name}`));
    expect(undefinedNames).toEqual([]);

    // The severity badges are the one dynamic family: the class is built from
    // the wire value, so it cannot be read out of the source.
    for (const severity of ["info", "warning", "error"]) {
      expect(stylesheet).toContain(`.status-badge.cluster-severity-${severity}`);
    }
    // And the frame itself has to be a scroll container: `main.workspace` clips
    // a page that only grows, so without this the ledger below the fold is
    // unreachable.
    expect(stylesheet).toMatch(
      /\.cluster-overview-surface\s*\{[^}]*overflow-y:\s*auto[^}]*\}/s,
    );
  });

  it("keeps the registry page's toolbar vocabulary and its full-page state class out", () => {
    // Neither of these has a rule in this app; `bootstrap-state` additionally
    // carries `min-height: 100vh`, which inside a scrolling frame is a page-tall
    // empty gap rather than a message.
    for (const dead of ["data-surface", "resource-toolbar", "toolbar-meta", "bootstrap-state"]) {
      expect(surfaceSource).not.toContain(dead);
    }
  });
});

describe("cluster overview states", () => {
  it("renders the counters, the ledger, and each event's severity", async () => {
    satisfied();
    renderOverview();

    await screen.findByText("最近事件");

    // The card labels are the operator's own words, not the wire field names.
    for (const label of ["在线宿主", "宿主总数", "在线实例", "实例总数", "异常实例", "待投递节点消息"]) {
      expect(screen.getByText(label)).toBeTruthy();
    }
    expect(screen.getByText("host BRAINXBOOK joined cluster default")).toBeTruthy();
    expect(screen.getByText("INSTANCE_UNHEALTHY")).toBeTruthy();
    expect(screen.getByText("警告")).toBeTruthy();
    // A bounded recent-events page, not the whole ledger.
    expect(sdk.list).toHaveBeenCalledWith(
      { pageSize: 8 },
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
  });

  it("opens with the frame and the load failure alone when the first read fails", async () => {
    sdk.retrieve.mockRejectedValue(new Error("cluster plane unavailable"));
    sdk.list.mockResolvedValue({ items: [], pageInfo: {} });
    const { container } = renderOverview();

    const alert = await screen.findByRole("alert");
    expect(container.firstElementChild?.className).toBe("cluster-overview-surface");
    expect(alert.textContent).toContain("集群概览不可用");
    expect(alert.textContent).toContain("cluster plane unavailable");
    // No counters and no ledger on a page that never got a reading.
    expect(screen.queryByText("最近事件")).toBeNull();
  });

  it("keeps the reading on screen and only warns when a later refresh fails", async () => {
    const { handlers } = captureRefreshes();
    sdk.retrieve.mockResolvedValueOnce(OVERVIEW).mockRejectedValue(new Error("heartbeat timeout"));
    sdk.list.mockResolvedValue({ items: [HOST_REGISTERED], pageInfo: {} });

    renderOverview();
    await screen.findByText("在线宿主");
    await settle();
    expect(handlers.length).toBeGreaterThanOrEqual(1);

    // One scheduled tick, with the read now failing.
    await act(async () => { handlers[0](); });

    const warning = screen.getByRole("alert");
    expect(warning.className).toBe("warning");
    expect(warning.textContent).toContain("上次刷新失败，将自动重试");
    // The stale reading is still the operator's best evidence; the failure is
    // reported beside it, not instead of it.
    expect(screen.getByText("在线宿主")).toBeTruthy();
    expect(screen.getByText("host BRAINXBOOK joined cluster default")).toBeTruthy();
  });

  it("stops polling once the page unmounts", async () => {
    const { tokens, cleared } = captureRefreshes();
    satisfied();
    const { unmount } = renderOverview();
    await screen.findByText("最近事件");
    await settle();

    expect(tokens.length).toBeGreaterThanOrEqual(1);
    const own = tokens[0];

    unmount();

    expect(cleared).toContain(own);
  });
});
