import { ArrowRight, Boxes, Rocket } from "lucide-react";
import type { PortalTranslator } from "../services/portal-translator.ts";
import type { PortalNavigation } from "../types.ts";

export function PortalClosing({ navigation, t }: { navigation: PortalNavigation; t: PortalTranslator }) {
  return (
    <>
      <section className="bg-blue-400 py-16 text-blue-950 sm:py-20">
        <div className="mx-auto flex max-w-[1280px] flex-col items-start justify-between gap-8 px-5 sm:px-7 lg:flex-row lg:items-end lg:px-10">
          <div className="max-w-[760px]">
            <span className="text-xs font-bold uppercase text-blue-900/70">{t("cta.eyebrow")}</span>
            <h2 className="mt-3 text-3xl font-bold leading-tight sm:text-4xl">{t("cta.title")}</h2>
            <p className="mt-4 max-w-[680px] leading-7 text-blue-950/75">{t("cta.description")}</p>
          </div>
          <div className="flex flex-wrap gap-3">
            <a className="inline-flex min-h-12 items-center gap-2 rounded bg-[#020617] px-6 text-sm font-bold text-white no-underline transition-colors hover:bg-black" href={navigation.consoleHref}>
              <Rocket aria-hidden="true" size={18} />
              {t("cta.primary")}
            </a>
            <a className="inline-flex min-h-12 items-center gap-2 rounded bg-blue-300 px-6 text-sm font-semibold text-blue-950 no-underline transition-colors hover:bg-blue-200" href={navigation.deploymentsHref}>
              {t("cta.secondary")}
              <ArrowRight aria-hidden="true" size={18} />
            </a>
          </div>
        </div>
      </section>
      <footer className="bg-[#020617] py-7 text-zinc-400">
        <div className="mx-auto flex max-w-[1280px] flex-col gap-3 px-5 text-xs sm:flex-row sm:items-center sm:justify-between sm:px-7 lg:px-10">
          <span className="flex items-center gap-2 font-bold text-white">
            <Boxes aria-hidden="true" className="text-blue-300" size={16} />
            {t("footer.product")}
          </span>
          <span>{t("footer.note")}</span>
        </div>
      </footer>
    </>
  );
}
