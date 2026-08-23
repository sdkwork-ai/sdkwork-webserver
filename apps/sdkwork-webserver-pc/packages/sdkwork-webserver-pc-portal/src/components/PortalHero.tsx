import { ArrowRight, BookOpen, CheckCircle2, CloudCog, Rocket } from "lucide-react";
import { portalAgentCount } from "../data/portal-agent-catalog.ts";
import type { PortalTranslator } from "../services/portal-translator.ts";
import type { PortalClipboardPort, PortalNavigation, PortalStatisticsPort } from "../types.ts";
import { CloudTopologyScene } from "./CloudTopologyScene.tsx";
import { PortalStatistics } from "./PortalStatistics.tsx";
import { SkillIntegrationPanel } from "./SkillIntegrationPanel.tsx";

export function PortalHero({
  clipboard,
  navigation,
  statistics,
  t,
}: {
  clipboard: PortalClipboardPort;
  navigation: PortalNavigation;
  statistics?: PortalStatisticsPort;
  t: PortalTranslator;
}) {
  return (
    <section className="overflow-hidden bg-[#020617] text-white">
      <div className="mx-auto max-w-[1280px] px-5 pb-0 pt-12 sm:px-7 sm:pt-16 lg:px-10 lg:pt-20">
        <div className="grid items-center gap-12 lg:grid-cols-[minmax(0,1.18fr)_minmax(430px,0.82fr)] xl:gap-16">
          <div className="min-w-0 py-2 lg:py-8">
            <div className="mb-6 flex items-center gap-3 text-xs font-bold uppercase text-blue-300">
              <CloudCog aria-hidden="true" size={16} />
              {t("hero.kicker")}
              <span aria-hidden="true" className="h-px w-12 bg-blue-300/45" />
            </div>
            <h1 className="m-0 max-w-[720px] text-[42px] font-bold leading-[1.08] sm:text-[54px] lg:text-[62px]">
              {t("hero.title")}
            </h1>
            <p className="mt-6 max-w-[680px] text-base leading-8 text-zinc-300 sm:text-lg">
              {t("hero.description")}
            </p>
            <div className="mt-7 flex flex-wrap gap-3">
              <a className="inline-flex min-h-12 items-center gap-2 rounded bg-blue-400 px-6 text-sm font-bold text-blue-950 no-underline transition-colors hover:bg-blue-300" href={navigation.createApplicationHref}>
                <Rocket aria-hidden="true" size={18} />
                {t("hero.primary")}
              </a>
              <a className="inline-flex min-h-12 items-center gap-2 rounded bg-white/[0.08] px-6 text-sm font-semibold text-white no-underline transition-colors hover:bg-white/[0.14]" href={navigation.deploymentsHref}>
                {t("hero.secondary")}
                <ArrowRight aria-hidden="true" size={18} />
              </a>
              <a className="inline-flex min-h-12 items-center gap-2 px-2 text-sm font-semibold text-blue-100 no-underline transition-colors hover:text-white" href={navigation.documentationHref}>
                <BookOpen aria-hidden="true" size={18} />
                {t("hero.documentation")}
              </a>
            </div>
            <div className="mt-5 flex items-center gap-2 text-sm text-blue-100">
              <CheckCircle2 aria-hidden="true" size={17} />
              {t("hero.availability")}
            </div>
          </div>

          <SkillIntegrationPanel clipboard={clipboard} t={t} />
        </div>

        <PortalStatistics agentCount={portalAgentCount} statistics={statistics} t={t} />
        <CloudTopologyScene t={t} />
      </div>
    </section>
  );
}
