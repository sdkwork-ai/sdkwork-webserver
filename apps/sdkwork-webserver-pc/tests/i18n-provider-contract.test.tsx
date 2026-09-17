// @vitest-environment jsdom

import {
  SdkworkI18nProvider,
  useSdkworkI18n,
  useSdkworkModuleMessages,
} from "@sdkwork/i18n-pc-react";
import { parseWebserverPcRuntimeConfig } from "@sdkwork/webserver-pc-core";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  createWebserverI18nRuntimeConfig,
  webserverApplicationCatalog,
} from "../src/i18n/index.ts";

afterEach(() => {
  cleanup();
  document.documentElement.lang = "";
});

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

/**
 * Proves the shell's provider wiring, not just the catalog contents: a shell
 * that passes `locale` as an authoritative prop would change language once and
 * then snap back, so the live tag, the document language, and the resolved
 * shell copy are all asserted after the switch.
 */
describe("shell i18n provider wiring", () => {
  it("keeps the switched locale live across the provider, the document, and shell copy", async () => {
    render(
      <SdkworkI18nProvider
        catalogs={[webserverApplicationCatalog]}
        config={createWebserverI18nRuntimeConfig(config)}
        locale="zh-CN"
        syncDocumentLanguage
      >
        <LocaleProbe />
      </SdkworkI18nProvider>,
    );

    expect(screen.getByTestId("locale-tag").textContent).toBe("zh-CN");
    expect(screen.getByTestId("shell-copy").textContent).toBe("正在加载工作台");
    await waitFor(() => expect(document.documentElement.lang).toBe("zh-CN"));

    fireEvent.click(screen.getByRole("button", { name: "switch" }));

    expect(screen.getByTestId("locale-tag").textContent).toBe("en-US");
    expect(screen.getByTestId("shell-copy").textContent).toBe("Loading workspace");
    await waitFor(() => expect(document.documentElement.lang).toBe("en-US"));

    // A later render must not restore the bootstrap locale.
    fireEvent.click(screen.getByRole("button", { name: "rerender" }));
    expect(screen.getByTestId("locale-tag").textContent).toBe("en-US");
    expect(document.documentElement.lang).toBe("en-US");
  });

  it("normalizes an unsupported requested locale onto the deployment default", () => {
    render(
      <SdkworkI18nProvider
        catalogs={[webserverApplicationCatalog]}
        config={createWebserverI18nRuntimeConfig(config)}
        locale="fr-FR"
        syncDocumentLanguage
      >
        <LocaleProbe />
      </SdkworkI18nProvider>,
    );

    expect(screen.getByTestId("locale-tag").textContent).toBe("zh-CN");
    expect(screen.getByTestId("shell-copy").textContent).toBe("正在加载工作台");
  });
});

function LocaleProbe() {
  const i18n = useSdkworkI18n();
  const messages = useSdkworkModuleMessages(webserverApplicationCatalog);

  return (
    <div>
      <output data-testid="locale-tag">{i18n?.localeTag}</output>
      <output data-testid="shell-copy">{messages["shell.status.loadingWorkspace"]}</output>
      <button onClick={() => void i18n?.changeLocale("en-US")} type="button">switch</button>
      <button onClick={() => undefined} type="button">rerender</button>
    </div>
  );
}
