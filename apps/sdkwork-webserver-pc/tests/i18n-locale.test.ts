import { assertSdkworkCatalogLocaleParity } from "@sdkwork/i18n-pc-react";
import {
  commitWebserverLocalePreference,
  isActiveWebserverLocale,
  parseWebserverPcRuntimeConfig,
  readWebserverLocalePreference,
  resolveInitialWebserverLocale,
  resolveWebserverLocale,
  toWebserverLocale,
  WEBSERVER_LOCALE_STORAGE_KEY,
  type WebserverLocaleStorage,
} from "@sdkwork/webserver-pc-core";
import { describe, expect, it } from "vitest";
import {
  createWebserverI18nRuntimeConfig,
  webserverApplicationCatalog,
} from "../src/i18n/index.ts";

const config = parseWebserverPcRuntimeConfig(
  {
    activeLocales: ["zh-CN", "en-US"],
    appApiBaseUrl: "/",
    appbaseAppApiBaseUrl: "/",
    backendApiBaseUrl: "/",
    browserOriginMode: "same-origin",
    defaultLocale: "zh-CN",
    deploymentProfile: "standalone",
    driveAppApiBaseUrl: "/",
    environment: "development",
    fallbackLocale: "en-US",
    messagingPcUrl: "http://127.0.0.1:5184/notifications",
    profileId: "standalone.development",
    runtimeTarget: "browser",
    supportedLocales: ["zh-CN", "en-US"],
  },
  "http://127.0.0.1:5217",
);

function createMemoryStorage(initial: Record<string, string> = {}) {
  const entries = new Map(Object.entries(initial));
  return {
    entries,
    getItem: (key: string) => entries.get(key) ?? null,
    setItem: (key: string, value: string) => {
      entries.set(key, value);
    },
  } satisfies WebserverLocaleStorage & { entries: Map<string, string> };
}

describe("webserver locale runtime", () => {
  it("narrows unsupported and malformed locales onto an active deployment locale", () => {
    expect(toWebserverLocale("en-GB", config)).toBe("en-US");
    expect(toWebserverLocale("zh-Hans", config)).toBe("zh-CN");
    expect(toWebserverLocale("fr-FR", config)).toBe("zh-CN");
    expect(toWebserverLocale("", config)).toBe("zh-CN");
    expect(toWebserverLocale(null, config)).toBe("zh-CN");
  });

  it("reports whether a runtime locale is one this deployment can render", () => {
    expect(isActiveWebserverLocale("zh-CN", config)).toBe(true);
    expect(isActiveWebserverLocale("en-US", config)).toBe(true);
    expect(isActiveWebserverLocale("fr-FR", config)).toBe(false);
    expect(isActiveWebserverLocale(undefined, config)).toBe(false);
  });

  it("keeps negotiated browser language resolution unchanged", () => {
    expect(resolveWebserverLocale(config, ["en-GB", "zh-CN"])).toBe("en-US");
    expect(resolveWebserverLocale(config, ["fr-FR"])).toBe("zh-CN");
  });

  it("ranks a stored user preference above browser negotiation", () => {
    const storage = createMemoryStorage({ [WEBSERVER_LOCALE_STORAGE_KEY]: "en-US" });

    expect(resolveInitialWebserverLocale(config, ["zh-CN"], storage)).toBe("en-US");
  });

  it("falls back to browser negotiation when no preference is stored", () => {
    const storage = createMemoryStorage();

    expect(resolveInitialWebserverLocale(config, ["en-GB"], storage)).toBe("en-US");
    expect(resolveInitialWebserverLocale(config, ["zh-Hant"], storage)).toBe("zh-CN");
    expect(resolveInitialWebserverLocale(config, ["fr-FR"], storage)).toBe("zh-CN");
  });

  it("round-trips the explicit preference through storage", () => {
    const storage = createMemoryStorage();

    expect(readWebserverLocalePreference(storage)).toBeUndefined();
    expect(commitWebserverLocalePreference("en-US", storage)).toBe("en-US");
    expect(readWebserverLocalePreference(storage)).toBe("en-US");
    expect(resolveInitialWebserverLocale(config, ["zh-CN"], storage)).toBe("en-US");
  });

  it("treats unavailable or failing storage as no preference instead of throwing", () => {
    const throwingStorage = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    };

    expect(readWebserverLocalePreference(null)).toBeUndefined();
    expect(readWebserverLocalePreference(undefined)).toBeUndefined();
    expect(readWebserverLocalePreference(throwingStorage)).toBeUndefined();
    expect(commitWebserverLocalePreference("zh-CN", throwingStorage)).toBe("zh-CN");
    expect(resolveInitialWebserverLocale(config, ["en-US"], throwingStorage)).toBe("en-US");
  });
});

describe("webserver application i18n catalog", () => {
  it("keeps every active locale on the same message keys", () => {
    expect(() => assertSdkworkCatalogLocaleParity(webserverApplicationCatalog)).not.toThrow();
  });

  it("resolves shell copy per locale and falls back for an unknown locale", () => {
    const zh = webserverApplicationCatalog.resolveMessages("zh-CN");
    const en = webserverApplicationCatalog.resolveMessages("en-US");
    const unknown = webserverApplicationCatalog.resolveMessages("fr-FR");

    expect(zh["shell.status.loadingWorkspace"]).toBe("正在加载工作台");
    expect(en["shell.status.loadingWorkspace"]).toBe("Loading workspace");
    expect(zh["shell.error.runtimeUnavailable"]).not.toBe(en["shell.error.runtimeUnavailable"]);
    // An unmatched locale resolves through the catalog default, which mirrors
    // the deployment-wide `fallbackLocale: en-US`.
    expect(unknown["shell.status.loadingWorkspace"]).toBe(en["shell.status.loadingWorkspace"]);
  });

  it("projects the deployment locale strategy onto the i18n runtime config", () => {
    const runtimeConfig = createWebserverI18nRuntimeConfig(config);

    expect(runtimeConfig.activeLocales).toEqual(["zh-CN", "en-US"]);
    expect(runtimeConfig.defaultLocale).toBe("zh-CN");
    expect(runtimeConfig.fallbackLocale).toBe("en-US");
    expect(runtimeConfig.supportedLocales).toEqual(["zh-CN", "en-US"]);
  });

  it("fails closed when the deployment locale set is self-inconsistent", () => {
    expect(() =>
      createWebserverI18nRuntimeConfig({ ...config, defaultLocale: "en-US", supportedLocales: ["zh-CN"] }),
    ).toThrow();
  });
});
