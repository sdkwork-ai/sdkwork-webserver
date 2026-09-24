/**
 * Presentation model for the traffic readings.
 *
 * Four rules drive everything in this file, and all four come from the
 * contract rather than from taste:
 *
 * 1. **Quantities are int64 and cross the wire as strings** (API_SPEC §13.6).
 *    Every helper narrows exactly once, and a quantity that is not a safe
 *    integer is reported unformatted rather than rounded — a dashboard that
 *    quietly rounds a big number is worse than one that shows the raw digits.
 * 2. **Dimensions are open.** The contract reports whatever dimensions the
 *    facts carry (`traffic.requests`, `traffic.ingress_bytes`, …) instead of
 *    rejecting an unknown one, so the surface labels the ones the catalog knows
 *    and falls back to the wire value for everything else. A new metered
 *    dimension therefore reaches the surface as its own row instead of
 *    disappearing behind a vocabulary this file would have to be extended for.
 * 3. **"Not assembled" is not "no traffic".** A reading the edge could not
 *    produce answers `503`, and drawing that as an empty chart would assert
 *    something the response never said. `isUnavailableReading` is what keeps
 *    the two apart, and it does not depend on class identity because the
 *    generated client may resolve `SdkError` through its own copy.
 * 4. **An answered window is drawn whole.** Whatever the figures are — a
 *    populated window, or one that summed to `0` in every dimension — the page
 *    renders its full frame: `TRAFFIC_DIMENSIONS` fixes the card and series
 *    vocabulary so a quiet window is a dashboard of zeros rather than a
 *    sentence, and an open set still appends whatever else the wire carried.
 *    Only "not assembled" and "the request failed" replace the frame, because
 *    only those two have no figures to draw.
 */

/** A wire quantity, narrowed to a number only when it is exactly representable. */
export function toQuantity(value: string): number | null {
  const trimmed = value.trim();
  if (!/^-?\d+$/.test(trimmed)) {
    return null;
  }
  const parsed = Number(trimmed);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

/**
 * Decimal scaling, because the contract's byte unit is `BYTE` and the surfaces
 * that produce it (edge response sizes, ingress/egress counters) are quoted in
 * powers of ten everywhere else in the product.
 */
export function formatBytes(bytes: number, locale: string): string {
  let value = Math.abs(bytes);
  let unitIndex = 0;
  while (value >= 1000 && unitIndex < BYTE_UNITS.length - 1) {
    value /= 1000;
    unitIndex += 1;
  }
  const scaled = bytes < 0 ? -value : value;
  const rounded = unitIndex === 0 ? scaled : Math.round(scaled * 10) / 10;
  const formatted = new Intl.NumberFormat(locale, {
    maximumFractionDigits: unitIndex === 0 ? 0 : 1,
  }).format(rounded);
  return `${formatted} ${BYTE_UNITS[unitIndex]}`;
}

export function formatCount(count: number, locale: string): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(count);
}

/**
 * Formats a wire quantity in the unit the contract reported alongside it.
 *
 * The metering plane publishes exactly two units — `BYTE` for the two traffic
 * dimensions and `REQUEST` for the request counter — and the unit travels with
 * every row, so this branches on the wire value rather than on which dimension
 * it happens to be attached to. Anything that is not `BYTE` is a count.
 */
export function formatQuantity(quantity: string, unit: string, locale: string): string {
  const parsed = toQuantity(quantity);
  if (parsed === null) {
    return quantity;
  }
  return unit === "BYTE" ? formatBytes(parsed, locale) : formatCount(parsed, locale);
}

/** UTC day `offsetDays` from today, as `YYYY-MM-DD` — the shape the query takes. */
export function utcDay(offsetDays: number): string {
  const day = new Date();
  day.setUTCDate(day.getUTCDate() + offsetDays);
  return day.toISOString().slice(0, 10);
}

/**
 * Catalog key for a dimension label.
 *
 * Returned as a plain string rather than a keyof union: the dimension set is
 * open, so a lookup for an unknown one is expected and the caller decides the
 * fallback. That is the same shape `resourceText` uses for resource keys.
 */
export function dimensionLabelKey(dimension: string): string {
  return `dataStatistics.dimension.${dimension}`;
}

/**
 * The metering plane's own dimension vocabulary, in the order an overview
 * reads it: the request counter, then the two byte counters.
 *
 * The facts owner aggregates with `GROUP BY dimension`, so the wire order is
 * alphabetical (`traffic.egress_bytes`, `traffic.ingress_bytes`,
 * `traffic.requests`) — stable, but not a meaningful order, and one that shifts
 * the moment a fourth dimension is metered. Ordering by this vocabulary instead
 * is what keeps the cards and the series reading the same way in every state,
 * including the empty one where there is nothing but the vocabulary left to
 * order by. It is the same three constants the edge's data plane meters
 * (`usage_metering.rs`).
 */
export const TRAFFIC_DIMENSIONS = [
  { dimension: "traffic.requests", unit: "REQUEST" },
  { dimension: "traffic.ingress_bytes", unit: "BYTE" },
  { dimension: "traffic.egress_bytes", unit: "BYTE" },
] as const;

/** The vocabulary's unit for a dimension, `""` for one this file has not
 *  heard of. */
export function dimensionUnit(dimension: string): string {
  return TRAFFIC_DIMENSIONS.find((known) => known.dimension === dimension)?.unit ?? "";
}

/** Dimensions the vocabulary does not name, deduplicated, in wire order. */
function unknownDimensions(present: readonly string[]): readonly string[] {
  const known = new Set(TRAFFIC_DIMENSIONS.map((entry) => entry.dimension as string));
  return [...new Set(present.filter((dimension) => !known.has(dimension)))];
}

/** The vocabulary's order for the dimensions named, then the rest as given. */
function orderedDimensions<Dimensions extends readonly string[]>(dimensions: Dimensions): readonly string[] {
  const given = new Set<string>(dimensions);
  return [
    ...TRAFFIC_DIMENSIONS.map((entry) => entry.dimension).filter((dimension) => given.has(dimension)),
    ...unknownDimensions(dimensions),
  ];
}

export interface TrafficSummaryCard {
  dimension: string;
  quantity: string;
  unit: string;
}

/**
 * The overview's summary cards: the dimensions asked for, plus whatever else
 * the reading carried.
 *
 * `overview` is the set the surface asks for — the whole vocabulary normally,
 * or the one dimension an operator filtered to. A dimension in that set the
 * window carried **no row for is a real `0`**, not a placeholder: the read
 * model groups the facts by dimension, so no row means the window summed no
 * facts of that dimension, and its card is what keeps the overview the same
 * overview when nothing was served. The filter case is the one that must *not*
 * be widened — the other dimensions were excluded from the read rather than
 * measured at zero, and a `0` card for them would report a figure the response
 * never made.
 *
 * A wire quantity is carried through untouched, including one this layer cannot
 * narrow: `toQuantity` reports `null` for a malformed quantity and for one past
 * `Number.MAX_SAFE_INTEGER` alike, so rewriting either into `0` would assert
 * "no traffic" about a number the surface simply could not read.
 */
export function buildSummaryCards(
  totals: readonly { dimension: string; quantity: string; unit: string }[],
  overview: readonly string[],
): readonly TrafficSummaryCard[] {
  const byDimension = new Map(totals.map((total) => [total.dimension, total] as const));
  return orderedDimensions([...overview, ...unknownDimensions(totals.map((total) => total.dimension))]).map(
    (dimension) => {
      const wire = byDimension.get(dimension);
      return {
        dimension,
        quantity: wire?.quantity ?? "0",
        // The contract leaves the unit to the producer and the facts owner
        // defaults a missing one to the empty string, under which a byte
        // quantity would be formatted as a count. The vocabulary is the
        // fallback for the dimensions it knows.
        unit: wire?.unit || dimensionUnit(dimension),
      };
    },
  );
}

/** The two ways the daily trend can be drawn. */
export const CHART_KINDS = ["bar", "line"] as const;
export type ChartKind = (typeof CHART_KINDS)[number];

/**
 * One day of a series.
 *
 * `value: null` is a day the wire *did* carry a row for but whose quantity this
 * layer cannot narrow (`toQuantity` reports `null` for a malformed quantity and
 * for anything past `Number.MAX_SAFE_INTEGER` alike). It is deliberately not
 * read as `0`: a gap says "this figure could not be read", where a zero says
 * "nothing happened", and the two are different claims about the deployment.
 * A day the wire carried **no row** for is a real `0` — see `buildChartSeries`.
 */
export interface ChartPoint {
  day: string;
  value: number | null;
}

/** One axis label: the figure, and where it sits on the plot as a fraction of
 *  the axis, `0` at the floor and `1` at the top. */
export interface ChartTick {
  value: number;
  fraction: number;
}

export interface ChartSeries {
  dimension: string;
  unit: string;
  points: readonly ChartPoint[];
  /** The largest plottable value, `0` for a series that has none. */
  peak: number;
  /** The axis top: a round number at or above the peak, so the labels read as
   *  figures an operator would write down rather than as the data's own maximum. */
  top: number;
  /** Bottom-up, always including the floor. */
  ticks: readonly ChartTick[];
}

/** How many intervals an axis is aimed at — one more tick than this. */
const AXIS_INTERVALS = 4;

/**
 * The axis for a peak: a round top and the ticks under it.
 *
 * **Every step is a whole number.** The labels are rendered with
 * `formatQuantity`, whose count branch rounds to zero decimals, so a fractional
 * step would print the same label twice on adjacent gridlines — the axis would
 * claim two different heights are the same figure. Bytes are metered in whole
 * bytes too, so an integer step is never wrong for them either.
 *
 * A peak of `0` — an answered window in which every day summed to nothing — is
 * a *real* axis whose top is `0`: one tick on the floor, which is what keeps the
 * empty window's chart the same chart with the same frame rather than a blank.
 */
export function buildAxis(peak: number): { top: number; ticks: readonly ChartTick[] } {
  const floorTick: readonly ChartTick[] = [{ value: 0, fraction: 0 }];
  if (!Number.isFinite(peak) || peak <= 0) {
    return { top: 0, ticks: floorTick };
  }
  const rough = peak / AXIS_INTERVALS;
  const magnitude = 10 ** Math.max(0, Math.floor(Math.log10(Math.max(rough, 1))));
  let step = Math.max(1, Math.round(magnitude * 10));
  for (const multiple of [1, 2, 2.5, 5, 10]) {
    const candidate = Math.max(1, Math.round(magnitude * multiple));
    if (candidate >= rough) {
      step = candidate;
      break;
    }
  }
  const top = Math.ceil(peak / step) * step;
  const ticks: ChartTick[] = [];
  for (let value = 0; value <= top; value += step) {
    ticks.push({ value, fraction: value / top });
  }
  return { top, ticks };
}

/**
 * The entity reading's per-day series, in the shape the chart's series builder
 * takes.
 *
 * An adapter rather than a second chart pipeline. The two readings produce the
 * same thing — one figure per day per named series — so they flow through one
 * implementation of the domain, the axis, and the bars; a second one would let
 * the trend drawn from the entity reading disagree with the trend beside it
 * about what a day with no rows means.
 *
 * A metric with no points is carried through as an empty series rather than
 * dropped: the contract reports one entry per assembled metric whatever the
 * window held, and `buildChartSeries` is what turns the window's days into the
 * zeroes a quiet window is drawn from. Dropping it here would make the tab
 * disappear exactly when its line is flat at zero.
 */
export function entitySeriesFigures(
  series: readonly { metric: string; points: readonly { date: string; quantity: string }[] }[],
): readonly { usageDate: string; dimension: string; quantity: string }[] {
  return series.flatMap((entry) =>
    entry.points.map((point) => ({
      usageDate: point.date,
      dimension: entry.metric,
      quantity: point.quantity,
    })),
  );
}

/**
 * One series per dimension, on **the axis of its own unit**.
 *
 * Not a shared axis, and not an index either. Requests and bytes differ by
 * orders of magnitude, so one scale would make their relative height a property
 * of the units rather than of the data; the previous revision answered that by
 * indexing every series to its own peak, which made the series comparable to
 * nothing at all — a bar of `100` carried no figure. This draws a single series
 * at a time, against `buildAxis`, so every height on screen is a real quantity.
 * Switching between them is the surface's tab strip, not a shared scale.
 *
 * `vocabulary` is the set the frame always draws — the metering vocabulary, or
 * the one dimension an operator filtered to, or the entity metrics the metric
 * reading reported. A dimension in it the window carried no row for gets a
 * series of its own anyway, over the window's days at `0`: the read model groups
 * the facts by day, so no row means that day summed nothing, and a missing
 * series is what would make a quiet window read as a broken chart. It is the
 * same rule as `buildSummaryCards`, and for the same reason. Dimensions the
 * window carried that the vocabulary does not name are appended, so a newly
 * metered dimension appears as its own series instead of disappearing.
 *
 * `daily` may hold **either** reading's days — the metered dimensions and the
 * entity metrics arrive from two operations with two windows, and the surface
 * hands them the same window so both are plotted against one domain. The two are
 * distinguishable only by id (`isEntityMetric`), which is what the label lookup
 * uses; nothing here branches on which reading a point came from, because a
 * branch would be a second chart.
 *
 * `days` is the x domain, taken from the response's own window rather than from
 * the days that happen to have rows: a dimension metered on five days of a
 * seven-day window has two days of *no facts*, and plotting it against its own
 * five days would draw a busy week. A window whose bounds are unusable falls
 * back to the days the rows themselves carry — one domain for every series, so
 * the charts stay aligned — because vanishing beats misaligning.
 */
export function buildChartSeries(
  daily: readonly { usageDate: string; dimension: string; quantity: string }[],
  days: readonly string[],
  vocabulary: readonly string[],
  units: ReadonlyMap<string, string>,
): readonly ChartSeries[] {
  const byDimension = new Map<string, Map<string, number | null>>();
  for (const point of daily) {
    const bucket = byDimension.get(point.dimension) ?? new Map<string, number | null>();
    byDimension.set(point.dimension, bucket);
    bucket.set(point.usageDate, toQuantity(point.quantity));
  }
  const domain =
    days.length > 0 ? [...days] : [...new Set(daily.map((point) => point.usageDate))].sort();
  const dimensions = orderedDimensions([
    ...vocabulary,
    ...daily.map((point) => point.dimension),
  ]);
  return dimensions.map((dimension) => {
    const bucket = byDimension.get(dimension) ?? new Map<string, number | null>();
    const points: ChartPoint[] = domain.map((day) => ({
      day,
      value: bucket.has(day) ? (bucket.get(day) ?? null) : 0,
    }));
    const peak = points.reduce(
      (highest, point) => (point.value === null ? highest : Math.max(highest, point.value)),
      0,
    );
    const { top, ticks } = buildAxis(peak);
    return { dimension, unit: units.get(dimension) ?? dimensionUnit(dimension), points, peak, top, ticks };
  });
}

/**
 * Which series the tab strip has selected.
 *
 * Kept as a function because the interesting case is not the first visit but
 * the *re-read*: an operator who has switched to a dimension and then widens
 * the window, or applies a dimension filter, gets a response whose series set
 * may no longer hold the one they picked. Falling back to the first series is
 * right; falling back to *nothing* would take the chart away under a person who
 * only asked for a different window, and keeping the stale name would draw an
 * empty plot under a tab that is no longer there.
 */
export function resolveChartDimension(
  series: readonly { dimension: string }[],
  selected: string | null,
): string | null {
  if (selected !== null && series.some((entry) => entry.dimension === selected)) {
    return selected;
  }
  return series[0]?.dimension ?? null;
}

/** A plotted bar, in the plot's own `0–100` space. `y` is measured **up from
 *  the floor**, so `height` is that same number and the renderer flips it. */
export interface ChartBar {
  day: string;
  value: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A plotted point, in the same space and with the same floor-up convention. */
export interface ChartMark {
  day: string;
  value: number;
  x: number;
  y: number;
}

export interface ChartPlot {
  bars: readonly ChartBar[];
  /**
   * Runs of consecutive plottable days, in order.
   *
   * A run breaks where a day could not be read, which is the whole reason this
   * is a list of runs rather than a list of points: joining across a gap would
   * draw a slope through a period nobody measured, which is the one thing a
   * chart must not invent. A run of one is a lone mark.
   */
  segments: ReadonlyArray<readonly ChartMark[]>;
}

/**
 * The geometry for one series, as percentages of the plot box.
 *
 * Both shapes are computed from the same marks: a bar is a mark widened to its
 * slot, a line is the marks joined inside a run. The caller picks which to draw
 * — the geometry does not depend on the chart type, and computing it once is
 * what keeps the bar and the line showing the operator *the same data*.
 *
 * Bars divide the width into `n` slots and tile it; points spread across the
 * full width, because a line has no width of its own to reserve. Both put the
 * first and last day flush against the plot's edges.
 *
 * A lone day is the one case the two cannot share: a single slot would fill the
 * plot with one column, which reads as a filled panel rather than as a figure.
 * The slot is therefore floored at the width two days would give — so a one-day
 * window draws the same column a two-day window would, centred under the same
 * mark the line puts there.
 */
export function buildChartPlot(series: ChartSeries | null): ChartPlot {
  if (series === null) {
    return { bars: [], segments: [] };
  }
  const { points, top } = series;
  const plottable = points.length;
  const scale = (value: number): number => (top > 0 ? (value / top) * 100 : 0);
  const bars: ChartBar[] = [];
  const segments: ChartMark[][] = [];
  let run: ChartMark[] = [];
  for (const [index, point] of points.entries()) {
    if (point.value === null) {
      if (run.length > 0) {
        segments.push(run);
        run = [];
      }
      continue;
    }
    const y = scale(point.value);
    // Widened to its slot, floored at the two-day width so a lone day is a
    // column rather than a panel (see the note above).
    const slot = 100 / Math.max(plottable, 2);
    bars.push({
      day: point.day,
      value: point.value,
      x: plottable > 1 ? index * slot : 50 - slot / 2,
      y,
      width: slot,
      height: y,
    });
    run.push({
      day: point.day,
      value: point.value,
      x: plottable > 1 ? (index / (plottable - 1)) * 100 : 50,
      y,
    });
  }
  if (run.length > 0) {
    segments.push(run);
  }
  return { bars, segments };
}

/** The contract's bound on how many days one read may span (`MAX_TRAFFIC_USAGE_WINDOW_DAYS`). */
const MAX_WINDOW_DAYS = 366;

/**
 * The days an answered window covers, as `YYYY-MM-DD`.
 *
 * `dateTo` is exclusive in the contract, so the range is half-open and holds
 * exactly the span the response reported. The bound is the contract's own
 * `MAX_TRAFFIC_USAGE_WINDOW_DAYS`: the service rejects a longer window, and a
 * client handed one anyway must not try to draw it.
 */
export function buildWindowDays(dateFrom: string, dateTo: string): readonly string[] {
  const start = Date.parse(`${dateFrom}T00:00:00Z`);
  const end = Date.parse(`${dateTo}T00:00:00Z`);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) {
    return [];
  }
  const days: string[] = [];
  for (let at = start; at < end && days.length < MAX_WINDOW_DAYS; at += 86_400_000) {
    days.push(new Date(at).toISOString().slice(0, 10));
  }
  return days;
}

/**
 * Whether a failure is the edge saying it has no reading to give, as opposed to
 * a failed *attempt* at one.
 *
 * Both `Unavailable` ("no usage read model is assembled in this deployment")
 * and `DatabaseUnavailable` are mapped to `50301 SERVICE_UNAVAILABLE` by the
 * route layer, and the client-facing `detail` is masked before it is sent, so
 * this deliberately does **not** try to name the cause: it reports the class of
 * the answer and lets the copy say that the cause is not distinguished. Read
 * structurally — `code`/`httpStatus`/`problem.status` — because the generated
 * client reaches `SdkError` through its own module instance and an `instanceof`
 * check across that boundary is not dependable.
 */
export function isUnavailableReading(reason: unknown): boolean {
  if (typeof reason !== "object" || reason === null) {
    return false;
  }
  const candidate = reason as {
    code?: unknown;
    httpStatus?: unknown;
    problem?: { status?: unknown; code?: unknown } | undefined;
  };
  if (candidate.code === "SERVICE_UNAVAILABLE" || candidate.httpStatus === 503) {
    return true;
  }
  return candidate.problem?.status === 503 || candidate.problem?.code === 50301;
}

/**
 * Whether a reading that *did* answer carries no facts at all.
 *
 * Only reachable once a response has arrived: a `503` never produces one, which
 * is what keeps "the edge has nothing to measure with" out of this predicate.
 * The daily series is the authority rather than the totals, because a window
 * can hold days whose dimensions all cancelled to zero while the totals carry
 * dimensions with no series behind them — either way, no day and no non-zero
 * total is the shape of "nothing happened in this window".
 *
 * A total this layer cannot narrow is **not** read as zero: `toQuantity` reports
 * `null` both for a malformed quantity and for one past `Number.MAX_SAFE_INTEGER`,
 * and declaring such a window empty would assert "no traffic" on the strength of
 * a number the surface is unable to read. Only an exact `0` counts.
 *
 * The surface reads this twice, and both readings are about the *copy* rather
 * than about the frame: it selects the "no traffic in this window" note, and it
 * licenses the zero baseline the trend block draws — a chart of the window's own
 * days at `0`, which is a claim only a window with no facts at all supports.
 */
export function isEmptyReading(reading: {
  daily: readonly unknown[];
  totals: readonly { quantity: string }[];
}): boolean {
  if (reading.daily.length > 0) {
    return false;
  }
  return reading.totals.every((total) => toQuantity(total.quantity) === 0);
}

/** Message for a failure, preferring the problem detail the service sent. */
export function failureMessage(reason: unknown): string {
  if (reason instanceof Error) {
    return reason.message;
  }
  return String(reason);
}

/**
 * The dashboard's cardinal metric row.
 *
 * A second reading with its own vocabulary, deliberately not folded into the
 * traffic helpers above: those describe *one* window that the operator chose,
 * while a metric card reports *four* windows the server resolved. Merging the
 * two would give one function two meanings for "window", which is how a card
 * ends up labelled with a period its figure was not cut against.
 */

/**
 * The four reporting windows, narrowest first.
 *
 * Mirrors the contract's own vocabulary, and the order is the point: a card
 * reads left to right from "today" out to "everything", so the wire order — or
 * the order a caller happened to assemble its array in — must not decide how
 * the row reads.
 */
export const METRICS_WINDOWS = ["today", "last_7_days", "current_month", "lifetime"] as const;

/**
 * The window whose figure is a standing total rather than an arrival inside the
 * window. Always the widest, and the one a card uses as its headline.
 */
export const METRICS_LIFETIME_WINDOW = "lifetime";

/** Metric id: the agents the tenant has created. */
export const METRICS_ENTITY_AGENTS = "agents";

/**
 * The entity metrics the own-tenant reading reports.
 *
 * `agents` is in the vocabulary rather than appended when the response happens
 * to carry it, so a deployment that *can* count agents but has none draws a real
 * `0` card — and a deployment that cannot count them at all names the metric in
 * `unassembledMetrics`, which the surface draws as a missing capability. Only
 * the reading knows which of the two it is; a surface that inferred it from an
 * absent row would be guessing between opposite claims.
 */
export const METRICS_ENTITY_METRICS = ["users", "applications", "agents"] as const;

/** Unit of every entity metric: a count of rows, not a metered quantity.
 *  Mirrors the contract's `METRICS_UNIT_COUNT`. */
export const METRICS_UNIT_COUNT = "COUNT";

/** Unit of the storage volume metric: whole bytes. Mirrors the contract's
 *  `METRICS_UNIT_BYTE`, and the same unit the metering plane publishes for its
 *  two byte dimensions — so `formatQuantity` formats both through one path. */
export const METRICS_UNIT_BYTE = "BYTE";

/**
 * The storage group's vocabulary: the volume, then what occupies it.
 *
 * The figure is sdkwork-drive's own occupancy definition — `SUM(content_length)`
 * over the objects Drive still holds — which is the same number Drive's quota
 * service reports and refuses an upload against. Neither reach withholds it: a
 * tenant's own consumption is answerable for that tenant.
 */
export const METRICS_STORAGE_METRICS = [
  { metric: "storage.used_bytes", unit: METRICS_UNIT_BYTE },
  { metric: "storage.object_count", unit: METRICS_UNIT_COUNT },
] as const;

/**
 * The entity metrics of the platform reading: the own-tenant pair plus tenants.
 *
 * A tenant counting itself is structurally one, so the own-tenant vocabulary
 * omits it rather than reporting a constant — and the omission is enforced
 * server-side too, so a surface cannot reintroduce the noise by asking.
 */
export const METRICS_PLATFORM_ENTITY_METRICS = ["users", "tenants", "applications", "agents"] as const;

/**
 * Whether a series id is an entity metric rather than a metered dimension.
 *
 * Read off the two vocabularies rather than off a prefix: the entity ids carry
 * no shared prefix (`users`, `applications`, `agents`), and the metered ones are
 * namespaced (`traffic.…`), so a rule based on the shape of the string would be
 * a coincidence that a fourth metric could break. This is the one place the two
 * vocabularies are compared, so the chart's tab labels cannot disagree with the
 * cards about which catalog a name comes from.
 */
export function isEntityMetric(metric: string): boolean {
  return (
    (METRICS_ENTITY_METRICS as readonly string[]).includes(metric) ||
    (METRICS_PLATFORM_ENTITY_METRICS as readonly string[]).includes(metric)
  );
}

/**
 * Catalog key for a label on the trend chart, whichever vocabulary the id
 * belongs to.
 *
 * The chart draws both readings on one plot, so its tab strip holds entity ids
 * and metered dimensions side by side. They are labelled from different
 * catalogs (`dataStatistics.metric.*` versus `dataStatistics.dimension.*`), and
 * picking one catalog for both would show an operator the key itself for half
 * the tabs — or, worse, reuse a label that means something else in the other
 * vocabulary.
 */
export function seriesLabelKey(id: string): string {
  return isEntityMetric(id) ? metricLabelKey(id) : dimensionLabelKey(id);
}

/**
 * Which vocabulary a window label is drawn from.
 *
 * The three groups share the window *ids* but not their meaning: a narrow window
 * counts arrivals for the entity metrics, accumulated volume for traffic, and
 * the part of a standing holding that was added for storage. Keying the label
 * by group is what keeps one card from saying "Today" where its neighbour says
 * "New today" about the same period.
 */
export type MetricsGroup = "arrival" | "volume" | "storage";

/**
 * Whether a group's narrow windows are arrivals — figures that say how much
 * *entered* during the window — and so must be signed.
 *
 * The entity group's arrivals are new subjects and the storage group's are new
 * bytes, so both read as a delta (`+2` users, `+1.2 GB`). The traffic group is a
 * volume the window accumulated, which is not a change to anything standing.
 */
export function isArrivalGroup(group: MetricsGroup): boolean {
  return group === "arrival" || group === "storage";
}

/** Catalog key for an entity metric's label. */
export function metricLabelKey(metric: string): string {
  return `dataStatistics.metric.${metric}`;
}

/**
 * Catalog key for a window's label under a group.
 *
 * The suffix table exists because the wire vocabulary is `lower_snake_case`
 * (`last_7_days`) while the catalog keys are camelCase (`last7Days`); deriving
 * one from the other mechanically would produce a key that no catalog has, and
 * `translateWebserver` would hand the key itself to the operator.
 *
 * The widest window is group-dependent too. Storage's is a *standing* figure
 * rather than a running total — a deleted object stops counting in every window,
 * including the one it was born in — so labelling it "Total" would claim a sum
 * this group never reports.
 */
export function metricsWindowLabelKey(group: MetricsGroup, window: string): string {
  if (window === METRICS_LIFETIME_WINDOW) {
    return group === "storage"
      ? "dataStatistics.window.storage.lifetime"
      : "dataStatistics.window.lifetime";
  }
  const suffix = METRICS_WINDOW_KEY_SUFFIX[window];
  return suffix ? `dataStatistics.window.${group}.${suffix}` : window;
}

const METRICS_WINDOW_KEY_SUFFIX: Record<string, string> = {
  today: "today",
  last_7_days: "last7Days",
  current_month: "currentMonth",
};

/** One window's figure for one metric, still in its wire shape. */
export interface MetricsCardValue {
  window: string;
  quantity: string;
  unit: string;
}

/** One metric as a card: a headline total over its narrower windows. */
export interface MetricsCard {
  metric: string;
  /**
   * Whether this deployment **cannot read** the metric at all.
   *
   * Not a metric that came back empty — that one is a real `0` in every window.
   * This is the case where the source is not assembled, so there is no figure
   * and none may be invented. The card is still drawn, with its name, so the
   * row keeps its frame; what changes is that its figures read as "not
   * available here" rather than as zeroes, which is the same distinction the
   * absent reading (`503`) draws for the whole row.
   */
  unassembled: boolean;
  /**
   * The widest window's figure.
   *
   * `null` when the response made no claim about it — a card is then drawn with
   * its sub-row only, rather than promoting a narrower window to the headline
   * and presenting, say, the month's arrivals as the standing total. It is also
   * `null` for an unassembled metric, which has no figures at all.
   */
  headline: MetricsCardValue | null;
  /** The narrower windows, in vocabulary order, for the card's sub-row. */
  narrower: readonly MetricsCardValue[];
}

/**
 * The metric cards, in vocabulary order, with the vocabulary filled in.
 *
 * `vocabulary` is the set the surface asks for — the reach's own entity metrics,
 * or the metering vocabulary for the traffic group. A metric in that set the
 * response carried **no row for is a real `0`**: the boot probe has already
 * ruled out "the read model is missing", so a metric with no row summed nothing
 * here, and its card is what keeps the row the same row on a deployment that
 * has counted nothing yet.
 *
 * Two cases are deliberately *not* collapsed into that zero:
 *
 * - a metric the response carried but without one of the four windows. The
 *   contract reports all four, so this is a violation, and a `0` would invent a
 *   figure for a period nobody measured — the window is dropped instead.
 * - a metric in neither the vocabulary nor the response. It has no card at all,
 *   which is what lets a new metered dimension appear as its own card instead
 *   of being silently absent.
 *
 * A third case is not a zero either, and it is the reason `unassembled` exists:
 * a metric the response **names** as unreadable. That one keeps its card — the
 * row's frame must not lose a tile because a deployment lacks a module — but
 * carries no figures, so nothing on screen claims "none" about a count that was
 * never taken.
 */
export function buildMetricsCards(
  readings: readonly { metric: string; unit: string; values: readonly MetricsCardValue[] }[],
  vocabulary: readonly { metric: string; unit: string }[],
  unassembled: readonly string[] = [],
): readonly MetricsCard[] {
  const reported = new Map(readings.map((reading) => [reading.metric, reading] as const));
  const vocabularyMetrics = new Set(vocabulary.map((entry) => entry.metric));
  const unreadable = new Set(unassembled);
  const metrics = [
    ...vocabulary.map((entry) => entry.metric),
    ...readings
      .map((reading) => reading.metric)
      .filter((metric) => !vocabularyMetrics.has(metric)),
  ];
  return metrics.flatMap((metric): readonly MetricsCard[] => {
    // Checked before the figures, not after: a response that named a metric
    // unreadable *and* carried rows for it is a contradiction, and drawing the
    // rows would publish a figure the same response says it could not take.
    if (unreadable.has(metric)) {
      return [{ metric, unassembled: true, headline: null, narrower: [] }];
    }
    const wire = reported.get(metric);
    const fallbackUnit = vocabulary.find((entry) => entry.metric === metric)?.unit ?? "";
    const byWindow = new Map((wire?.values ?? []).map((value) => [value.window, value] as const));
    const values = METRICS_WINDOWS.flatMap<MetricsCardValue>((window) => {
      const hit = byWindow.get(window);
      if (hit) {
        // A row that reports no unit — the contract lets the producer leave it
        // empty — would format a byte quantity as a count, so the vocabulary's
        // unit for that metric is the fallback.
        return [{ window, quantity: hit.quantity, unit: hit.unit || fallbackUnit }];
      }
      return wire ? [] : [{ window, quantity: "0", unit: fallbackUnit }];
    });
    if (values.length === 0) {
      return [];
    }
    return [
      {
        metric,
        unassembled: false,
        headline: values.find((value) => value.window === METRICS_LIFETIME_WINDOW) ?? null,
        narrower: values.filter((value) => value.window !== METRICS_LIFETIME_WINDOW),
      },
    ];
  });
}

/**
 * The day bounds behind each window id, as the response stated them.
 *
 * Kept as a lookup so a card can state the basis under its figures instead of
 * only naming it: "近 7 日" is a label, `2026-09-18 → 2026-09-25` is a claim an
 * operator can check. A window the response did not describe is simply absent
 * from the map, and the caller draws the label without a basis rather than
 * inventing one.
 */
export function metricsWindowBasis(
  windows: readonly { window: string; dateFrom?: string; dateTo: string }[],
): ReadonlyMap<string, { from: string | null; to: string }> {
  return new Map(
    windows.map((bounds) => [
      bounds.window,
      { from: bounds.dateFrom ?? null, to: bounds.dateTo },
    ] as const),
  );
}

/**
 * The sign an arrival window's figure carries, `""` for one that must not.
 *
 * A narrow entity window counts what arrived inside it, so `+2` says "two more
 * exist than did before this period" where a bare `2` under a standing total
 * reads as a second population. The two exceptions are the point of this being
 * a function rather than an inline ternary:
 *
 * - **`0` is left unsigned.** `+0` reads as a change, and nothing changed.
 * - **A quantity this layer cannot narrow is left unsigned too**, because an
 *   unreadable figure must not be embellished into a delta it never reported.
 */
export function arrivalSign(quantity: string): string {
  const parsed = toQuantity(quantity);
  return parsed !== null && parsed > 0 ? "+" : "";
}
