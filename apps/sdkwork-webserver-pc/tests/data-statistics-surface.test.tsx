// @vitest-environment jsdom

import { createTokenManager } from "@sdkwork/sdk-common";
import { DashboardAdminSurface, TrafficStatisticsAdminSurface } from "@sdkwork/webserver-pc-admin-data-statistics";
import {
  DashboardPlatformSurface,
  DashboardSurface,
  TrafficStatisticsSurface,
  webserverModule as dataStatisticsModule,
} from "@sdkwork/webserver-pc-console-data-statistics";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
 * 3. **Only two of them replace the frame.** An answered window is drawn whole
 *    whatever its figures are — a dashboard of zeros, with the note explaining
 *    them — so these assertions read the cards, the block titles and the table
 *    frames as well as the copy. "The page said it was empty" would pass on the
 *    broken version too, which is exactly what it did.
 * 4. **The metric row is a third read.** The overview's card row comes from the
 *    metric summary, not from the traffic reading, so the two cannot be told
 *    apart by counting "a request was sent". Its fixtures and spies are kept
 *    separate from the traffic ones, and the ledger page — which is filtered to
 *    an arbitrary window and so cannot carry fixed today/7-day/month/total cards
 *    — must not ask for it at all.
 */
const sdk = vi.hoisted(() => ({
  retrieve: vi.fn(),
  retrievePlatform: vi.fn(),
  retrieveMetrics: vi.fn(),
  retrievePlatformMetrics: vi.fn(),
}));

vi.mock("@sdkwork/webserver-pc-admin-core", () => ({
  createWebserverAdminSdkClient: () => ({
    trafficUsage: {
      retrieve: sdk.retrieve,
      platformTrafficUsages: { retrieve: sdk.retrievePlatform },
    },
    // The metric row is its own pair of operations, so its mocks are its own
    // pair too: sharing the traffic spies would make "the page asked for the
    // metrics" indistinguishable from "it asked for the traffic again".
    metricsSummary: {
      retrieve: sdk.retrieveMetrics,
      platformMetricsSummaries: { retrieve: sdk.retrievePlatformMetrics },
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

/** One metric across the four windows, in the order the contract states them. */
function metric(
  metric: string,
  unit: string,
  quantities: readonly [string, string, string, string],
) {
  return {
    metric,
    unit,
    values: ["today", "last_7_days", "current_month", "lifetime"].map((window, index) => ({
      window,
      quantity: quantities[index],
      unit,
    })),
  };
}

/**
 * One entity metric's per-day arrivals, days ascending.
 *
 * Sparse on purpose — the contract reports only the days a metric gained
 * something on, and a day the fixture leaves out is a real zero the page has to
 * fill from the window rather than from the rows.
 */
function series(metric: string, points: readonly (readonly [string, string])[]) {
  return {
    metric,
    unit: "COUNT",
    points: points.map(([date, quantity]) => ({ date, quantity })),
  };
}

/** The window the per-day series was cut against. Wider than the traffic
 *  fixture's own window, which is the point: the two readings are asserted to
 *  be asked for the *same* days, not to agree by accident. */
const seriesWindow = { dateFrom: "2026-08-26", dateTo: "2026-09-25" };

/** The day bounds the response reports behind the window ids. */
const windowBounds = [
  { window: "today", dateFrom: "2026-09-24", dateTo: "2026-09-25" },
  { window: "last_7_days", dateFrom: "2026-09-18", dateTo: "2026-09-25" },
  { window: "current_month", dateFrom: "2026-09-01", dateTo: "2026-09-25" },
  { window: "lifetime", dateTo: "2026-09-25" },
];

function metricsReading(overrides: Record<string, unknown> = {}) {
  return {
    asOf: "2026-09-24",
    platformScope: false,
    trafficSince: "2026-08-01",
    windows: windowBounds,
    // The own-tenant vocabulary: users and applications. No tenants — a tenant
    // counting itself is structurally one.
    entities: [
      metric("users", "COUNT", ["1", "2", "2", "2"]),
      metric("applications", "COUNT", ["0", "0", "0", "0"]),
      metric("agents", "COUNT", ["0", "1", "1", "3"]),
    ],
    traffic: [metric("traffic.requests", "REQUEST", ["10", "40", "40", "1200"])],
    // Drive's occupancy: a standing holding whose narrow windows are the part
    // added inside them, so the four figures are deliberately *not* a running
    // total — `lifetime` (4 GiB) is far above the month's arrivals, and the
    // object count moves for the same reason.
    storage: [
      metric("storage.used_bytes", "BYTE", ["1048576", "5242880", "5242880", "4294967296"]),
      metric("storage.object_count", "COUNT", ["2", "9", "9", "120"]),
    ],
    // The estate's own trend. One entry per metric the read model could read,
    // **including** a metric the window held no arrivals for — a quiet metric
    // is a line flat at zero, and dropping it would make its tab vanish exactly
    // when there is nothing to see.
    series: [
      series("users", [["2026-08-26", "1"], ["2026-09-24", "1"]]),
      series("applications", []),
      series("agents", [["2026-09-22", "1"], ["2026-09-23", "2"]]),
    ],
    seriesWindow,
    // Nothing is unreadable here: every figure above was read. The unassembled
    // case is a deployment without the agents module, and it gets its own test.
    unassembledMetrics: [],
    ...overrides,
  };
}

/** The same reading as the platform operation answers it: every tenant, and so
 *  with the tenant count the tenant-scoped reading omits outright. */
function platformMetricsReading(overrides: Record<string, unknown> = {}) {
  return metricsReading({
    platformScope: true,
    entities: [
      metric("users", "COUNT", ["1", "2", "2", "2"]),
      metric("tenants", "COUNT", ["0", "1", "1", "1"]),
      metric("applications", "COUNT", ["0", "0", "0", "0"]),
      metric("agents", "COUNT", ["0", "1", "1", "3"]),
    ],
    ...overrides,
  });
}

/** The overview's cards, in the order the page drew them. */
const cardText = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-card")].map((card) => [
    card.querySelector(".statistics-label")?.textContent ?? null,
    card.querySelector(".statistics-value")?.textContent ?? null,
  ]);

/** The metric row's cards: name, headline, then the narrow windows under it. */
const metricCards = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-metric-card")].map((card) => [
    card.querySelector(".statistics-metric-name")?.textContent ?? null,
    card.querySelector(".statistics-metric-headline")?.textContent ?? null,
    [...card.querySelectorAll(".statistics-metric-cell")].map((cell) => [
      cell.querySelector("span")?.textContent ?? null,
      cell.querySelector("strong")?.textContent ?? null,
    ]),
  ]);

/** The metric groups' headings, which are not the block headings. */
const metricGroupTitles = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-metrics-legend h3")].map(
    (heading) => heading.textContent,
  );

/**
 * Waits for the metric row to have drawn, by a card's own name.
 *
 * Scoped to the card rather than a bare text query because the **chart's tab
 * strip carries the same names**: the two readings label a series identically
 * (`users` is "Users" in both catalogs), so once the entity series reached the
 * chart, `findByText("Users")` matched the card and the tab alike. The
 * ambiguity is the feature working; the query is what has to be specific.
 */
const findMetricCard = (name: string) =>
  screen.findByText(name, { selector: ".statistics-metric-name" });

const blockTitles = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-block h3")].map((heading) => heading.textContent);

/**
 * The trend chart's own surface.
 *
 * Read as *structure* rather than as pixels: the tab strip, which series is
 * selected, which shape is drawn, and — for the day marks — the accessible text
 * each one carries. A chart that drew the right number of bars at the wrong
 * heights would pass a count, so the tick labels and the peaks are read too.
 */
const chartTabs = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-chart-tab")].map((tab) => ({
    label: tab.textContent,
    selected: tab.getAttribute("aria-selected") === "true",
  }));

const chartKinds = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-chart-kind")].map((button) => ({
    label: button.textContent,
    pressed: button.getAttribute("aria-pressed") === "true",
  }));

const chartTicks = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-chart-tick")].map((tick) => tick.textContent);

const chartPeaks = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-chart-peak")].map((peak) => peak.textContent);

/** One entry per drawn column, as the text an operator reads on hover. */
const chartBars = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-chart-bar")].map((bar) => bar.getAttribute("title"));

/** One entry per drawn line mark, same reading. `null` is *not* a mark: an
 *  unreadable day is a gap, and a gap has nothing to hover. */
const chartDots = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-chart-dot")].map((dot) => dot.getAttribute("title"));

/** The number of polyline runs: one per unbroken stretch of plottable days. */
const chartLines = (container: HTMLElement) =>
  container.querySelectorAll(".statistics-chart-line").length;

/** The notes the blocks draw under their headings — read as a class rather than
 *  by text, because the same sentence is also a table's empty state. */
const blockNotes = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-note")].map((note) => note.textContent);

/** The framework `DataTable`'s own surface — present for an empty table too,
 *  which is the difference between a table with an empty state and no table. */
const tableFrames = (container: HTMLElement) =>
  container.querySelectorAll("[data-sdk-region='data-table-surface']").length;

/** Everything below the card row: the half of the page the traffic reading
 *  draws. Compared as text so a differential assertion covers the blocks, the
 *  chart and the tables in one reading rather than three. */
/**
 * The traffic blocks' own text, with the chart's series strip removed.
 *
 * The strip is the one surface the **two** readings share now: it holds the
 * metered dimensions and the estate's series in a single tablist, so a mount
 * whose metric reading failed legitimately draws fewer tabs than one whose
 * metric reading answered. Comparing the raw blocks would fail on exactly that
 * difference while the claim being tested — that the traffic reading's own
 * content is untouched by the metric reading's failure — still holds. The strip
 * is asserted separately, where the strip is the subject.
 */
const trafficHalf = (container: HTMLElement) =>
  [...container.querySelectorAll(".statistics-block")].map((block) => {
    const clone = block.cloneNode(true) as HTMLElement;
    clone.querySelector(".statistics-chart-tabs")?.remove();
    return clone.textContent;
  });

beforeEach(() => {
  sdk.retrieve.mockReset();
  sdk.retrievePlatform.mockReset();
  sdk.retrieveMetrics.mockReset();
  sdk.retrievePlatformMetrics.mockReset();
  sdk.retrieve.mockResolvedValue(reading());
  sdk.retrievePlatform.mockResolvedValue(reading({ platformScope: true }));
  sdk.retrieveMetrics.mockResolvedValue(metricsReading());
  sdk.retrievePlatformMetrics.mockResolvedValue(platformMetricsReading());
});

afterEach(() => {
  cleanup();
});

describe("data statistics reaches", () => {
  it("answers the console pages from the caller's own tenant", async () => {
    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    await waitFor(() => expect(sdk.retrieve).toHaveBeenCalledTimes(1));
    expect(sdk.retrievePlatform).not.toHaveBeenCalled();
    // The card row is a second, independent read off the same page, and it too
    // has to reach for the tenant-scoped operation.
    await waitFor(() => expect(sdk.retrieveMetrics).toHaveBeenCalledTimes(1));
    expect(sdk.retrievePlatformMetrics).not.toHaveBeenCalled();
    // The scope is reported back by the response, not chosen by the page.
    expect(await screen.findByText(/Your tenant/)).toBeTruthy();

    view.unmount();
    render(<TrafficStatisticsSurface {...surfaceProps()} resource="traffic-usage" />);

    await waitFor(() => expect(sdk.retrieve).toHaveBeenCalledTimes(2));
    expect(sdk.retrievePlatform).not.toHaveBeenCalled();
    // The ledger is filtered to an arbitrary window, so it draws no fixed
    // today/7-day/month/total row and must not ask for one.
    expect(sdk.retrieveMetrics).toHaveBeenCalledTimes(1);
    expect(sdk.retrievePlatformMetrics).not.toHaveBeenCalled();
  });

  it("hands the metric summary the same window it handed the traffic reading", async () => {
    // One chart, two readings. The plot has **one** x domain, and that domain is
    // read off the *traffic* response's resolved bounds — so the days the entity
    // series was cut against have to be the same days, or the entity lines would
    // be drawn over a period they were not measured over and silently truncated
    // where the domain did not reach. Asserted as *equality between the two
    // calls* rather than against literal dates, because the window's bounds come
    // from the clock: what is being pinned is that one window produces one
    // reading of it, not what today happens to be.
    //
    // Both calls state their bounds rather than omitting them, which is the
    // point: each operation resolves an omitted bound from the clock on its own,
    // so a page that omitted them would be relying on two defaults coinciding.
    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    await waitFor(() => expect(sdk.retrieveMetrics).toHaveBeenCalledTimes(1));
    const datesOf = (params: Record<string, unknown> | undefined) => ({
      dateFrom: params?.dateFrom,
      dateTo: params?.dateTo,
    });
    const trafficWindow = datesOf(sdk.retrieve.mock.calls[0]?.[0]);
    expect(trafficWindow.dateFrom).toBeTruthy();
    expect(trafficWindow.dateTo).toBeTruthy();
    expect(datesOf(sdk.retrieveMetrics.mock.calls[0]?.[0])).toEqual(trafficWindow);
    view.unmount();
  });

  it("answers the operations pages from every tenant, through the admin re-export", async () => {
    // The admin names are aliases of this package's platform-reach symbols, so
    // this assertion is what keeps the alias from being repointed at the
    // tenant-scoped operation.
    const view = render(<DashboardAdminSurface {...surfaceProps()} resource="dashboard" />);

    await waitFor(() => expect(sdk.retrievePlatform).toHaveBeenCalledTimes(1));
    expect(sdk.retrieve).not.toHaveBeenCalled();
    await waitFor(() => expect(sdk.retrievePlatformMetrics).toHaveBeenCalledTimes(1));
    expect(sdk.retrieveMetrics).not.toHaveBeenCalled();
    expect(await screen.findByText(/Every tenant this edge serves/)).toBeTruthy();

    view.unmount();
    render(<TrafficStatisticsAdminSurface {...surfaceProps()} resource="traffic-usage" />);

    await waitFor(() => expect(sdk.retrievePlatform).toHaveBeenCalledTimes(2));
    expect(sdk.retrieve).not.toHaveBeenCalled();
    expect(sdk.retrievePlatformMetrics).toHaveBeenCalledTimes(1);
    expect(sdk.retrieveMetrics).not.toHaveBeenCalled();
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
    // Both reads answer 503. They are separate capabilities, so the page has to
    // say so about each rather than draw one empty frame over both.
    sdk.retrieve.mockRejectedValue({ code: "SERVICE_UNAVAILABLE", httpStatus: 503 });
    sdk.retrieveMetrics.mockRejectedValue({ code: "SERVICE_UNAVAILABLE", httpStatus: 503 });

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    expect(
      await screen.findByText("The traffic reading is unavailable in this deployment"),
    ).toBeTruthy();
    expect(
      await screen.findByText("The metric summary is unavailable in this deployment"),
    ).toBeTruthy();
    // The empty copy asserts something the response never said, so it must not
    // be the state a failed read lands in.
    expect(screen.queryByText("No traffic recorded in this window")).toBeNull();
    // Nor is there a frame to draw: this is one of the two states with no
    // figures at all, and the only two the page may replace the frame with.
    expect(cardText(view.container)).toEqual([]);
    expect(metricCards(view.container)).toEqual([]);
    expect(blockTitles(view.container)).toEqual([]);
  });

  it("keeps the metric row and the traffic frame independent", async () => {
    // The card row is not derived from the traffic reading, so a deployment that
    // cannot count the metrics still draws everything the traffic reading
    // answered. Asserted differentially against the healthy mount rather than
    // against a hard-coded figure count, so the comparison cannot drift as the
    // fixture changes — and so it proves the traffic half is *identical*, not
    // merely present.
    const healthy = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);
    await findMetricCard("Users");
    const healthyTraffic = trafficHalf(healthy.container);
    expect(healthyTraffic.length).toBeGreaterThan(0);
    healthy.unmount();

    sdk.retrieveMetrics.mockRejectedValue({ code: "SERVICE_UNAVAILABLE", httpStatus: 503 });
    const degraded = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    expect(
      await screen.findByText("The metric summary is unavailable in this deployment"),
    ).toBeTruthy();
    expect(metricCards(degraded.container)).toEqual([]);
    expect(metricGroupTitles(degraded.container)).toEqual([]);
    expect(trafficHalf(degraded.container)).toEqual(healthyTraffic);
    // The difference the comparison above deliberately ignores, asserted on its
    // own: the estate's series come from the reading that failed, so their tabs
    // are gone — and the metered ones, which come from the reading that
    // answered, are not.
    expect(chartTabs(degraded.container).map((tab) => tab.label)).toEqual([
      "Requests",
      "Ingress",
      "Egress",
    ]);
    expect(screen.queryByText("The traffic reading is unavailable in this deployment")).toBeNull();
  });

  it("draws the whole frame for a window that answered with no facts", async () => {
    sdk.retrieve.mockResolvedValue(reading({ totals: [], daily: [], apps: [], tenants: [] }));

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    // The note explains the zeros; it does not stand in for the page.
    expect(await screen.findByText("No traffic recorded in this window")).toBeTruthy();
    expect(screen.queryByText("The traffic reading is unavailable in this deployment")).toBeNull();

    // The overview's own cards are the metric row, so the zero-figure grid the
    // ledger draws is absent here — while the metric row, which is a different
    // reading, is drawn over it.
    expect(cardText(view.container)).toEqual([]);
    expect(metricGroupTitles(view.container)).toEqual(["Estate", "Traffic", "Storage"]);

    // Every block the populated dashboard renders, in the same order, with the
    // trend drawn over the window's own days: 2026-08-25 (inclusive) to
    // 2026-09-24 (exclusive) is 30 days, and the chart offers all three metered
    // dimensions rather than only the one the window carried a row for.
    expect(blockTitles(view.container)).toEqual(["Daily trend", "Top applications"]);
    // The strip carries the estate's series too, and an empty *traffic* window
    // does not empty them: they are answered by the other reading, off their own
    // facts, and a mount that dropped them would be reporting the traffic
    // window's emptiness as the estate's.
    expect(chartTabs(view.container).map((tab) => tab.label)).toEqual([
      "Requests",
      "Ingress",
      "Egress",
      "Users",
      "Applications",
      "Agents",
    ]);
    // One series at a time, so the columns are the window's 30 days — not three
    // series' worth of them — and every one of them is a real `0`.
    expect(chartBars(view.container)).toHaveLength(30);
    expect(chartPeaks(view.container)).toEqual(["peak 0"]);
    expect(chartTicks(view.container)).toEqual(["0"]);
    // The block with no rows is still a block: a table with an empty state.
    expect(tableFrames(view.container)).toBe(1);
  });

  it("renders the ledger's filters and all four blocks on an empty window", async () => {
    sdk.retrieve.mockResolvedValue(reading({ totals: [], daily: [], apps: [], tenants: [] }));

    const view = render(<TrafficStatisticsSurface {...surfaceProps()} resource="traffic-usage" />);

    expect(await screen.findByText("No traffic recorded in this window")).toBeTruthy();

    // A card per metered dimension, at zero — not an empty grid. No row means
    // the window summed no facts of that dimension, so `0` is the figure, and
    // these cards mirror the window the form above asked for rather than any
    // fixed period.
    expect(cardText(view.container)).toEqual([
      ["Requests", "0"],
      ["Ingress", "0 B"],
      ["Egress", "0 B"],
    ]);

    expect(blockTitles(view.container)).toEqual([
      "Daily trend",
      "Top applications",
      "Per tenant",
      "Daily detail",
    ]);
    expect(tableFrames(view.container)).toBe(3);
    // The window form is how an operator gets out of an empty window, so it has
    // to survive one.
    expect(
      [...view.container.querySelectorAll(".statistics-filters .statistics-field > span")].map(
        (label) => label.textContent,
      ),
    ).toEqual(["From (inclusive)", "To (exclusive)", "Dimension", "Applications shown"]);
    expect(
      [...view.container.querySelectorAll(".statistics-filters-actions button")].map(
        (button) => button.textContent,
      ),
    ).toEqual(["Apply", "Reset"]);
  });

  it("draws the same frame on the operations mount", async () => {
    // The two reaches are one implementation, so the empty state has to read
    // the same on both — including the platform mount, which no tenant-scoped
    // read can reach.
    sdk.retrievePlatform.mockResolvedValue(
      reading({ platformScope: true, totals: [], daily: [], apps: [], tenants: [] }),
    );

    const view = render(<DashboardAdminSurface {...surfaceProps()} resource="dashboard" />);

    expect(await screen.findByText("No traffic recorded in this window")).toBeTruthy();
    expect(metricGroupTitles(view.container)).toEqual(["Estate", "Traffic", "Storage"]);
    // The operations mount is the only one with a tenant count: the own-tenant
    // read omits it rather than reporting the constant `1`. Storage is on both,
    // because a tenant's own consumption is answerable for that tenant.
    expect(metricCards(view.container).map(([name]) => name)).toEqual([
      "Users",
      "Tenants",
      "Applications",
      "Agents",
      "Requests",
      "Ingress",
      "Egress",
      "Storage used",
      "Stored objects",
    ]);
    expect(blockTitles(view.container)).toEqual(["Daily trend", "Top applications"]);
    expect(chartBars(view.container)).toHaveLength(30);
    expect(chartPeaks(view.container)).toEqual(["peak 0"]);
  });

  it("draws a window whose figures are all zero as figures, not as an empty window", async () => {
    // A day that was measured and summed to `0` is a different response from a
    // window that holds no facts: the two draw the same chart of zeros, but only
    // the one with no facts is announced as empty. Asserted on the ledger, which
    // is the page that draws the reading's own cards; the overview's row comes
    // from the metric summary instead.
    sdk.retrieve.mockResolvedValue(
      reading({
        totals: [{ dimension: "traffic.requests", quantity: "0", unit: "REQUEST" }],
        daily: [{ usageDate: "2026-09-01", dimension: "traffic.requests", quantity: "0" }],
        apps: [],
      }),
    );

    const view = render(<TrafficStatisticsSurface {...surfaceProps()} resource="traffic-usage" />);

    await waitFor(() => expect(view.container.querySelectorAll(".statistics-card")).toHaveLength(3));
    expect(screen.queryByText("No traffic recorded in this window")).toBeNull();
    expect(cardText(view.container)).toEqual([
      ["Requests", "0"],
      ["Ingress", "0 B"],
      ["Egress", "0 B"],
    ]);
    // A day that was measured is a day to draw, so the chart is the reading's
    // own window at the reading's own figures — three series, the first one
    // drawn, and no note standing in for any of it.
    expect(chartTabs(view.container).map((tab) => tab.label)).toEqual([
      "Requests",
      "Ingress",
      "Egress",
    ]);
    expect(chartBars(view.container)).toHaveLength(30);
    expect(chartPeaks(view.container)).toEqual(["peak 0"]);
  });

  it("renders an actionable error for a failure that is not 503", async () => {
    sdk.retrieve.mockRejectedValue(new Error("network down"));

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    expect(await screen.findByText("The traffic reading could not be loaded")).toBeTruthy();
    expect(screen.queryByText("No traffic recorded in this window")).toBeNull();
    // Same as `503`: a failed attempt has no figures to draw.
    expect(cardText(view.container)).toEqual([]);
    expect(blockTitles(view.container)).toEqual([]);
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
    // Both reads the page would make are behind the one permission, so neither
    // the traffic nor the metric request may leave.
    expect(sdk.retrieve).not.toHaveBeenCalled();
    expect(sdk.retrievePlatform).not.toHaveBeenCalled();
    expect(sdk.retrieveMetrics).not.toHaveBeenCalled();
    expect(sdk.retrievePlatformMetrics).not.toHaveBeenCalled();
  });
});

/**
 * The daily trend chart.
 *
 * The block draws **one** metric at a time against that metric's own real axis,
 * and both of those are the operator's to change: the tab strip picks the
 * series, the kind group picks the shape. Every assertion below is about one of
 * those two switches, about the axis they share, or about the one branch that
 * removes the chart — a chart that drew the right number of columns on the
 * wrong scale, or that drew columns at all where the response carried no
 * breakdown, would pass a count.
 */
describe("the daily trend chart", () => {
  /** A window with two dimensions metered, four orders of magnitude apart. */
  const trend = () =>
    reading({
      totals: [
        { dimension: "traffic.requests", quantity: "40", unit: "REQUEST" },
        { dimension: "traffic.egress_bytes", quantity: "2048", unit: "BYTE" },
      ],
      daily: [
        { usageDate: "2026-08-26", dimension: "traffic.requests", quantity: "40" },
        { usageDate: "2026-08-26", dimension: "traffic.egress_bytes", quantity: "2048" },
      ],
      apps: [],
    });

  it("draws one series at a time, switched by the tab strip", async () => {
    sdk.retrieve.mockResolvedValue(trend());

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);
    await screen.findByText("Daily trend");

    // The strip offers the metering vocabulary **and** the estate's own series,
    // not only the dimensions the window happened to carry rows for. The
    // metered dimensions come first because that is the vocabulary's order, so
    // the chart still opens on the same series it always did.
    expect(chartTabs(view.container).map((tab) => tab.label)).toEqual([
      "Requests",
      "Ingress",
      "Egress",
      "Users",
      "Applications",
      "Agents",
    ]);
    expect(chartTabs(view.container).map((tab) => tab.selected)).toEqual([
      true,
      false,
      false,
      false,
      false,
      false,
    ]);
    // The selected tab is the request counter, so the axis is *its* — 40, not
    // the 3 KB an egress axis tops out at. One series at a time is exactly what
    // makes every height a real quantity rather than an index.
    expect(chartTicks(view.container)).toEqual(["40", "30", "20", "10", "0"]);
    expect(chartPeaks(view.container)).toEqual(["peak 40"]);
    expect(chartBars(view.container)).toHaveLength(30);
    // The block says why it draws one at a time, because the reason is a
    // contract fact — the units differ — and not a preference.
    expect(blockNotes(view.container)[0]).toContain("One series at a time");

    fireEvent.click(screen.getByRole("tab", { name: "Egress" }));

    expect(chartTabs(view.container).map((tab) => tab.selected)).toEqual([
      false,
      false,
      true,
      false,
      false,
      false,
    ]);
    // Same window, same 30 days, the other metric's own scale and unit.
    expect(chartTicks(view.container)).toEqual(["3 KB", "2 KB", "1 KB", "0 B"]);
    expect(chartPeaks(view.container)).toEqual(["peak 2 KB"]);
    expect(chartBars(view.container)).toHaveLength(30);
  });

  it("draws the same window as columns or as a line, switched by the kind group", async () => {
    sdk.retrieve.mockResolvedValue(trend());

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);
    await screen.findByText("Daily trend");

    // Columns are the default: one per day of the window, and no line at all.
    expect(chartKinds(view.container)).toEqual([
      { label: "Bar", pressed: true },
      { label: "Line", pressed: false },
    ]);
    expect(chartBars(view.container)).toHaveLength(30);
    expect(chartDots(view.container)).toHaveLength(0);
    expect(chartLines(view.container)).toBe(0);

    fireEvent.click(screen.getByRole("button", { name: "Line" }));

    expect(chartKinds(view.container).map((kind) => kind.pressed)).toEqual([false, true]);
    // The line draws the same 30 days as marks in one unbroken run, and no
    // columns — and the axis does not move, because switching the shape is not
    // switching the data.
    expect(chartBars(view.container)).toHaveLength(0);
    expect(chartDots(view.container)).toHaveLength(30);
    expect(chartLines(view.container)).toBe(1);
    expect(chartTicks(view.container)).toEqual(["40", "30", "20", "10", "0"]);
    expect(chartPeaks(view.container)).toEqual(["peak 40"]);

    // Back again: the two shapes are two readings of one set of measurements,
    // so neither is a one-way door.
    fireEvent.click(screen.getByRole("button", { name: "Bar" }));
    expect(chartBars(view.container)).toHaveLength(30);
    expect(chartDots(view.container)).toHaveLength(0);
  });

  it("sizes the axis gutter from its widest label instead of clipping it", async () => {
    sdk.retrieve.mockResolvedValue(trend());

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);
    await screen.findByText("Daily trend");

    // The gutter is the widest label's own width, so that label is rendered once
    // more in flow — an absolutely positioned label cannot size its parent, and
    // a fixed gutter clips exactly the figures that matter most, the nine-digit
    // ones. jsdom has no font metrics, so what is pinned here is the mechanism;
    // that the gutter then *measures* wide enough is a browser assertion.
    const sizer = view.container.querySelector(".statistics-chart-tick-sizer");
    const labels = chartTicks(view.container).map((label) => label ?? "");
    expect(sizer).not.toBeNull();
    expect(sizer?.getAttribute("aria-hidden")).toBe("true");
    // It is not a tick: the axis still reads as its own labels, once each.
    expect(labels).toContain(sizer?.textContent);
    expect(labels.every((label) => label.length <= (sizer?.textContent ?? "").length)).toBe(true);
    expect(labels).toHaveLength(new Set(labels).size);

    // And it follows the selection, because it is the *selected* series' axis.
    fireEvent.click(screen.getByRole("tab", { name: "Egress" }));
    const egressLabels = chartTicks(view.container).map((label) => label ?? "");
    const egressSizer = view.container.querySelector(".statistics-chart-tick-sizer")?.textContent ?? "";
    expect(egressLabels).toContain(egressSizer);
    expect(egressLabels.every((label) => label.length <= egressSizer.length)).toBe(true);
  });

  it("breaks the line at a day it could not read, and keeps that day's slot", async () => {
    sdk.retrieve.mockResolvedValue(
      reading({
        totals: [{ dimension: "traffic.requests", quantity: "40", unit: "REQUEST" }],
        daily: [
          // A quantity past 2^53: the wire *carried* a row, this layer cannot
          // narrow it, and reading it as `0` would assert "no traffic" on the
          // strength of a figure the surface simply could not read.
          { usageDate: "2026-09-05", dimension: "traffic.requests", quantity: "9007199254740993" },
          { usageDate: "2026-09-06", dimension: "traffic.requests", quantity: "40" },
        ],
        apps: [],
      }),
    );

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);
    await screen.findByText("Daily trend");

    // Columns: one per *readable* day, and none for the day that could not be
    // read — so it cannot be hovered into a figure it never reported.
    expect(chartBars(view.container)).toHaveLength(29);
    expect(chartBars(view.container).some((text) => text?.includes("2026-09-05"))).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "Line" }));

    // 30 days with one unreadable: two runs either side of it, never one
    // polyline drawn straight through a day nobody measured.
    expect(chartDots(view.container)).toHaveLength(29);
    expect(chartDots(view.container).some((text) => text?.includes("2026-09-05"))).toBe(false);
    expect(chartLines(view.container)).toBe(2);
  });

  it("says why it has no breakdown instead of charting zeros against the cards", async () => {
    sdk.retrieve.mockResolvedValue(
      reading({
        totals: [{ dimension: "traffic.requests", quantity: "1200", unit: "REQUEST" }],
        daily: [],
        apps: [],
      }),
    );

    // The ledger draws its own cards from the totals, which is the page where
    // the contradiction would show: three series of zeros under a headline of
    // 1,200 requests.
    const view = render(<TrafficStatisticsSurface {...surfaceProps()} resource="traffic-usage" />);
    await screen.findByText("Daily trend");

    expect(cardText(view.container)).toEqual([
      ["Requests", "1,200"],
      ["Ingress", "0 B"],
      ["Egress", "0 B"],
    ]);
    // A total with no daily rows behind it is not an empty window, and it is not
    // a zero chart either: the block keeps its heading and states what it has.
    expect(blockNotes(view.container)).toEqual([
      "This reading carries no breakdown for the selected window.",
    ]);
    expect(chartTabs(view.container)).toEqual([]);
    expect(chartBars(view.container)).toEqual([]);
    expect(view.container.querySelector(".statistics-state[data-tone='empty']")).toBeNull();
  });

  it("lands on a series that exists when a re-read drops the operator's choice", async () => {
    sdk.retrieve.mockResolvedValue(trend());

    const view = render(<TrafficStatisticsSurface {...surfaceProps()} resource="traffic-usage" />);
    await screen.findByText("Daily trend");

    fireEvent.click(screen.getByRole("tab", { name: "Egress" }));
    expect(chartTabs(view.container).map((tab) => tab.selected)).toEqual([false, false, true]);

    // Narrowing the read to another dimension is a later and stronger
    // instruction than the tab the operator picked, so the strip is now that one
    // dimension — and the chart has to land on it rather than disappearing under
    // someone who only asked for a different slice of the same window.
    sdk.retrieve.mockResolvedValue(
      reading({
        totals: [{ dimension: "traffic.requests", quantity: "40", unit: "REQUEST" }],
        daily: [{ usageDate: "2026-08-26", dimension: "traffic.requests", quantity: "40" }],
        apps: [],
      }),
    );
    fireEvent.change(screen.getByPlaceholderText("All dimensions"), {
      target: { value: "traffic.requests" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));

    await waitFor(() =>
      expect(chartTabs(view.container).map((tab) => tab.label)).toEqual(["Requests"]),
    );
    expect(chartTabs(view.container).map((tab) => tab.selected)).toEqual([true]);
    expect(chartBars(view.container)).toHaveLength(30);
  });
});

describe("the dashboard metric row", () => {
  it("reads each metric across the four windows, the standing total as headline", async () => {
    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    // Today / last 7 days / this month in the order the vocabulary declares,
    // under a headline the standing total owns. The narrow windows are arrivals,
    // so they carry a sign; the headline is a population, so it does not.
    expect(await findMetricCard("Users")).toBeTruthy();
    expect(metricCards(view.container)).toEqual([
      [
        "Users",
        "2",
        [
          ["New today", "+1"],
          ["Last 7 days", "+2"],
          ["This month", "+2"],
        ],
      ],
      [
        "Applications",
        "0",
        [
          ["New today", "0"],
          ["Last 7 days", "0"],
          ["This month", "0"],
        ],
      ],
      [
        // The agents metric is read like the other two: a narrow window is an
        // arrival, so it carries a sign, and the widest is the live population.
        "Agents",
        "3",
        [
          ["New today", "0"],
          ["Last 7 days", "+1"],
          ["This month", "+1"],
        ],
      ],
      [
        "Requests",
        "1,200",
        [
          ["Today", "10"],
          ["Last 7 days", "40"],
          ["This month", "40"],
        ],
      ],
      // The two dimensions the response carried no row for are still drawn, at
      // zero: the vocabulary is the set the page asked for, and a metered
      // dimension the window summed nothing of is a real `0`.
      [
        "Ingress",
        "0 B",
        [
          ["Today", "0 B"],
          ["Last 7 days", "0 B"],
          ["This month", "0 B"],
        ],
      ],
      [
        "Egress",
        "0 B",
        [
          ["Today", "0 B"],
          ["Last 7 days", "0 B"],
          ["This month", "0 B"],
        ],
      ],
      // The storage group reads the same four windows under its own labels,
      // because its widest window is a *holding* rather than a running total:
      // the bytes still in storage (4.3 GiB) are far above what this month
      // added (5.2 MB), which a "Total" headline would have mis-described. The
      // narrow windows are the part of that holding added inside them, so they
      // are signed exactly as the entity arrivals are.
      [
        "Storage used",
        "4.3 GB",
        [
          ["Added today", "+1 MB"],
          ["Added in 7 days", "+5.2 MB"],
          ["Added this month", "+5.2 MB"],
        ],
      ],
      [
        "Stored objects",
        "120",
        [
          ["Added today", "+2"],
          ["Added in 7 days", "+9"],
          ["Added this month", "+9"],
        ],
      ],
    ]);
  });

  it("captions the storage card's headline with a holding rather than a running total", async () => {
    // The badge names the headline's window. For storage that window is "In use
    // now", not "Total": a deleted object stops counting in every window
    // including the one it was born in, so the widest storage figure is not the
    // sum of anything, and a "Total" caption would claim a sum this group never
    // reports. The other two groups keep the shared caption.
    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    await findMetricCard("Storage used");
    expect(
      [...view.container.querySelectorAll(".statistics-metric-card")].map((card) => [
        card.querySelector(".statistics-metric-name")?.textContent ?? null,
        card.querySelector(".statistics-metric-badge")?.textContent ?? null,
      ]),
    ).toEqual([
      ["Users", "Total"],
      ["Applications", "Total"],
      ["Agents", "Total"],
      ["Requests", "Total"],
      ["Ingress", "Total"],
      ["Egress", "Total"],
      ["Storage used", "In use now"],
      ["Stored objects", "In use now"],
    ]);
  });

  it("counts tenants only on the reach that answers for every tenant", async () => {
    // A tenant counting itself is structurally one, so the own-tenant reading
    // omits the metric rather than reporting a constant — and a surface that
    // reintroduced it from its own vocabulary would put a permanently-`1` card
    // in front of an operator.
    const own = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);
    await waitFor(() => expect(metricCards(own.container)).toHaveLength(8));
    expect(metricCards(own.container).map(([name]) => name)).toEqual([
      "Users",
      "Applications",
      "Agents",
      "Requests",
      "Ingress",
      "Egress",
      "Storage used",
      "Stored objects",
    ]);
    own.unmount();

    const operations = render(<DashboardAdminSurface {...surfaceProps()} resource="dashboard" />);
    await waitFor(() => expect(metricCards(operations.container)).toHaveLength(9));
    const tenants = metricCards(operations.container).find(([name]) => name === "Tenants");
    expect(tenants).toEqual([
      "Tenants",
      "1",
      [
        ["New today", "0"],
        ["Last 7 days", "+1"],
        ["This month", "+1"],
      ],
    ]);
  });

  it("keeps the tenant count on the operations mount even when the reading carried none", async () => {
    // The vocabulary decides which cards exist, not the reading: a
    // freshly-installed deployment answers with no row for anything at all, and
    // the tenant card has to be there at `0` rather than the row thinning out.
    // That is the whole reason each reach owns a vocabulary instead of drawing
    // whatever happened to arrive — and it is the case a populated fixture
    // cannot show, because a platform reading that carries `tenants` would
    // supply the card even if the vocabulary had dropped it.
    sdk.retrievePlatformMetrics.mockResolvedValue(
      platformMetricsReading({ entities: [], traffic: [], storage: [] }),
    );

    const view = render(<DashboardAdminSurface {...surfaceProps()} resource="dashboard" />);

    await waitFor(() => expect(metricCards(view.container)).toHaveLength(9));
    expect(metricCards(view.container).map(([name]) => name)).toEqual([
      "Users",
      "Tenants",
      "Applications",
      "Agents",
      "Requests",
      "Ingress",
      "Egress",
      "Storage used",
      "Stored objects",
    ]);
    expect(metricCards(view.container).find(([name]) => name === "Tenants")).toEqual([
      "Tenants",
      "0",
      [
        ["New today", "0"],
        ["Last 7 days", "0"],
        ["This month", "0"],
      ],
    ]);
    // The storage group is vocabulary-fed too, so a deployment whose storage
    // plane has counted nothing draws "0 B" and "0" rather than losing the
    // group: the boot probe has already ruled out "the read model is missing",
    // and an absent group would be indistinguishable from an unassembled one.
    expect(metricCards(view.container).find(([name]) => name === "Storage used")).toEqual([
      "Storage used",
      "0 B",
      [
        ["Added today", "0 B"],
        ["Added in 7 days", "0 B"],
        ["Added this month", "0 B"],
      ],
    ]);
  });

  it("states the day bounds behind each figure", async () => {    // "Last 7 days" is a label; the bounds are a claim an operator can check, so
    // they are carried on the cell as its title rather than left implicit.
    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    await findMetricCard("Users");
    const cells = [...view.container.querySelectorAll(".statistics-metric-card .statistics-metric-cell")];
    const titles = [...new Set(cells.map((cell) => cell.getAttribute("title")))];

    expect(titles).toEqual([
      "2026-09-24 to 2026-09-25 (to is exclusive)",
      "2026-09-18 to 2026-09-25 (to is exclusive)",
      "2026-09-01 to 2026-09-25 (to is exclusive)",
    ]);
  });

  it("withholds the row when the reading's own scope disagrees with the page", async () => {
    // A count carries no identifiers, so a platform-wide count is
    // indistinguishable from this tenant's own. Drawn, it would be labelled by
    // the page's scope and read as this tenant's estate.
    sdk.retrieveMetrics.mockResolvedValue(metricsReading({ platformScope: true }));

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    expect(
      await screen.findByText("The metric summary does not match this page's scope"),
    ).toBeTruthy();
    expect(metricCards(view.container)).toEqual([]);
    expect(metricGroupTitles(view.container)).toEqual([]);
    // The traffic half did agree with the page, so it is untouched.
    expect(blockTitles(view.container)).toEqual(["Daily trend", "Top applications"]);
  });

  it("drops a window the response left out rather than inventing a zero for it", async () => {
    // The contract reports all four windows, so a metric that arrives with three
    // is a violation. Filling the gap with `0` would state a figure for a period
    // nobody measured; the window is dropped and the rest of the card stands.
    sdk.retrieveMetrics.mockResolvedValue(
      metricsReading({
        entities: [
          {
            metric: "users",
            unit: "COUNT",
            values: [
              { window: "today", quantity: "1", unit: "COUNT" },
              { window: "last_7_days", quantity: "2", unit: "COUNT" },
              { window: "lifetime", quantity: "2", unit: "COUNT" },
            ],
          },
        ],
      }),
    );

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    await findMetricCard("Users");
    const users = metricCards(view.container).find(([name]) => name === "Users");
    expect(users).toEqual([
      "Users",
      "2",
      [
        ["New today", "+1"],
        ["Last 7 days", "+2"],
      ],
    ]);
  });

  it("names a metric this deployment cannot count instead of drawing a zero", async () => {
    // `agents` counts rows in the agents module's own table. A deployment that
    // does not assemble that module cannot count them — and the difference
    // between "this edge cannot count agents" and "there are no agents" is
    // invisible in the figures, because both would be an absent row. So the
    // response names the metric and the card says so; a `0` there would be a
    // figure nobody measured, which is the one thing a count must not be.
    sdk.retrieveMetrics.mockResolvedValue(
      metricsReading({
        entities: [metric("users", "COUNT", ["1", "2", "2", "2"])],
        series: [series("users", [["2026-08-26", "1"]])],
        unassembledMetrics: ["agents"],
      }),
    );

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    await findMetricCard("Agents");
    const agents = metricCards(view.container).find(([name]) => name === "Agents");
    // No headline, no window cells: the card holds its name and its reason —
    // the four windows the other cards carry would each be an invented figure.
    expect(agents).toEqual(["Agents", null, []]);
    const agentCard = [...view.container.querySelectorAll(".statistics-metric-card")].find(
      (card) => card.querySelector(".statistics-metric-name")?.textContent === "Agents",
    );
    expect(agentCard?.classList.contains("statistics-metric-card--unassembled")).toBe(true);
    expect(agentCard?.querySelector(".statistics-metric-unassembled")?.textContent).toContain(
      "not assembled in this deployment",
    );
    // The badge slot says *why* the card is empty rather than captioning a
    // headline that does not exist — and it is not the window badge the four
    // measurable cards wear.
    expect(agentCard?.querySelector(".statistics-metric-badge--unassembled")?.textContent).toBe(
      "Not assembled here",
    );
    expect(agentCard?.querySelector(".statistics-metric-cell")).toBeNull();
    expect(agentCard?.querySelector(".statistics-metric-headline")).toBeNull();

    // The metric the edge *could* count is untouched, and the unreadable one is
    // kept out of the chart: a tab whose series the server never produced would
    // open on a flat line the response never reported.
    expect(metricCards(view.container).find(([name]) => name === "Users")).toEqual([
      "Users",
      "2",
      [
        ["New today", "+1"],
        ["Last 7 days", "+2"],
        ["This month", "+2"],
      ],
    ]);
    expect(chartTabs(view.container).map((tab) => tab.label)).toEqual([
      "Requests",
      "Ingress",
      "Egress",
      "Users",
    ]);
  });

  it("re-asks for the metric summary when the operator retries", async () => {
    sdk.retrieveMetrics.mockRejectedValueOnce(new Error("network down"));

    const view = render(<DashboardSurface {...surfaceProps()} resource="dashboard" />);

    expect(await screen.findByText("The metric summary could not be loaded")).toBeTruthy();
    expect(metricCards(view.container)).toEqual([]);

    sdk.retrieveMetrics.mockResolvedValue(metricsReading());
    fireEvent.click(screen.getByText("Retry"));

    expect(await findMetricCard("Users")).toBeTruthy();
    expect(sdk.retrieveMetrics).toHaveBeenCalledTimes(2);
  });
});
