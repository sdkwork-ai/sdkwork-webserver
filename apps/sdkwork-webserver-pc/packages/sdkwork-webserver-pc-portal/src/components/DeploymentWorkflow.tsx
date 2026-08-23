import { Check, CircleCheck, History, ShieldCheck } from "lucide-react";
import type { PortalMessageKey } from "../i18n/index.ts";
import type { PortalTranslator } from "../services/portal-translator.ts";

const steps = [
  ["workflow.step1.title", "workflow.step1.description"],
  ["workflow.step2.title", "workflow.step2.description"],
  ["workflow.step3.title", "workflow.step3.description"],
  ["workflow.step4.title", "workflow.step4.description"],
] as const satisfies readonly (readonly [PortalMessageKey, PortalMessageKey])[];

const deploymentFacts = [
  ["workflow.panel.artifact", "workflow.panel.artifactValue"],
  ["workflow.panel.integrity", "workflow.panel.integrityValue"],
  ["workflow.panel.ingress", "workflow.panel.ingressValue"],
  ["workflow.panel.rollback", "workflow.panel.rollbackValue"],
] as const satisfies readonly (readonly [PortalMessageKey, PortalMessageKey])[];

export function DeploymentWorkflow({ t }: { t: PortalTranslator }) {
  return (
    <section className="scroll-mt-[56px] bg-white pt-16 text-zinc-950 sm:pt-24" id="workflow">
      <div className="mx-auto max-w-[1280px] px-5 sm:px-7 lg:px-10">
        <div className="grid items-end gap-8 lg:grid-cols-[minmax(0,0.9fr)_minmax(420px,1.1fr)] lg:gap-16">
          <div>
            <span className="text-xs font-bold uppercase text-blue-700">{t("workflow.eyebrow")}</span>
            <h2 className="mt-3 max-w-[700px] text-3xl font-bold leading-tight sm:text-4xl">{t("workflow.title")}</h2>
          </div>
          <p className="m-0 max-w-[680px] leading-7 text-zinc-600">{t("workflow.description")}</p>
        </div>

        <ol className="mt-14 grid gap-8 md:grid-cols-2 lg:grid-cols-4 lg:gap-10">
          {steps.map(([title, description], index) => (
            <li className="min-w-0" key={title}>
              <span className="grid size-8 place-items-center rounded-full bg-blue-100 text-xs font-bold text-blue-800">
                {index + 1}
              </span>
              <h3 className="mt-5 text-base font-bold">{t(title)}</h3>
              <p className="mt-2 text-sm leading-6 text-zinc-600">{t(description)}</p>
            </li>
          ))}
        </ol>
      </div>

      <div className="mt-16 bg-[#0f172a] text-white" aria-label={t("workflow.panel.title")}>
        <div className="mx-auto grid max-w-[1280px] gap-8 px-5 py-10 sm:px-7 lg:grid-cols-[minmax(250px,0.65fr)_minmax(0,1.35fr)] lg:items-center lg:px-10 lg:py-12">
          <div>
            <header className="flex items-center gap-3">
              <div className="flex min-w-0 items-center gap-3 whitespace-nowrap">
                <span className="size-2 rounded-full bg-blue-500 motion-safe:animate-pulse" />
                <strong className="truncate font-mono text-sm">{t("workflow.panel.title")}</strong>
              </div>
              <span className="shrink-0 whitespace-nowrap bg-blue-400/15 px-2 py-1 text-xs font-bold text-blue-200">
                {t("workflow.panel.status")}
              </span>
            </header>
            <p className="mt-5 flex items-center gap-2 text-xs font-semibold text-blue-200">
              <CircleCheck aria-hidden="true" size={17} />
              {t("workflow.panel.audit")}
            </p>
          </div>
          <dl className="m-0 grid gap-x-8 gap-y-7 sm:grid-cols-2">
            {deploymentFacts.map(([label, value], index) => (
              <div className="min-w-0" key={label}>
                <dt className="text-xs font-medium text-zinc-400">{t(label)}</dt>
                <dd className="m-0 mt-2 flex min-w-0 items-center gap-2 text-sm font-semibold text-zinc-100">
                  {index === 1 ? <ShieldCheck aria-hidden="true" className="shrink-0 text-blue-600" size={16} /> : index === 3 ? <History aria-hidden="true" className="shrink-0 text-sky-600" size={16} /> : <Check aria-hidden="true" className="shrink-0 text-blue-600" size={16} />}
                  <span className="min-w-0 break-words">{t(value)}</span>
                </dd>
              </div>
            ))}
          </dl>
        </div>
      </div>
    </section>
  );
}
