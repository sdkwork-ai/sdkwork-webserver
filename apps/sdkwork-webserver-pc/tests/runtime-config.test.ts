import { describe, expect, it, vi } from "vitest";
import {
  loadWebserverPcRuntimeConfig,
  parseWebserverPcRuntimeConfig,
} from "@sdkwork/webserver-pc-core";

const locales = {
  activeLocales: ["zh-CN", "en-US"],
  defaultLocale: "zh-CN",
  fallbackLocale: "en-US",
  supportedLocales: ["zh-CN", "en-US"],
};

const standaloneConfig = {
  ...locales,
  appApiBaseUrl: "/",
  appbaseAppApiBaseUrl: "/",
  backendApiBaseUrl: "/",
  browserOriginMode: "same-origin",
  deploymentProfile: "standalone",
  driveAppApiBaseUrl: "/",
  environment: "development",
  messagingPcUrl: "http://127.0.0.1:5184/notifications",
  profileId: "standalone.development",
  runtimeTarget: "browser",
};

describe("webserver runtime config", () => {
  it("resolves every standalone SDK root from the explicit browser origin", () => {
    const config = parseWebserverPcRuntimeConfig(
      standaloneConfig,
      "http://127.0.0.1:5217",
    );

    expect(config).toMatchObject({
      appApiBaseUrl: "http://127.0.0.1:5217",
      appbaseAppApiBaseUrl: "http://127.0.0.1:5217",
      backendApiBaseUrl: "http://127.0.0.1:5217",
      browserOriginMode: "same-origin",
      driveAppApiBaseUrl: "http://127.0.0.1:5217",
      messagingPcUrl: "http://127.0.0.1:5184/notifications",
      profileId: "standalone.development",
      runtimeTarget: "browser",
    });
  });

  it("rejects an absolute standalone SDK root even when it matches the browser", () => {
    expect(() => parseWebserverPcRuntimeConfig({
      ...standaloneConfig,
      appApiBaseUrl: "http://127.0.0.1:5217",
    }, "http://127.0.0.1:5217")).toThrow(/canonical standalone same-origin root/);
  });

  it("rejects standalone config when the browser origin is unavailable", () => {
    expect(() => parseWebserverPcRuntimeConfig(standaloneConfig)).toThrow(/browser origin is required/);
  });

  it("rejects a deployment profile and browser origin mode mismatch", () => {
    expect(() => parseWebserverPcRuntimeConfig({
      ...standaloneConfig,
      browserOriginMode: "cross-origin",
    }, "http://127.0.0.1:5217")).toThrow(/standalone browserOriginMode/);
  });

  it("loads public config from the page origin before returning SDK roots", async () => {
    const fetcher = vi.fn().mockResolvedValue({
      json: async () => standaloneConfig,
      ok: true,
    });

    const config = await loadWebserverPcRuntimeConfig(
      fetcher as unknown as typeof fetch,
      "http://127.0.0.1:5217",
    );

    expect(fetcher).toHaveBeenCalledWith("/runtime-env.json", {
      cache: "no-store",
      credentials: "same-origin",
    });
    expect(config.appbaseAppApiBaseUrl).toBe("http://127.0.0.1:5217");
  });

  it("accepts explicit cross-origin cloud SDK roots", () => {
    const config = parseWebserverPcRuntimeConfig({
      ...locales,
      appApiBaseUrl: "https://server-app-dev.sdkwork.com",
      appbaseAppApiBaseUrl: "https://api-dev.sdkwork.com",
      backendApiBaseUrl: "https://server-admin-dev.sdkwork.com",
      browserOriginMode: "cross-origin",
      deployAppApiBaseUrl: "https://deploy-app-dev.sdkwork.com",
      deploymentProfile: "cloud",
      driveAppApiBaseUrl: "https://api-dev.sdkwork.com",
      environment: "development",
      messagingPcUrl: "https://messaging-dev.sdkwork.com/notifications",
      profileId: "cloud.development",
      runtimeTarget: "browser",
    });

    expect(config.browserOriginMode).toBe("cross-origin");
    expect(config.deployAppApiBaseUrl).toBe("https://deploy-app-dev.sdkwork.com");
  });

  it("rejects production cloud loopback endpoints", () => {
    expect(() => parseWebserverPcRuntimeConfig({
      ...locales,
      appApiBaseUrl: "http://127.0.0.1:8080",
      appbaseAppApiBaseUrl: "https://api.sdkwork.com",
      backendApiBaseUrl: "https://server.sdkwork.com",
      browserOriginMode: "cross-origin",
      deployAppApiBaseUrl: "https://deploy-app.sdkwork.com",
      deploymentProfile: "cloud",
      driveAppApiBaseUrl: "https://api.sdkwork.com",
      environment: "production",
      messagingPcUrl: "https://messaging.sdkwork.com/notifications",
      profileId: "cloud.production",
      runtimeTarget: "browser",
    })).toThrow(/loopback/);
  });

  it("rejects a production notification center loopback URL", () => {
    expect(() => parseWebserverPcRuntimeConfig({
      ...locales,
      appApiBaseUrl: "https://server.sdkwork.com",
      appbaseAppApiBaseUrl: "https://api.sdkwork.com",
      backendApiBaseUrl: "https://server.sdkwork.com",
      browserOriginMode: "cross-origin",
      deployAppApiBaseUrl: "https://deploy-app.sdkwork.com",
      deploymentProfile: "cloud",
      driveAppApiBaseUrl: "https://api.sdkwork.com",
      environment: "production",
      messagingPcUrl: "http://127.0.0.1:5184/notifications",
      profileId: "cloud.production",
      runtimeTarget: "browser",
    })).toThrow(/messagingPcUrl cannot use a loopback host/);
  });
});
