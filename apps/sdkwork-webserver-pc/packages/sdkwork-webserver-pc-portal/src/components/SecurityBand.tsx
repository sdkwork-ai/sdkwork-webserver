import { FileCheck2, History, LockKeyhole, ShieldCheck } from "lucide-react";
import type { PortalMessageKey } from "../i18n/index.ts";
import type { PortalTranslator } from "../services/portal-translator.ts";

const controls = [
  { icon: FileCheck2, title: "security.integrity.title", description: "security.integrity.description" },
  { icon: LockKeyhole, title: "security.tls.title", description: "security.tls.description" },
  { icon: ShieldCheck, title: "security.supply.title", description: "security.supply.description" },
  { icon: History, title: "security.audit.title", description: "security.audit.description" },
] as const satisfies readonly {
  icon: typeof FileCheck2;
  title: PortalMessageKey;
  description: PortalMessageKey;
}[];

export function SecurityBand({ t }: { t: PortalTranslator }) {
  return (
    <section className="scroll-mt-16 bg-[#111827] py-16 text-white sm:py-24" id="security">
      <div className="mx-auto max-w-[1280px] px-5 sm:px-7 lg:px-10">
        <div className="grid items-end gap-8 lg:grid-cols-[minmax(0,0.9fr)_minmax(420px,1.1fr)] lg:gap-16">
          <div>
            <span className="text-xs font-bold uppercase text-blue-300">{t("security.eyebrow")}</span>
            <h2 className="mt-3 max-w-[760px] text-3xl font-bold leading-tight sm:text-4xl">{t("security.title")}</h2>
          </div>
          <p className="m-0 max-w-[680px] leading-7 text-zinc-400">{t("security.description")}</p>
        </div>
        <div className="mt-14 grid gap-10 sm:grid-cols-2 lg:grid-cols-4 lg:gap-12">
          {controls.map(({ description, icon: Icon, title }, index) => (
            <article className="min-w-0" key={title}>
              <div className="flex items-center justify-between gap-4">
                <Icon aria-hidden="true" className="text-blue-300" size={24} />
                <span className="font-mono text-xs font-bold text-zinc-600">0{index + 1}</span>
              </div>
              <h3 className="mt-7 text-base font-bold">{t(title)}</h3>
              <p className="mt-3 text-sm leading-6 text-zinc-400">{t(description)}</p>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}
