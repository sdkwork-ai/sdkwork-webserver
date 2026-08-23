import type {
  SdkworkThemeColor,
  SdkworkThemeOverrides,
  SdkworkThemeSelection,
} from "@sdkwork/ui-pc-react/theme";

export const WEBSERVER_THEME_STORAGE_KEY = "sdkwork.webserver.theme";
export const WEBSERVER_THEME_COLOR: SdkworkThemeColor = "tech-blue";
export const WEBSERVER_THEME_OVERRIDES: SdkworkThemeOverrides = {
  border: {
    focus: "rgba(37, 99, 235, 0.48)",
  },
  radius: {
    control: "0.375rem",
    field: "0.375rem",
    panel: "0.5rem",
  },
};

function isThemeSelection(value: string | null): value is SdkworkThemeSelection {
  return value === "dark" || value === "light" || value === "system";
}

export function resolveInitialWebserverTheme(): SdkworkThemeSelection {
  if (typeof window === "undefined") {
    return "system";
  }

  try {
    const storedTheme = window.localStorage.getItem(WEBSERVER_THEME_STORAGE_KEY);
    return isThemeSelection(storedTheme) ? storedTheme : "system";
  } catch {
    return "system";
  }
}

export function commitWebserverTheme(
  theme: SdkworkThemeSelection,
): SdkworkThemeSelection {
  try {
    window.localStorage.setItem(WEBSERVER_THEME_STORAGE_KEY, theme);
  } catch {
    // Storage can be unavailable in privacy-restricted browser contexts.
  }
  return theme;
}
