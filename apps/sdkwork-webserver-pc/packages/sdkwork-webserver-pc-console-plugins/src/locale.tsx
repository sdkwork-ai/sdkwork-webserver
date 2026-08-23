import { createContext, useCallback, useContext, type ReactNode } from "react";
import {
  normalizePluginsLocale,
  translatePlugins,
  type PluginsLocale,
  type PluginsMessageKey,
} from "./i18n.ts";

const PluginsLocaleContext = createContext<PluginsLocale>("en-US");

export function PluginsLocaleProvider({
  children,
  locale,
}: {
  children: ReactNode;
  locale?: string | null;
}) {
  return (
    <PluginsLocaleContext.Provider value={normalizePluginsLocale(locale)}>
      {children}
    </PluginsLocaleContext.Provider>
  );
}

export function usePluginsT() {
  const locale = useContext(PluginsLocaleContext);
  return useCallback(
    (key: PluginsMessageKey, values: Record<string, string | number> = {}) =>
      translatePlugins(locale, key, values),
    [locale],
  );
}
