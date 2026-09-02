import {
  formatWebserverErrorMessage,
  translateWebserver,
  WebserverActionError,
  type WebserverLocale,
  type WebserverMessageKey,
} from "@sdkwork/webserver-pc-commons";
import { describe, expect, it } from "vitest";

function translator(locale: WebserverLocale) {
  return (key: WebserverMessageKey, values?: Record<string, string | number>) => (
    translateWebserver(locale, key, values)
  );
}

describe("webserver error messages", () => {
  it("presents validation fields and a support reference from Problem Details", () => {
    const message = formatWebserverErrorMessage({
      code: "VALIDATION_ERROR",
      details: [{ field: "/name", message: "An application with this name already exists" }],
      httpStatus: 422,
      problem: {
        code: 40001,
        detail: "Validation failed",
        status: 422,
        traceId: "trace-create-40001",
      },
    }, translator("zh-CN"));

    expect(message).toContain("部分输入未通过校验");
    expect(message).toContain("name: An application with this name already exists");
    expect(message).toContain("支持参考号：trace-create-40001");
  });

  it.each([
    [40101, 401, "sign-in session"],
    [40301, 403, "does not have permission"],
    [40901, 409, "duplicate record"],
    [42901, 429, "Too many operations"],
    [50001, 500, "support reference"],
  ])("maps result code %i to actionable copy", (code, status, expected) => {
    const message = formatWebserverErrorMessage({
      code: status >= 500 ? "SERVER_ERROR" : "BUSINESS_ERROR",
      httpStatus: status,
      problem: { code, detail: "internal implementation detail", status },
    }, translator("en-US"));

    expect(message).toContain(expected);
    if (status === 401 || status === 403 || status >= 500) {
      expect(message).not.toContain("internal implementation detail");
    }
  });

  it("distinguishes network failures, timeouts, and cancellations", () => {
    expect(formatWebserverErrorMessage(
      { code: "NETWORK_ERROR" },
      translator("en-US"),
    )).toContain("network connection");
    expect(formatWebserverErrorMessage(
      { code: "TIMEOUT" },
      translator("en-US"),
    )).toContain("may still be processing");
    expect(formatWebserverErrorMessage(
      { message: "The operation was aborted", name: "AbortError" },
      translator("en-US"),
    )).toContain("cancelled");
  });

  it("keeps staged recovery guidance and appends a safe nested cause", () => {
    const error = new WebserverActionError(
      "application-draft-source-failed",
      { applicationId: "application-42" },
      {
        cause: {
          code: "SERVICE_UNAVAILABLE",
          httpStatus: 503,
          problem: { code: 50301, status: 503, traceId: "trace-source-50301" },
        },
      },
    );
    const message = formatWebserverErrorMessage(error, translator("zh-CN"));

    expect(message).toContain("应用 application-42 已创建为草稿");
    expect(message).toContain("服务暂时不可用");
    expect(message).toContain("支持参考号：trace-source-50301");
  });

  it("uses safe 4xx detail without exposing SQL, secrets, stacks, or raw 5xx detail", () => {
    const safeConflict = formatWebserverErrorMessage({
      httpStatus: 409,
      problem: {
        code: 60004,
        detail: "Application name 'Portal' is already in use",
        status: 409,
      },
    }, translator("en-US"));
    expect(safeConflict).toContain("Application name 'Portal' is already in use");

    const unsafe = formatWebserverErrorMessage({
      code: "SERVER_ERROR",
      httpStatus: 500,
      problem: {
        code: 50001,
        detail: "sqlx: SELECT token FROM sessions; password=super-secret",
        errors: [
          { field: "dependency", message: "connection reset by peer" },
          { field: "token", message: "access_token=secret-value" },
        ],
        status: 500,
        traceId: "trace-redacted-50001",
      },
    }, translator("en-US"));
    expect(unsafe).not.toMatch(/sqlx|SELECT|super-secret|secret-value|connection reset/);
    expect(unsafe).toContain("Support reference: trace-redacted-50001");
  });

  it("unwraps ordinary error causes and falls back safely for unknown exceptions", () => {
    const nested = new Error("wrapper", {
      cause: {
        code: "CONFLICT",
        httpStatus: 409,
        problem: { code: 40901, status: 409 },
      },
    });
    expect(formatWebserverErrorMessage(nested, translator("zh-CN"))).toContain("重复记录");
    expect(formatWebserverErrorMessage(
      new Error("database password=do-not-show"),
      translator("zh-CN"),
    )).toBe("操作未能完成。请稍后重试；若持续失败请联系管理员。");
  });
});
