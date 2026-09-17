import { describe, expect, it } from "vitest";

import {
  createWebserverH5MessageCatalog,
  normalizeWebserverH5Locale,
  webserverCommonsMessageSources,
} from "@sdkwork/webserver-h5-commons";
import { webserverApplicationsMessageSources } from "@sdkwork/webserver-h5-applications";

import { resolveInitialWebserverH5Locale } from "../src/bootstrap/locale.ts";

const supported = ["en-US", "zh-CN"] as const;

describe("h5 locale negotiation", () => {
  it("keeps an exact supported tag", () => {
    expect(normalizeWebserverH5Locale("zh-CN", supported)).toBe("zh-CN");
    expect(normalizeWebserverH5Locale("en-US", supported)).toBe("en-US");
  });

  it("collapses a richer tag onto its primary subtag", () => {
    expect(normalizeWebserverH5Locale("zh-Hans-CN", supported)).toBe("zh-CN");
    expect(normalizeWebserverH5Locale("en-GB", supported)).toBe("en-US");
  });

  it("rejects a locale the deployment does not ship", () => {
    expect(normalizeWebserverH5Locale("ja-JP", supported)).toBeUndefined();
    expect(normalizeWebserverH5Locale("", supported)).toBeUndefined();
  });

  it("prefers the stored preference over the browser list, then the default", () => {
    expect(
      resolveInitialWebserverH5Locale({
        defaultLocale: "en-US",
        preferredLocales: ["zh-CN"],
        storedPreference: "en-US",
        supportedLocales: supported,
      }),
    ).toBe("en-US");
    expect(
      resolveInitialWebserverH5Locale({
        defaultLocale: "en-US",
        preferredLocales: ["zh-Hans-CN", "en-US"],
        supportedLocales: supported,
      }),
    ).toBe("zh-CN");
    expect(
      resolveInitialWebserverH5Locale({
        defaultLocale: "en-US",
        preferredLocales: ["ja-JP"],
        supportedLocales: supported,
      }),
    ).toBe("en-US");
  });
});

describe("h5 message catalog", () => {
  const catalog = createWebserverH5MessageCatalog({
    sources: [webserverCommonsMessageSources, webserverApplicationsMessageSources],
  });

  it("resolves contributed fragments per locale", () => {
    expect(catalog.resolve("zh-CN", "navigation.applications")).toBe("应用");
    expect(catalog.resolve("en-US", "navigation.applications")).toBe("Applications");
  });

  it("falls back to the fallback locale instead of blanking out", () => {
    expect(catalog.resolve("ja-JP", "chrome.brand")).toBe("SDKWork Web Server");
  });

  it("returns the key itself for an unknown key rather than an empty string", () => {
    expect(catalog.resolve("en-US", "missing.key")).toBe("missing.key");
    expect(catalog.has("missing.key")).toBe(false);
  });

  it("lets a later contribution override an earlier one", () => {
    const overridden = createWebserverH5MessageCatalog({
      sources: [
        webserverCommonsMessageSources,
        { "en-US": { "navigation.applications": "Apps" } },
      ],
    });
    expect(overridden.resolve("en-US", "navigation.applications")).toBe("Apps");
  });
});
