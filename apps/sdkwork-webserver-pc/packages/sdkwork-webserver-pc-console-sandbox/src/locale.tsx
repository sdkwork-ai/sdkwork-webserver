import { createContext, useCallback, useContext, type ReactNode } from "react";
import {
  normalizeSandboxInstancesLocale,
  translateSandboxInstances,
  type SandboxInstancesLocale,
  type SandboxInstancesMessageKey,
} from "./i18n.ts";

const SandboxInstancesLocaleContext = createContext<SandboxInstancesLocale>("en-US");

export function SandboxInstancesLocaleProvider({
  children,
  locale,
}: {
  children: ReactNode;
  locale?: string | null;
}) {
  return (
    <SandboxInstancesLocaleContext.Provider value={normalizeSandboxInstancesLocale(locale)}>
      {children}
    </SandboxInstancesLocaleContext.Provider>
  );
}

export type SandboxInstancesTranslator = (
  key: SandboxInstancesMessageKey,
  values?: Record<string, string | number>,
) => string;

export function useSandboxInstancesT(): SandboxInstancesTranslator {
  const locale = useContext(SandboxInstancesLocaleContext);
  return useCallback(
    (key: SandboxInstancesMessageKey, values: Record<string, string | number> = {}) =>
      translateSandboxInstances(locale, key, values),
    [locale],
  );
}
