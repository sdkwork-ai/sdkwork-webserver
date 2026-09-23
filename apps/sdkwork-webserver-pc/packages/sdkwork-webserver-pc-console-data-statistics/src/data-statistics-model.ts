/**
 * Presentation model for the traffic readings.
 *
 * Three rules drive everything in this file, and all three come from the
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

export interface TrendSeries {
  dimension: string;
  /** `[day, index]` pairs, each series indexed to its own peak (=100). */
  points: readonly (readonly [string, number])[];
  /** The series' peak, kept so the surface can state what 100 means. */
  peak: number;
}

/**
 * One series per dimension, each indexed to its own peak.
 *
 * Deliberately **not** a shared axis: requests and bytes differ by orders of
 * magnitude, so drawing them to one scale makes their relative height a
 * property of the units rather than of the data — the chart would look
 * informative while carrying no comparable information. Indexing each series to
 * its own peak keeps every series' *shape* readable, and the footnote says so.
 */
export function buildTrendSeries(
  daily: readonly { usageDate: string; dimension: string; quantity: string }[],
): readonly TrendSeries[] {
  const byDimension = new Map<string, { day: string; value: number }[]>();
  for (const point of daily) {
    const value = toQuantity(point.quantity);
    if (value === null) {
      continue;
    }
    const bucket = byDimension.get(point.dimension);
    if (bucket) {
      bucket.push({ day: point.usageDate, value });
    } else {
      byDimension.set(point.dimension, [{ day: point.usageDate, value }]);
    }
  }
  const series: TrendSeries[] = [];
  for (const [dimension, points] of byDimension) {
    const ordered = [...points].sort((left, right) => left.day.localeCompare(right.day));
    const peak = ordered.reduce((highest, point) => Math.max(highest, point.value), 0);
    series.push({
      dimension,
      peak,
      points: ordered.map((point) => [
        point.day,
        peak === 0 ? 0 : Math.round((point.value / peak) * 100),
      ] as const),
    });
  }
  // Declaration order is the wire order per dimension, which is stable across
  // reads of the same window; the surface renders them in that order.
  return series;
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
