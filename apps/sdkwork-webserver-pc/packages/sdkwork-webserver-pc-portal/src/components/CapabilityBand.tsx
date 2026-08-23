import { CloudCog, GlobeLock, PackageCheck } from "lucide-react";
import type { PortalMessageKey } from "../i18n/index.ts";
import type { PortalTranslator } from "../services/portal-translator.ts";

const capabilities = [
  { icon: PackageCheck, title: "capabilities.publish.title", description: "capabilities.publish.description" },
  { icon: CloudCog, title: "capabilities.deploy.title", description: "capabilities.deploy.description" },
  { icon: GlobeLock, title: "capabilities.delivery.title", description: "capabilities.delivery.description" },
] as const satisfies readonly {
  icon: typeof PackageCheck;
  title: PortalMessageKey;
  description: PortalMessageKey;
}[];

export function CapabilityBand({ t }: { t: PortalTranslator }) {
  return (
    <section className="scroll-mt-16 bg-[#f8fafc] py-16 text-zinc-950 sm:py-24 [@media(max-height:760px)]:py-14" id="capabilities">
      <div className="mx-auto max-w-[1280px] px-5 sm:px-7 lg:px-10">
        <div className="grid items-end gap-8 lg:grid-cols-[minmax(0,0.9fr)_minmax(420px,1.1fr)] lg:gap-16">
          <div>
            <span className="text-xs font-bold uppercase text-blue-700">{t("capabilities.eyebrow")}</span>
            <h2 className="mt-3 max-w-[620px] text-3xl font-bold leading-tight sm:text-4xl">{t("capabilities.title")}</h2>
          </div>
          <p className="m-0 max-w-[680px] leading-7 text-zinc-600">{t("capabilities.description")}</p>
        </div>
        <div className="mt-14 grid gap-10 md:grid-cols-3 lg:gap-14">
          {capabilities.map(({ description, icon: Icon, title }, index) => (
            <article className="min-w-0 py-2" key={title}>
              <div className="flex items-center justify-between gap-4">
                <span className="grid size-11 place-items-center rounded bg-blue-100 text-blue-800">
                  <Icon aria-hidden="true" size={23} />
                </span>
                <span className="font-mono text-xs font-bold text-zinc-400">0{index + 1}</span>
              </div>
              <h3 className="mt-7 text-xl font-bold">{t(title)}</h3>
              <p className="mt-3 text-sm leading-7 text-zinc-600">{t(description)}</p>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}
