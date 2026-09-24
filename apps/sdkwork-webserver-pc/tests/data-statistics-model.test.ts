import { describe, expect, it } from "vitest";
import {
  arrivalSign,
  buildAxis,
  buildChartPlot,
  buildChartSeries,
  buildMetricsCards,
  buildSummaryCards,
  buildWindowDays,
  CHART_KINDS,
  dimensionUnit,
  entitySeriesFigures,
  formatQuantity,
  isEntityMetric,
  isUnavailableReading,
  isEmptyReading,
  isArrivalGroup,
  metricLabelKey,
  metricsWindowBasis,
  metricsWindowLabelKey,
  METRICS_ENTITY_AGENTS,
  METRICS_ENTITY_METRICS,
  METRICS_PLATFORM_ENTITY_METRICS,
  METRICS_STORAGE_METRICS,
  METRICS_UNIT_BYTE,
  METRICS_UNIT_COUNT,
  METRICS_WINDOWS,
  resolveChartDimension,
  seriesLabelKey,
  toQuantity,
  TRAFFIC_DIMENSIONS,
  type ChartSeries,
} from "@sdkwork/webserver-pc-console-data-statistics";

/** The card vocabulary the surface asks for, which is the metering plane's own. */
const VOCABULARY = TRAFFIC_DIMENSIONS.map((known) => known.dimension);

/**
 * The presentation model behind the traffic readings.
 *
 * These assertions are about the contract's own hazards rather than about
 * formatting taste: int64 arrives as a string, dimensions are an open set, a
 * reading the edge could not produce is a different claim from a window with no
 * traffic in it, and the daily series must not be drawn on one scale.
 *
 * They also pin the frame an answered window is drawn in, because the empty
 * window is where it is easiest to be quietly wrong: filling the card
 * vocabulary must not invent a dimension the read filtered out, must not
 * rewrite a quantity it could not narrow, and must not drop one the contract
 * reports and this file has never heard of.
 */
describe("data statistics model", () => {
  it("narrows a wire quantity only when it is exactly representable", () => {
    expect(toQuantity("1200")).toBe(1200);
    expect(toQuantity("0")).toBe(0);
    // Beyond 2^53 the digits are no longer recoverable, so the raw string is
    // what the surface must show instead of a rounded number.
    expect(toQuantity("9007199254740993")).toBeNull();
    expect(toQuantity(" 42 ")).toBe(42);
    expect(toQuantity("12.5")).toBeNull();
    expect(toQuantity("1e3")).toBeNull();
    expect(toQuantity("")).toBeNull();
  });

  it("formats a byte quantity in decimal units and anything else as a count", () => {
    // The contract reports `BYTE` for the two traffic dimensions and `REQUEST`
    // for the request counter, so the unit decides the presentation.
    expect(formatQuantity("1500", "BYTE", "en-US")).toBe("1.5 KB");
    expect(formatQuantity("1200", "REQUEST", "en-US")).toBe("1,200");
    // An unrepresentable quantity is reported as sent rather than rounded.
    expect(formatQuantity("9007199254740993", "REQUEST", "en-US")).toBe("9007199254740993");
  });

  it("draws each series on its own real axis instead of indexing it to its peak", () => {
    const series = buildChartSeries(
      [
        { dimension: "traffic.requests", usageDate: "2026-09-02", quantity: "50" },
        { dimension: "traffic.requests", usageDate: "2026-09-01", quantity: "100" },
        { dimension: "traffic.egress_bytes", usageDate: "2026-09-01", quantity: "1048576" },
      ],
      ["2026-09-01", "2026-09-02"],
      ["traffic.requests", "traffic.egress_bytes"],
      new Map(),
    );

    const requests = series.find((line) => line.dimension === "traffic.requests");
    const egress = series.find((line) => line.dimension === "traffic.egress_bytes");
    // The wire's own figures survive all the way to the chart, and each series
    // carries the axis its unit deserves. An index would have made both of them
    // "100" — a height that names no quantity at all — which is exactly what
    // this replaced.
    expect(requests?.points).toEqual([
      { day: "2026-09-01", value: 100 },
      { day: "2026-09-02", value: 50 },
    ]);
    expect(requests?.peak).toBe(100);
    expect(egress?.peak).toBe(1_048_576);
    expect(requests?.unit).toBe("REQUEST");
    expect(egress?.unit).toBe("BYTE");
    // Different scales are the reason the surface draws one at a time.
    expect(requests?.top).not.toBe(egress?.top);
  });

  it("keeps an unreadable day as a gap rather than reading it as zero", () => {
    const series = buildChartSeries(
      [{ dimension: "traffic.requests", usageDate: "2026-09-01", quantity: "9007199254740993" }],
      ["2026-09-01", "2026-09-02"],
      ["traffic.requests"],
      new Map(),
    );

    // The day the wire carried but this layer cannot narrow is a gap; the day it
    // carried *no row* for is a real zero. A zero here would assert "no traffic"
    // about a figure the surface could not read, and the plot has to be able to
    // tell the two apart — that is what `null` is for.
    expect(series[0]?.points).toEqual([
      { day: "2026-09-01", value: null },
      { day: "2026-09-02", value: 0 },
    ]);
    expect(series[0]?.peak).toBe(0);
  });

  it("plots a window's missing days at zero rather than compressing the axis", () => {
    // A dimension metered on two days of a four-day window has two days of *no
    // facts*. Plotting it against its own two days would draw a busy half-week.
    const series = buildChartSeries(
      [{ dimension: "traffic.requests", usageDate: "2026-09-02", quantity: "7" }],
      ["2026-09-01", "2026-09-02", "2026-09-03", "2026-09-04"],
      ["traffic.requests"],
      new Map(),
    );

    expect(series[0]?.points.map((point) => point.value)).toEqual([0, 7, 0, 0]);
  });

  it("separates an unavailable reading from an empty window", () => {
    // 503 is the only thing that means "this deployment has nothing to produce
    // a reading with"; a response with no facts is a different claim.
    expect(isUnavailableReading({ code: "SERVICE_UNAVAILABLE" })).toBe(true);
    expect(isUnavailableReading({ httpStatus: 503 })).toBe(true);
    expect(isUnavailableReading({ problem: { status: 503 } })).toBe(true);
    expect(isUnavailableReading({ problem: { code: 50301 } })).toBe(true);
    expect(isUnavailableReading({ httpStatus: 500 })).toBe(false);
    expect(isUnavailableReading(new Error("boom"))).toBe(false);
    expect(isUnavailableReading(undefined)).toBe(false);

    expect(isEmptyReading({ daily: [], totals: [{ quantity: "0" }] })).toBe(true);
    expect(isEmptyReading({ daily: [], totals: [] })).toBe(true);
    expect(isEmptyReading({ daily: [], totals: [{ quantity: "7" }] })).toBe(false);
    expect(
      isEmptyReading({
        daily: [{ usageDate: "2026-09-01" }],
        totals: [{ quantity: "0" }],
      }),
    ).toBe(false);
    // An unrepresentable total cannot be shown to be zero, so the window is not
    // declared empty on the strength of a number this layer could not read.
    expect(isEmptyReading({ daily: [], totals: [{ quantity: "9007199254740993" }] })).toBe(false);
  });

  // The overview's frame is built from the vocabulary, not from whichever rows
  // the window happened to carry: that is what makes a quiet window a dashboard
  // of zeros rather than a sentence. These assertions pin the three ways the
  // fill-in could go wrong — inventing a dimension the read excluded, rewriting
  // a quantity it could not read, or losing one it has never heard of.
  describe("summary cards", () => {
    it("fills the vocabulary with zeros for the dimensions a quiet window omitted", () => {
      expect(buildSummaryCards([], VOCABULARY)).toEqual([
        { dimension: "traffic.requests", quantity: "0", unit: "REQUEST" },
        { dimension: "traffic.ingress_bytes", quantity: "0", unit: "BYTE" },
        { dimension: "traffic.egress_bytes", quantity: "0", unit: "BYTE" },
      ]);
    });

    it("orders by the vocabulary rather than by the wire", () => {
      // The facts owner groups by dimension, so the wire order is alphabetical;
      // a page that drew it as sent would list egress, ingress, requests.
      const cards = buildSummaryCards(
        [
          { dimension: "traffic.egress_bytes", quantity: "918", unit: "BYTE" },
          { dimension: "traffic.requests", quantity: "184223", unit: "REQUEST" },
        ],
        VOCABULARY,
      );

      expect(cards.map((card) => card.dimension)).toEqual([
        "traffic.requests",
        "traffic.ingress_bytes",
        "traffic.egress_bytes",
      ]);
      expect(cards.map((card) => card.quantity)).toEqual(["184223", "0", "918"]);
    });

    it("narrows to the filtered dimension instead of reporting the others at zero", () => {
      // With a dimension filter applied the other dimensions were *excluded*
      // from the read, not measured at zero, so they must not appear as figures.
      expect(buildSummaryCards([], ["traffic.egress_bytes"])).toEqual([
        { dimension: "traffic.egress_bytes", quantity: "0", unit: "BYTE" },
      ]);
    });

    it("carries an unreadable quantity through rather than rewriting it as zero", () => {
      const cards = buildSummaryCards(
        [{ dimension: "traffic.requests", quantity: "9007199254740993", unit: "REQUEST" }],
        ["traffic.requests"],
      );

      expect(cards[0]?.quantity).toBe("9007199254740993");
    });

    it("keeps a dimension the vocabulary has never heard of", () => {
      const cards = buildSummaryCards(
        [{ dimension: "traffic.custom", quantity: "5", unit: "REQUEST" }],
        ["traffic.requests"],
      );

      expect(cards.map((card) => card.dimension)).toEqual(["traffic.requests", "traffic.custom"]);
      expect(cards[1]).toEqual({ dimension: "traffic.custom", quantity: "5", unit: "REQUEST" });
    });

    it("falls back to the vocabulary's unit when the wire left one empty", () => {
      // A byte quantity read as a count is a wrong figure, not a missing label.
      expect(dimensionUnit("traffic.ingress_bytes")).toBe("BYTE");
      expect(dimensionUnit("traffic.unknown")).toBe("");
      expect(
        buildSummaryCards(
          [{ dimension: "traffic.ingress_bytes", quantity: "10", unit: "" }],
          ["traffic.ingress_bytes"],
        )[0]?.unit,
      ).toBe("BYTE");
    });
  });

  // The window an answered reading covers is the x domain of every series on
  // the chart, whether the window carries facts or none: `dateTo` is exclusive,
  // so the days are a half-open range and the count is exactly the span the
  // response reported.
  describe("window days", () => {
    it("covers the reported window, exclusive of its end day", () => {
      expect(buildWindowDays("2026-09-01", "2026-09-04")).toEqual([
        "2026-09-01",
        "2026-09-02",
        "2026-09-03",
      ]);
      expect(buildWindowDays("2026-08-25", "2026-09-24")).toHaveLength(30);
    });

    it("reports no days at all for a window it cannot read", () => {
      // Nothing to draw is better than a made-up axis.
      expect(buildWindowDays("2026-09-01", "2026-09-01")).toEqual([]);
      expect(buildWindowDays("2026-09-04", "2026-09-01")).toEqual([]);
      expect(buildWindowDays("", "2026-09-01")).toEqual([]);
      expect(buildWindowDays("2026-09-01", "not-a-day")).toEqual([]);
    });

    it("draws an empty window as its own days at zero", () => {
      const series = buildChartSeries(
        [],
        ["2026-09-01", "2026-09-02"],
        ["traffic.egress_bytes", "traffic.requests"],
        new Map(),
      );

      // Same order as the cards, and every day present: the block is the
      // populated block, with zeros in it. There is no separate zero-baseline
      // path — an empty window is the vocabulary over the window's own days,
      // through the same function a populated window goes through, which is why
      // the two cannot drift apart.
      expect(series.map((line) => line.dimension)).toEqual([
        "traffic.requests",
        "traffic.egress_bytes",
      ]);
      expect(series[0]?.points).toEqual([
        { day: "2026-09-01", value: 0 },
        { day: "2026-09-02", value: 0 },
      ]);
      expect(series[0]?.peak).toBe(0);
      // A zero peak still gets a real axis: one tick on the floor, rather than
      // an empty one. That is what keeps the empty chart the same chart.
      expect(series[0]?.top).toBe(0);
      expect(series[0]?.ticks).toEqual([{ value: 0, fraction: 0 }]);
    });
  });

  it("orders the chart's series by the vocabulary rather than by the wire", () => {
    const series = buildChartSeries(
      [
        { dimension: "traffic.egress_bytes", usageDate: "2026-09-01", quantity: "10" },
        { dimension: "traffic.requests", usageDate: "2026-09-01", quantity: "10" },
      ],
      ["2026-09-01"],
      [],
      new Map(),
    );

    expect(series.map((line) => line.dimension)).toEqual([
      "traffic.requests",
      "traffic.egress_bytes",
    ]);
  });

  it("fills in a vocabulary dimension the window carried no row for", () => {
    // The tabs are the vocabulary, not the response: a deployment that has
    // counted nothing yet must still offer the same tabs, each drawn at zero.
    const series = buildChartSeries(
      [],
      ["2026-09-01", "2026-09-02"],
      VOCABULARY,
      new Map(),
    );

    expect(series.map((line) => line.dimension)).toEqual(VOCABULARY);
    expect(series.map((line) => line.points.map((point) => point.value))).toEqual([
      [0, 0],
      [0, 0],
      [0, 0],
    ]);
  });
});

/**
 * The chart behind the daily trend.
 *
 * Two switches and one axis, and each of the three has a way of being quietly
 * wrong that a screenshot would not show: an axis whose steps are not whole
 * numbers prints two identical labels on different gridlines, a line joined
 * across a gap draws a slope through a period nobody measured, and a selection
 * that outlives the series it names draws an empty plot under a tab that is no
 * longer there.
 */
describe("the chart model", () => {
  const series = (overrides: Partial<ChartSeries> = {}): ChartSeries => ({
    dimension: "traffic.requests",
    unit: "REQUEST",
    points: [
      { day: "2026-09-01", value: 100 },
      { day: "2026-09-02", value: 50 },
    ],
    peak: 100,
    top: 100,
    ticks: [
      { value: 0, fraction: 0 },
      { value: 100, fraction: 1 },
    ],
    ...overrides,
  });

  it("names exactly the two chart types the surface can draw", () => {
    expect([...CHART_KINDS]).toEqual(["bar", "line"]);
  });

  it("gives every axis whole-number steps and distinct labels", () => {
    // The labels go through `formatQuantity`, whose count branch rounds to zero
    // decimals: a fractional step would print the same figure on two adjacent
    // gridlines and claim two different heights are equal. Swept rather than
    // sampled, because the step is picked from a table of multiples and only a
    // narrow band of peaks reaches the `2.5` entry — which is the one that is
    // fractional when the decade is 1.
    const peaks = [
      ...Array.from({ length: 200 }, (_, index) => index + 1),
      28_410,
      7_423_910_400,
    ];
    for (const peak of peaks) {
      const { top, ticks } = buildAxis(peak);
      expect(top).toBeGreaterThanOrEqual(peak);
      expect(ticks[0]).toEqual({ value: 0, fraction: 0 });
      expect(ticks[ticks.length - 1]).toEqual({ value: top, fraction: 1 });
      expect(ticks.length).toBeGreaterThan(1);
      const labels = ticks.map((tick) => formatQuantity(String(tick.value), "BYTE", "en-US"));
      for (const tick of ticks) expect(Number.isInteger(tick.value)).toBe(true);
      expect(new Set(labels).size).toBe(ticks.length);
      // Bottom-up and strictly increasing, which is how the plot reads it.
      for (let index = 1; index < ticks.length; index += 1) {
        expect(ticks[index]!.value).toBeGreaterThan(ticks[index - 1]!.value);
      }
    }
  });

  it("gives a zero peak a floor tick rather than an empty axis", () => {
    // An answered window whose every day summed to nothing gets a real axis
    // whose top is zero, which is what keeps the empty chart the same chart.
    expect(buildAxis(0)).toEqual({ top: 0, ticks: [{ value: 0, fraction: 0 }] });
    expect(buildAxis(-5)).toEqual({ top: 0, ticks: [{ value: 0, fraction: 0 }] });
  });

  it("keeps the selected dimension until the reading stops carrying it", () => {
    const lines = [{ dimension: "traffic.requests" }, { dimension: "traffic.egress_bytes" }];

    expect(resolveChartDimension(lines, "traffic.egress_bytes")).toBe("traffic.egress_bytes");
    // A re-read that dropped the operator's choice falls back to a series that
    // exists — it does not take the chart away or draw the stale name.
    expect(resolveChartDimension(lines, "traffic.ingress_bytes")).toBe("traffic.requests");
    expect(resolveChartDimension(lines, null)).toBe("traffic.requests");
    expect(resolveChartDimension([], "traffic.requests")).toBeNull();
  });

  it("breaks the line at a gap instead of joining across it", () => {
    const plot = buildChartPlot(
      series({
        points: [
          { day: "2026-09-01", value: 100 },
          { day: "2026-09-02", value: null },
          { day: "2026-09-03", value: 50 },
        ],
        peak: 100,
      }),
    );

    // Two runs of one, not one run of two: a single polyline here would draw a
    // straight line through a day nobody measured.
    expect(plot.segments.map((run) => run.map((mark) => mark.day))).toEqual([
      ["2026-09-01"],
      ["2026-09-03"],
    ]);
    // The unreadable day has no bar either, and the two plottable days keep
    // their own slots: the gap holds its place on the axis.
    expect(plot.bars.map((bar) => bar.day)).toEqual(["2026-09-01", "2026-09-03"]);
    expect(plot.bars[1]!.x).toBeCloseTo(200 / 3, 6);
  });

  it("lets a bar and a line describe the same measurements", () => {
    const source = series({
      points: [
        { day: "2026-09-01", value: 25 },
        { day: "2026-09-02", value: 100 },
      ],
      peak: 100,
    });
    const plot = buildChartPlot(source);
    const flattened = plot.segments.flat();

    // The two shapes come off one set of marks, so switching type cannot change
    // what an operator is looking at.
    expect(plot.bars.map((bar) => bar.value)).toEqual([25, 100]);
    expect(flattened.map((mark) => mark.value)).toEqual([25, 100]);
    // Axis tops sit at 100, so the peak day fills the plot and the other is a
    // quarter of it.
    expect(plot.bars.map((bar) => bar.height)).toEqual([25, 100]);
    expect(flattened.map((mark) => mark.y)).toEqual([25, 100]);
  });

  it("puts the first and last day flush against the plot's edges", () => {
    const plot = buildChartPlot(
      series({
        points: [
          { day: "2026-09-01", value: 10 },
          { day: "2026-09-02", value: 20 },
          { day: "2026-09-03", value: 30 },
        ],
        peak: 30,
        top: 30,
      }),
    );

    // A line reaches both edges; bars fill the width without overhanging it.
    expect(plot.segments[0]!.map((mark) => mark.x)).toEqual([0, 50, 100]);
    expect(plot.bars[0]!.x).toBe(0);
    const last = plot.bars[plot.bars.length - 1]!;
    expect(last.x + last.width).toBeCloseTo(100, 6);
    // Every bar is the same width, and they tile the plot exactly.
    expect(new Set(plot.bars.map((bar) => bar.width)).size).toBe(1);
    expect(plot.bars.reduce((total, bar) => total + bar.width, 0)).toBeCloseTo(100, 6);
  });

  it("centres a lone day instead of filling the plot with one column", () => {
    const plot = buildChartPlot(
      series({ points: [{ day: "2026-09-01", value: 5 }], peak: 5, top: 5 }),
    );

    // One plottable day: a line would have to divide by zero to place it, and a
    // bar's own slot would be the whole plot — a filled panel rather than a
    // figure. The column is drawn at the width two days would give it, and the
    // line's mark sits on the same centre. Bar mode must never be an empty plot.
    expect(plot.bars).toEqual([
      { day: "2026-09-01", value: 5, x: 25, y: 100, width: 50, height: 100 },
    ]);
    expect(plot.segments).toEqual([[{ day: "2026-09-01", value: 5, x: 50, y: 100 }]]);
  });

  it("has no plot at all for no series", () => {
    expect(buildChartPlot(null)).toEqual({ bars: [], segments: [] });
  });
});

/**
 * The dashboard metric row's presentation model.
 *
 * Two hazards are pinned here, and both are the kind that only show up on a
 * deployment that has just been installed:
 *
 * 1. **An absent row is not an absent metric.** The vocabulary the surface asks
 *    for is what decides which cards exist, so a metric the response carried no
 *    row for is a real `0` — otherwise the row would thin out to nothing on a
 *    fresh installation instead of showing the zeros.
 * 2. **But a missing window is not a zero.** The contract reports all four
 *    windows per metric, so a metric that arrives with three is a violation, and
 *    inventing the fourth would state a figure for a period nobody measured.
 */
describe("the metric row model", () => {
  const users = (quantities: readonly [string, string, string, string]) => ({
    metric: "users",
    unit: METRICS_UNIT_COUNT,
    values: METRICS_WINDOWS.map((window, index) => ({
      window,
      quantity: quantities[index],
      unit: METRICS_UNIT_COUNT,
    })),
  });

  const entityVocabulary = METRICS_ENTITY_METRICS.map((metric) => ({
    metric,
    unit: METRICS_UNIT_COUNT,
  }));

  it("draws a vocabulary metric the response carried no row for at zero", () => {
    const cards = buildMetricsCards([users(["1", "2", "2", "2"])], entityVocabulary);

    // Both vocabulary entries get a card, in vocabulary order, even though only
    // one of them arrived.
    expect(cards.map((card) => card.metric)).toEqual([...METRICS_ENTITY_METRICS]);
    expect(cards[1]?.headline).toEqual({
      window: "lifetime",
      quantity: "0",
      unit: METRICS_UNIT_COUNT,
    });
    expect(cards[1]?.narrower.map((value) => value.quantity)).toEqual(["0", "0", "0"]);
  });

  it("drops a window the response left out instead of inventing a zero for it", () => {
    const cards = buildMetricsCards(
      [
        {
          metric: "users",
          unit: METRICS_UNIT_COUNT,
          values: [
            { window: "today", quantity: "1", unit: METRICS_UNIT_COUNT },
            { window: "lifetime", quantity: "2", unit: METRICS_UNIT_COUNT },
          ],
        },
      ],
      entityVocabulary,
    );

    // The windows the contract owes and did not deliver are gone; the two that
    // arrived stand, and the headline is still the standing total.
    expect(cards[0]?.narrower.map((value) => value.window)).toEqual(["today"]);
    expect(cards[0]?.headline?.quantity).toBe("2");
  });

  it("gives a metric the response carried and the vocabulary does not its own card", () => {
    // The dimension set is open, so a newly metered dimension has to arrive as
    // its own card rather than as a silent absence.
    const cards = buildMetricsCards(
      [
        {
          metric: "traffic.new_dimension",
          unit: "REQUEST",
          values: [{ window: "today", quantity: "7", unit: "REQUEST" }],
        },
      ],
      [],
    );

    expect(cards.map((card) => card.metric)).toEqual(["traffic.new_dimension"]);
    expect(cards[0]?.narrower).toEqual([{ window: "today", quantity: "7", unit: "REQUEST" }]);
    // Only `today` arrived, so there is no standing total to head the card with.
    expect(cards[0]?.headline).toBeNull();
  });

  it("draws no card at all for a row that carries no window", () => {
    // Every window this metric reported was dropped, so there is no figure to
    // draw. A card with an empty sub-row and no headline would be a card for
    // nothing, which is worse than no card.
    const cards = buildMetricsCards(
      [{ metric: "traffic.requests", unit: "REQUEST", values: [] }],
      [],
    );

    expect(cards).toEqual([]);
  });

  it("never promotes a narrower window to the headline", () => {
    // With no `lifetime` figure the card is drawn with its sub-row alone. A
    // promoted window would present, say, the month's arrivals as the standing
    // total — a figure the response never claimed.
    const cards = buildMetricsCards(
      [
        {
          metric: "users",
          unit: METRICS_UNIT_COUNT,
          values: [{ window: "today", quantity: "1", unit: METRICS_UNIT_COUNT }],
        },
      ],
      entityVocabulary,
    );

    expect(cards[0]?.headline).toBeNull();
    expect(cards[0]?.narrower).toHaveLength(1);
  });

  it("falls back to the vocabulary's unit for a row that reports none", () => {
    // The contract lets a producer leave `unit` empty, and a byte quantity
    // formatted as a count is off by three orders of magnitude.
    const cards = buildMetricsCards(
      [
        {
          metric: "traffic.ingress_bytes",
          unit: "",
          values: [{ window: "today", quantity: "1024", unit: "" }],
        },
      ],
      [{ metric: "traffic.ingress_bytes", unit: "BYTE" }],
    );

    expect(cards[0]?.narrower[0]?.unit).toBe("BYTE");
  });

  it("orders the windows narrowest first regardless of the wire order", () => {
    const cards = buildMetricsCards(
      [
        {
          metric: "users",
          unit: METRICS_UNIT_COUNT,
          values: [
            { window: "lifetime", quantity: "2", unit: METRICS_UNIT_COUNT },
            { window: "current_month", quantity: "2", unit: METRICS_UNIT_COUNT },
            { window: "today", quantity: "1", unit: METRICS_UNIT_COUNT },
            { window: "last_7_days", quantity: "2", unit: METRICS_UNIT_COUNT },
          ],
        },
      ],
      entityVocabulary,
    );

    expect(cards[0]?.narrower.map((value) => value.window)).toEqual([
      "today",
      "last_7_days",
      "current_month",
    ]);
  });

  it("signs a positive arrival and nothing else", () => {
    expect(arrivalSign("1")).toBe("+");
    expect(arrivalSign("1200")).toBe("+");
    // `+0` would read as a change where nothing changed.
    expect(arrivalSign("0")).toBe("");
    // Nor would a figure this layer cannot narrow be embellished into a delta it
    // never reported.
    expect(arrivalSign("12.5")).toBe("");
    expect(arrivalSign("9007199254740993")).toBe("");
    expect(arrivalSign("")).toBe("");
  });

  it("keys the window labels by group so one card cannot borrow another's label", () => {
    // The wire vocabulary is `lower_snake_case` and the catalog keys are
    // camelCase, so a mechanical derivation would produce a key no catalog has.
    expect(metricsWindowLabelKey("arrival", "today")).toBe("dataStatistics.window.arrival.today");
    expect(metricsWindowLabelKey("volume", "today")).toBe("dataStatistics.window.volume.today");
    expect(metricsWindowLabelKey("volume", "last_7_days")).toBe(
      "dataStatistics.window.volume.last7Days",
    );
    expect(metricsWindowLabelKey("volume", "current_month")).toBe(
      "dataStatistics.window.volume.currentMonth",
    );
    expect(metricsWindowLabelKey("storage", "today")).toBe("dataStatistics.window.storage.today");
    expect(metricsWindowLabelKey("storage", "last_7_days")).toBe(
      "dataStatistics.window.storage.last7Days",
    );
    expect(metricsWindowLabelKey("storage", "current_month")).toBe(
      "dataStatistics.window.storage.currentMonth",
    );
    // The widest window is shared by the two groups that report a *total* and is
    // keyed once for them — but storage's is a standing holding rather than a
    // running total, so it needs its own key. Sharing it would caption a
    // holding with "Total" and claim a sum that group never reports.
    expect(metricsWindowLabelKey("arrival", "lifetime")).toBe("dataStatistics.window.lifetime");
    expect(metricsWindowLabelKey("volume", "lifetime")).toBe("dataStatistics.window.lifetime");
    expect(metricsWindowLabelKey("storage", "lifetime")).toBe(
      "dataStatistics.window.storage.lifetime",
    );
    // A window this file has never heard of reaches the caller as its own id
    // rather than as a key that would be handed to the operator verbatim.
    expect(metricsWindowLabelKey("volume", "last_30_days")).toBe("last_30_days");
  });

  it("treats a group's narrow windows as arrivals only where they are deltas", () => {
    // Reading a group decides two things at once — whether its narrow figures
    // are signed, and (through `metricsWindowLabelKey`) which catalog they are
    // labelled from — so the predicate is pinned rather than left to a literal
    // repeated at each call site.
    expect(isArrivalGroup("arrival")).toBe(true);
    expect(isArrivalGroup("storage")).toBe(true);
    // Traffic is a volume accumulated inside the window, not a change to
    // anything standing, so `+40 requests` would be a claim it never made.
    expect(isArrivalGroup("volume")).toBe(false);
  });

  it("states the storage vocabulary in the units the contract reports", () => {
    // The volume is formatted through the byte path only if the unit travels
    // with it, so a vocabulary that left the unit empty would render 4 GiB as
    // the count `4294967296`.
    expect(METRICS_STORAGE_METRICS).toEqual([
      { metric: "storage.used_bytes", unit: METRICS_UNIT_BYTE },
      { metric: "storage.object_count", unit: METRICS_UNIT_COUNT },
    ]);
    expect(metricLabelKey(METRICS_STORAGE_METRICS[0].metric)).toBe(
      "dataStatistics.metric.storage.used_bytes",
    );
    // Vocabulary-fed, so an empty storage plane still draws both cards at `0`.
    expect(
      buildMetricsCards([], METRICS_STORAGE_METRICS).map((card) => [
        card.metric,
        card.headline?.quantity,
        card.headline?.unit,
      ]),
    ).toEqual([
      ["storage.used_bytes", "0", "BYTE"],
      ["storage.object_count", "0", "COUNT"],
    ]);
  });

  it("keys a metric label without treating an unknown one as known", () => {
    expect(metricLabelKey("users")).toBe("dataStatistics.metric.users");
    expect(metricLabelKey("tenants")).toBe("dataStatistics.metric.tenants");
    // The traffic metrics are dimensions, so their labels come from the
    // dimension vocabulary instead — but the key is still well formed.
    expect(metricLabelKey("traffic.requests")).toBe("dataStatistics.metric.traffic.requests");
  });

  it("keeps the platform vocabulary a strict superset of the own-tenant one", () => {
    // The tenant count is the only difference, and it is the reason the two
    // reaches cannot share one vocabulary.
    expect([...METRICS_PLATFORM_ENTITY_METRICS]).toEqual([
      ...METRICS_ENTITY_METRICS.slice(0, 1),
      "tenants",
      ...METRICS_ENTITY_METRICS.slice(1),
    ]);
    expect(METRICS_ENTITY_METRICS).not.toContain("tenants");
  });

  it("states the day bounds per window, and none for the open one", () => {
    const basis = metricsWindowBasis([
      { window: "today", dateFrom: "2026-09-24", dateTo: "2026-09-25" },
      { window: "lifetime", dateTo: "2026-09-25" },
    ]);

    expect(basis.get("today")).toEqual({ from: "2026-09-24", to: "2026-09-25" });
    // The lifetime lower bound is "the beginning of the figures" and is reported
    // by the response's `trafficSince`, not invented here.
    expect(basis.get("lifetime")).toEqual({ from: null, to: "2026-09-25" });
    expect(basis.get("current_month")).toBeUndefined();
  });

  it("counts agents on both reaches and tenants on one", () => {
    // Agents are the estate's fourth subject and are not tenant-specific, so
    // both reaches ask for them: an own-tenant operator's agent count is
    // answerable for that tenant, exactly as its user count is. Tenants stay the
    // one difference between the vocabularies.
    expect(METRICS_ENTITY_METRICS).toContain(METRICS_ENTITY_AGENTS);
    expect(METRICS_PLATFORM_ENTITY_METRICS).toContain(METRICS_ENTITY_AGENTS);
    expect(METRICS_ENTITY_AGENTS).toBe("agents");

    // And the two reaches are the same command line up to the tenant count —
    // asserted on the ids rather than on the arrays, so a reordering does not
    // read as a vocabulary change.
    expect([...METRICS_PLATFORM_ENTITY_METRICS].filter((metric) => metric !== "tenants")).toEqual([
      ...METRICS_ENTITY_METRICS,
    ]);
  });

  it("names a metric the deployment could not read instead of drawing a zero", () => {
    // "This edge cannot count agents" and "there are no agents" are opposite
    // claims about the system, and both would be an absent row. The response is
    // the only thing that knows which one holds, so the card keeps its tile —
    // the row must not lose a column because a deployment lacks a module — and
    // carries no figures at all.
    const cards = buildMetricsCards([], entityVocabulary, [METRICS_ENTITY_AGENTS]);

    expect(cards.map((card) => card.metric)).toEqual([...METRICS_ENTITY_METRICS]);
    const agents = cards.find((card) => card.metric === METRICS_ENTITY_AGENTS);
    expect(agents).toEqual({
      metric: METRICS_ENTITY_AGENTS,
      unassembled: true,
      headline: null,
      narrower: [],
    });
    // The metrics the reading *did* take are still zero-filled from the
    // vocabulary: an unreadable neighbour must not turn the whole row into
    // "unknown".
    expect(cards.find((card) => card.metric === "users")?.unassembled).toBe(false);
    expect(cards.find((card) => card.metric === "users")?.headline?.quantity).toBe("0");
  });

  it("lets a metric be named unreadable and answer with rows, and withholds the rows", () => {
    // A response that named a metric unreadable *and* carried figures for it is
    // a contradiction. Drawing the figures would publish a number the same
    // response says it did not take, so the name wins — checked before the
    // figures rather than after, which is the only order that can be wrong
    // silently.
    const cards = buildMetricsCards([users(["1", "2", "2", "9"])], entityVocabulary, ["users"]);

    const drawn = cards.find((card) => card.metric === "users");
    expect(drawn?.unassembled).toBe(true);
    expect(drawn?.headline).toBeNull();
    expect(drawn?.narrower).toEqual([]);
  });
});

/**
 * The estate's own per-day series, drawn on the traffic reading's chart.
 *
 * Two readings, one plot: the strip holds metered dimensions and entity metrics
 * side by side and each id has to reach the catalog that owns it. These
 * assertions pin what that joint makes easy to get wrong — which vocabulary an
 * id belongs to, that a metric the window held nothing for is still a series,
 * and that both readings' days flow through one domain.
 */
describe("the estate's series on the trend chart", () => {
  it("reads an id's vocabulary off the two catalogs rather than off its spelling", () => {
    // The entity ids share no prefix and the metered ones are namespaced, so a
    // rule based on the shape of the string would pass today and break on the
    // next metric. What is asserted is membership, not a pattern.
    for (const metric of METRICS_ENTITY_METRICS) {
      expect(isEntityMetric(metric)).toBe(true);
    }
    for (const known of TRAFFIC_DIMENSIONS) {
      expect(isEntityMetric(known.dimension)).toBe(false);
    }
    // An id neither catalog names belongs to the metered vocabulary, which is
    // the open one — so it falls through to the dimension label, not to a crash.
    expect(isEntityMetric("traffic.new_dimension")).toBe(false);
    expect(isEntityMetric("")).toBe(false);
  });

  it("labels a series from the catalog that owns its id", () => {
    expect(seriesLabelKey("users")).toBe("dataStatistics.metric.users");
    expect(seriesLabelKey(METRICS_ENTITY_AGENTS)).toBe("dataStatistics.metric.agents");
    expect(seriesLabelKey("traffic.requests")).toBe(
      "dataStatistics.dimension.traffic.requests",
    );
    // The two catalogs are keyed apart, which is the whole point: labelled from
    // one of them, half the strip would show the key itself — or, worse, a label
    // that means something else in the other vocabulary.
    expect(seriesLabelKey("traffic.requests")).not.toBe(metricLabelKey("traffic.requests"));
    // An unknown id is still keyed, so the caller's fallback is the id itself.
    expect(seriesLabelKey("traffic.new_dimension")).toBe(
      "dataStatistics.dimension.traffic.new_dimension",
    );
  });

  it("carries a series the window held nothing for, and draws it at zero", () => {
    // The read model groups by day, so a metric with no arrivals carries no
    // points at all. It is still one of the response's series, and drawing it
    // flat at zero is what makes a quiet metric read as quiet rather than as a
    // tab that vanished exactly when it had nothing to show.
    const figures = entitySeriesFigures([
      { metric: "users", points: [{ date: "2026-08-26", quantity: "1" }] },
      { metric: METRICS_ENTITY_AGENTS, points: [] },
    ]);

    expect(figures).toEqual([
      { usageDate: "2026-08-26", dimension: "users", quantity: "1" },
    ]);

    const series = buildChartSeries(
      figures,
      ["2026-08-26", "2026-08-27"],
      [...METRICS_ENTITY_METRICS],
      new Map([["users", METRICS_UNIT_COUNT], [METRICS_ENTITY_AGENTS, METRICS_UNIT_COUNT]]),
    );

    // One series per vocabulary entry, in vocabulary order — the two readings
    // share one `orderedDimensions`, so the strip cannot order the entity ids
    // differently from the cards above it.
    expect(series.map((line) => line.dimension)).toEqual([...METRICS_ENTITY_METRICS]);
    const agents = series.find((line) => line.dimension === METRICS_ENTITY_AGENTS);
    // A day inside the domain with no point is a real `0`, and the domain comes
    // from the window rather than from the rows — so the empty series still has
    // one point per day.
    expect(agents?.points).toEqual([
      { day: "2026-08-26", value: 0 },
      { day: "2026-08-27", value: 0 },
    ]);
    expect(agents?.peak).toBe(0);
    expect(agents?.unit).toBe(METRICS_UNIT_COUNT);
  });

  it("plots a day the metrics reading carried outside the traffic window nowhere", () => {
    // The plot has one x domain, taken from the traffic reading's own resolved
    // window. A metric day outside it is unreachable while the page hands one
    // pair of bounds to both readings — and if it were reached, it is not
    // plotted rather than widening the axis and relabelling the days around it.
    const series = buildChartSeries(
      entitySeriesFigures([{ metric: "users", points: [{ date: "2026-07-01", quantity: "5" }] }]),
      ["2026-08-26", "2026-08-27"],
      ["users"],
      new Map([["users", METRICS_UNIT_COUNT]]),
    );

    expect(series[0]?.points).toEqual([
      { day: "2026-08-26", value: 0 },
      { day: "2026-08-27", value: 0 },
    ]);
    expect(series[0]?.peak).toBe(0);
  });
});
