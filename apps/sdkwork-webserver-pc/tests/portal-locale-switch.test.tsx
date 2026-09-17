// @vitest-environment jsdom

import {
  WebserverPortal,
  type PortalClipboardPort,
  type PortalLocale,
  type PortalNavigation,
} from "@sdkwork/webserver-pc-portal";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(cleanup);

const ACTIVE_LOCALES = ["zh-CN", "en-US"] as const;

const navigation = {
  consoleHref: "/console",
  createApplicationHref: "/console/sites",
  deploymentsHref: "/console/deployments",
  documentationHref: "/docs",
  notificationsHref: "http://127.0.0.1:5184/notifications",
} as const satisfies PortalNavigation;

const clipboard: PortalClipboardPort = { writeText: vi.fn().mockResolvedValue(undefined) };

describe("portal header language switch", () => {
  it("offers every active locale and reports the selected one", () => {
    const onLocaleChange = vi.fn();
    renderPortal({ locale: "zh-CN", onLocaleChange });

    const trigger = screen.getByRole("button", { name: "当前语言：简体中文" });
    // The control shares the page header with the primary navigation, so it is
    // part of the header rather than a secondary toolbar.
    const pageNavigation = screen.getByRole("navigation", { name: "Portal 导航" });
    expect(trigger.closest("header")).toBe(pageNavigation.closest("header"));
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(trigger.getAttribute("title")).toBe("语言");

    fireEvent.click(trigger);

    expect(screen.getByRole("button", { name: "当前语言：简体中文" }).getAttribute("aria-expanded")).toBe("true");
    const menu = screen.getByRole("menu", { name: "语言" });
    const options = within(menu).getAllByRole("menuitemradio");
    expect(options.map((option) => option.textContent)).toEqual(["简体中文", "English"]);
    expect(options[0]?.getAttribute("aria-checked")).toBe("true");
    expect(options[1]?.getAttribute("aria-checked")).toBe("false");

    fireEvent.click(options[1]!);

    expect(onLocaleChange).toHaveBeenCalledWith("en-US");
    expect(screen.queryByRole("menu", { name: "语言" })).toBeNull();
  });

  it("does not re-report the locale that is already active", () => {
    const onLocaleChange = vi.fn();
    renderPortal({ locale: "en-US", onLocaleChange });

    fireEvent.click(screen.getByRole("button", { name: "Current language: English" }));
    fireEvent.click(screen.getByRole("menuitemradio", { name: "English" }));

    expect(onLocaleChange).not.toHaveBeenCalled();
  });

  it("closes on Escape and on an outside pointer press", () => {
    renderPortal({ locale: "zh-CN", onLocaleChange: vi.fn() });

    fireEvent.click(screen.getByRole("button", { name: "当前语言：简体中文" }));
    expect(screen.getByRole("menu", { name: "语言" })).toBeTruthy();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu", { name: "语言" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "当前语言：简体中文" }));
    expect(screen.getByRole("menu", { name: "语言" })).toBeTruthy();
    // A press on the page background, outside the switch container.
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("menu", { name: "语言" })).toBeNull();
  });

  it("stays hidden when the host offers a single locale or no switch handler", () => {
    renderPortal({ availableLocales: ["zh-CN"] as const, locale: "zh-CN", onLocaleChange: vi.fn() });
    expect(screen.queryByRole("button", { name: /当前语言/ })).toBeNull();

    cleanup();
    renderPortal({ locale: "zh-CN" });
    expect(screen.queryByRole("button", { name: /当前语言/ })).toBeNull();
  });

  it("re-renders the whole portal in the newly selected language", () => {
    renderPortal({ controllable: true });

    expect(screen.getByRole("link", { name: "首页" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "当前语言：简体中文" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "当前语言：简体中文" }));
    fireEvent.click(screen.getByRole("menuitemradio", { name: "English" }));

    expect(screen.getByRole("link", { name: "Home" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Current language: English" })).toBeTruthy();
    expect(screen.queryByRole("link", { name: "首页" })).toBeNull();
    expect(screen.getByRole("heading", { level: 1, name: "SDKWork Web Server" })).toBeTruthy();
  });
});

function renderPortal({
  availableLocales = ACTIVE_LOCALES,
  controllable = false,
  locale = "zh-CN",
  onLocaleChange,
}: {
  availableLocales?: readonly PortalLocale[];
  controllable?: boolean;
  locale?: PortalLocale;
  onLocaleChange?: (locale: PortalLocale) => void;
}) {
  if (controllable) {
    return render(<ControllablePortal availableLocales={availableLocales} initialLocale={locale} />);
  }

  return render(
    <WebserverPortal
      availableLocales={availableLocales}
      clipboard={clipboard}
      locale={locale}
      navigation={navigation}
      onLocaleChange={onLocaleChange}
    />,
  );
}

/** Mirrors the shell wiring: the host owns the locale and feeds it back in. */
function ControllablePortal({
  availableLocales,
  initialLocale,
}: {
  availableLocales: readonly PortalLocale[];
  initialLocale: PortalLocale;
}) {
  const [locale, setLocale] = useState<PortalLocale>(initialLocale);

  return (
    <WebserverPortal
      availableLocales={availableLocales}
      clipboard={clipboard}
      locale={locale}
      navigation={navigation}
      onLocaleChange={setLocale}
    />
  );
}
