import { Boxes } from "lucide-react";
import { useMemo } from "react";
import { DocumentationContent } from "../components/DocumentationContent.tsx";
import { DocumentationHeader } from "../components/DocumentationHeader.tsx";
import { DocumentationSidebar } from "../components/DocumentationSidebar.tsx";
import { createDocumentationTranslator } from "../services/documentation-translator.ts";
import type { WebserverDocumentationProps } from "../types.ts";

export function WebserverDocumentation({
  locale,
  navigation,
  supportedAgents,
  viewer,
}: WebserverDocumentationProps) {
  const t = useMemo(() => createDocumentationTranslator(locale), [locale]);

  return (
    <div className="min-h-screen bg-white text-zinc-950 dark:bg-[#020617] dark:text-white">
      <DocumentationHeader navigation={navigation} t={t} viewer={viewer} />
      <div className="mx-auto grid max-w-[1600px] lg:grid-cols-[250px_minmax(0,1fr)]">
        <DocumentationSidebar t={t} />
        <DocumentationContent navigation={navigation} supportedAgents={supportedAgents} t={t} />
      </div>
      <footer className="border-t border-zinc-200 bg-zinc-50 py-7 text-zinc-700 dark:border-white/10 dark:bg-[#0f172a] dark:text-zinc-300">
        <div className="mx-auto flex max-w-[1600px] flex-col gap-3 px-5 text-xs sm:flex-row sm:items-center sm:justify-between sm:px-8 lg:pl-[298px] lg:pr-12">
          <span className="flex items-center gap-2 font-bold text-zinc-950 dark:text-white">
            <Boxes aria-hidden="true" className="text-blue-700 dark:text-blue-300" size={16} />
            {t("footer.product")}
          </span>
          <span>{t("footer.note")}</span>
        </div>
      </footer>
    </div>
  );
}
