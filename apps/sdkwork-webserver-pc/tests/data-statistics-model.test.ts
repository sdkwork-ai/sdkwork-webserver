import { describe, expect, it } from "vitest";
import {
  buildTrendSeries,
  formatQuantity,
  isUnavailableReading,
  isEmptyReading,
  toQuantity,
} from "@sdkwork/webserver-pc-console-data-statistics";

/**
 * The presentation model behind the traffic readings.
 *
 * These assertions are about the contract's own hazards rather than about
 * formatting taste: int64 arrives as a string, dimensions are an open set, a
 * reading the edge could not produce is a different claim from a window with no
 * traffic in it, and the daily series must not be drawn on one scale.
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

  it("indexes each series to its own peak instead of sharing one axis", () => {
    const series = buildTrendSeries([
      { dimension: "traffic.requests", usageDate: "2026-09-02", quantity: "50" },
      { dimension: "traffic.requests", usageDate: "2026-09-01", quantity: "100" },
      { dimension: "traffic.egress_bytes", usageDate: "2026-09-01", quantity: "1048576" },
    ]);

    const requests = series.find((line) => line.dimension === "traffic.requests");
    const egress = series.find((line) => line.dimension === "traffic.egress_bytes");
    // Days are ordered, and each series peaks at 100 regardless of the other's
    // absolute magnitude — which is exactly why one shared axis would be wrong.
    expect(requests?.points).toEqual([
      ["2026-09-01", 100],
      ["2026-09-02", 50],
    ]);
    expect(requests?.peak).toBe(100);
    expect(egress?.points).toEqual([["2026-09-01", 100]]);
    expect(egress?.peak).toBe(1_048_576);
  });

  it("treats a series with no recoverable quantity as empty rather than zero", () => {
    const series = buildTrendSeries([
      { dimension: "traffic.requests", usageDate: "2026-09-01", quantity: "9007199254740993" },
    ]);

    expect(series).toEqual([]);
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
});
