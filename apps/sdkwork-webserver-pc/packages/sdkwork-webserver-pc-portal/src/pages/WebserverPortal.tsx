import { useMemo } from "react";
import { CapabilityBand } from "../components/CapabilityBand.tsx";
import { DeploymentWorkflow } from "../components/DeploymentWorkflow.tsx";
import { PortalClosing } from "../components/PortalClosing.tsx";
import { PortalHeader } from "../components/PortalHeader.tsx";
import { PortalHero } from "../components/PortalHero.tsx";
import { SecurityBand } from "../components/SecurityBand.tsx";
import { createPortalTranslator } from "../services/portal-translator.ts";
import type { WebserverPortalProps } from "../types.ts";

export function WebserverPortal({ clipboard, locale, navigation, statistics, viewer }: WebserverPortalProps) {
  const t = useMemo(() => createPortalTranslator(locale), [locale]);

  return (
    <div className="min-h-screen bg-white text-zinc-950 dark:bg-[#020617] dark:text-white">
      <PortalHeader navigation={navigation} t={t} viewer={viewer} />
      <main>
        <PortalHero clipboard={clipboard} navigation={navigation} statistics={statistics} t={t} />
        <CapabilityBand t={t} />
        <DeploymentWorkflow t={t} />
        <SecurityBand t={t} />
        <PortalClosing navigation={navigation} t={t} />
      </main>
    </div>
  );
}
