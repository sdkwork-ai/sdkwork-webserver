import { Check, ChevronDown, Languages } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { portalLocaleNameKeys } from "../i18n/index.ts";
import type { PortalTranslator } from "../services/portal-translator.ts";
import type { PortalLocale } from "../types.ts";

/**
 * Header language switch for the public portal.
 *
 * The control is presentation-only: it reports the requested locale through
 * `onLocaleChange` and renders the locale the host hands back through
 * `locale`. Catalog selection, persistence, and `<html lang>` stay owned by
 * the application shell's i18n runtime (`I18N_SPEC.md` section 7), so the
 * portal never reads browser language or storage on its own.
 *
 * Locale names come from the message catalog rather than `Intl.DisplayNames`
 * so the label is identical on every ICU build and each language is offered
 * in its own script, which is what makes the control usable when the reader
 * cannot read the current locale.
 */
export function PortalLocaleSwitcher({
  locale,
  locales,
  onLocaleChange,
  t,
}: {
  locale: PortalLocale;
  locales: readonly PortalLocale[];
  onLocaleChange?: (locale: PortalLocale) => void;
  t: PortalTranslator;
}) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const currentLocaleName = t(portalLocaleNameKeys[locale]);

  useEffect(() => {
    if (!open) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  if (!onLocaleChange || locales.length < 2) {
    return null;
  }

  return (
    <div className="relative" ref={containerRef}>
      <button
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={t("header.localeSwitchCurrent", { language: currentLocaleName })}
        className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded bg-white/[0.07] px-2 text-zinc-200 transition-colors hover:bg-white/[0.12] hover:text-blue-300 focus-visible:bg-white/[0.12] focus-visible:text-blue-300"
        onClick={() => setOpen((current) => !current)}
        title={t("header.localeSwitch")}
        type="button"
      >
        <Languages aria-hidden="true" size={17} />
        <span className="hidden text-[13px] font-semibold sm:inline">{currentLocaleName}</span>
        <ChevronDown aria-hidden="true" className="hidden transition-transform sm:block" size={14} style={{ transform: open ? "rotate(180deg)" : undefined }} />
      </button>

      {open ? (
        <div
          aria-label={t("header.localeSwitch")}
          className="absolute right-0 top-[calc(100%+0.4rem)] z-50 w-40 overflow-hidden rounded-md border border-white/10 bg-[#0f172a] py-1 shadow-xl shadow-black/40"
          role="menu"
        >
          {locales.map((candidate) => {
            const selected = candidate === locale;
            return (
              <button
                aria-checked={selected}
                className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-[13px] font-medium text-zinc-300 transition-colors hover:bg-white/[0.08] hover:text-white focus-visible:bg-white/[0.08] focus-visible:text-white"
                key={candidate}
                onClick={() => {
                  setOpen(false);
                  if (!selected) {
                    onLocaleChange(candidate);
                  }
                }}
                role="menuitemradio"
                type="button"
              >
                <span>{t(portalLocaleNameKeys[candidate])}</span>
                {selected ? <Check aria-hidden="true" className="text-blue-300" size={15} /> : null}
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
