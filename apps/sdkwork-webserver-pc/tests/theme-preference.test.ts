// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";
import {
  commitWebserverTheme,
  resolveInitialWebserverTheme,
  WEBSERVER_THEME_COLOR,
  WEBSERVER_THEME_OVERRIDES,
  WEBSERVER_THEME_STORAGE_KEY,
} from "../src/bootstrap/theme.ts";

describe("webserver theme preference", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("uses the tech-blue application palette", () => {
    expect(WEBSERVER_THEME_COLOR).toBe("tech-blue");
    expect(WEBSERVER_THEME_OVERRIDES.border?.focus).toContain("37, 99, 235");
    expect(WEBSERVER_THEME_OVERRIDES.radius?.panel).toBe("0.5rem");
  });

  it("uses the system preference until a user makes a selection", () => {
    expect(resolveInitialWebserverTheme()).toBe("system");

    window.localStorage.setItem(WEBSERVER_THEME_STORAGE_KEY, "unsupported");
    expect(resolveInitialWebserverTheme()).toBe("system");
  });

  it("persists explicit light and dark selections", () => {
    expect(commitWebserverTheme("dark")).toBe("dark");
    expect(resolveInitialWebserverTheme()).toBe("dark");

    expect(commitWebserverTheme("light")).toBe("light");
    expect(window.localStorage.getItem(WEBSERVER_THEME_STORAGE_KEY)).toBe("light");
  });
});
