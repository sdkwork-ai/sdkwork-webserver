import {
  ArrowRight,
  Bot,
  Box,
  Boxes,
  CheckCircle2,
  CloudCog,
  Code2,
  FileCheck2,
  GlobeLock,
  History,
  PackageCheck,
  Rocket,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import type { DocumentationMessageKey } from "../i18n/index.ts";
import type { DocumentationTranslator } from "../services/documentation-translator.ts";
import type { DocumentationNavigation } from "../types.ts";

const overviewCapabilities = [
  { icon: Boxes, title: "overview.publish.title", description: "overview.publish.description" },
  { icon: CloudCog, title: "overview.delivery.title", description: "overview.delivery.description" },
  { icon: GlobeLock, title: "overview.operations.title", description: "overview.operations.description" },
] as const satisfies readonly DocumentationFeature[];

const applicationCapabilities = [
  { icon: Box, title: "applications.identity.title", description: "applications.identity.description" },
  { icon: PackageCheck, title: "applications.artifact.title", description: "applications.artifact.description" },
  { icon: FileCheck2, title: "applications.release.title", description: "applications.release.description" },
  { icon: History, title: "applications.rollback.title", description: "applications.rollback.description" },
] as const satisfies readonly DocumentationFeature[];

const quickStartSteps = [
  ["quickstart.step1.title", "quickstart.step1.description"],
  ["quickstart.step2.title", "quickstart.step2.description"],
  ["quickstart.step3.title", "quickstart.step3.description"],
  ["quickstart.step4.title", "quickstart.step4.description"],
] as const satisfies readonly DocumentationStep[];

const agentSteps = [
  ["agents.flow1.title", "agents.flow1.description"],
  ["agents.flow2.title", "agents.flow2.description"],
  ["agents.flow3.title", "agents.flow3.description"],
  ["agents.flow4.title", "agents.flow4.description"],
] as const satisfies readonly DocumentationStep[];

const securityCapabilities = [
  { icon: Code2, title: "security.sdk.title", description: "security.sdk.description" },
  { icon: ShieldCheck, title: "security.integrity.title", description: "security.integrity.description" },
  { icon: GlobeLock, title: "security.tls.title", description: "security.tls.description" },
  { icon: History, title: "security.recovery.title", description: "security.recovery.description" },
] as const satisfies readonly DocumentationFeature[];

const questions = [
  ["troubleshooting.auth.question", "troubleshooting.auth.answer"],
  ["troubleshooting.skill.question", "troubleshooting.skill.answer"],
  ["troubleshooting.deploy.question", "troubleshooting.deploy.answer"],
] as const satisfies readonly DocumentationStep[];

type DocumentationFeature = {
  description: DocumentationMessageKey;
  icon: typeof Boxes;
  title: DocumentationMessageKey;
};

type DocumentationStep = readonly [DocumentationMessageKey, DocumentationMessageKey];

export function DocumentationContent({
  navigation,
  supportedAgents,
  t,
}: {
  navigation: DocumentationNavigation;
  supportedAgents: readonly string[];
  t: DocumentationTranslator;
}) {
  return (
    <main className="min-w-0 bg-white text-zinc-950 dark:bg-[#020617] dark:text-white">
      <section className="border-b border-zinc-200 bg-[#0f172a] px-5 py-12 text-white sm:px-8 sm:py-16 lg:px-12 dark:border-white/10">
        <div className="max-w-[940px]">
          <span className="flex items-center gap-2 text-xs font-bold uppercase text-blue-300">
            <Terminal aria-hidden="true" size={16} />
            {t("hero.eyebrow")}
          </span>
          <h1 className="mt-4 max-w-[820px] text-4xl font-bold leading-tight sm:text-5xl">{t("hero.title")}</h1>
          <p className="mt-5 max-w-[820px] text-base leading-8 text-zinc-300 sm:text-lg">{t("hero.description")}</p>
          <dl className="mt-9 grid max-w-[820px] border-y border-white/15 sm:grid-cols-3">
            <DocumentationFact label={t("hero.guideLabel")} value={t("hero.guideValue")} />
            <DocumentationFact bordered label={t("hero.profileLabel")} value={t("hero.profileValue")} />
            <DocumentationFact bordered label={t("hero.agentLabel")} value={t("hero.agentValue", { count: supportedAgents.length })} />
          </dl>
        </div>
      </section>

      <div className="max-w-[1040px] px-5 sm:px-8 lg:px-12">
        <DocumentationSection description={t("overview.description")} eyebrow={t("overview.eyebrow")} id="overview" title={t("overview.title")}>
          <FeatureGrid features={overviewCapabilities} t={t} />
          <div className="mt-10 flex flex-wrap items-center gap-2 border-y border-zinc-200 py-5 text-sm font-semibold text-zinc-700 dark:border-white/10 dark:text-zinc-200">
            {["overview.flow.portal", "overview.flow.console", "overview.flow.sdk", "overview.flow.runtime"].map((key, index) => (
              <span className="contents" key={key}>
                {index > 0 ? <ArrowRight aria-hidden="true" className="text-blue-600" size={17} /> : null}
                <span className="px-2 py-1">{t(key as DocumentationMessageKey)}</span>
              </span>
            ))}
          </div>
        </DocumentationSection>

        <DocumentationSection description={t("quickstart.description")} eyebrow={t("quickstart.eyebrow")} id="quickstart" title={t("quickstart.title")}>
          <StepList steps={quickStartSteps} t={t} />
          <a className="mt-8 inline-flex min-h-11 items-center gap-2 rounded-md bg-blue-700 px-5 text-sm font-bold text-white no-underline hover:bg-blue-800 dark:bg-blue-400 dark:text-blue-950 dark:hover:bg-blue-300" href={navigation.consoleHref}>
            <Rocket aria-hidden="true" size={18} />
            {t("quickstart.openConsole")}
          </a>
        </DocumentationSection>

        <DocumentationSection description={t("applications.description")} eyebrow={t("applications.eyebrow")} id="applications" title={t("applications.title")}>
          <FeatureGrid columns={2} features={applicationCapabilities} t={t} />
        </DocumentationSection>

        <DocumentationSection description={t("deployment.description")} eyebrow={t("deployment.eyebrow")} id="deployment" title={t("deployment.title")}>
          <div className="mt-8 overflow-x-auto border border-zinc-200 dark:border-white/10">
            <table className="min-w-[720px] table-fixed text-left text-sm">
              <thead className="bg-zinc-100 text-zinc-600 dark:bg-white/5 dark:text-zinc-300">
                <tr>
                  <th className="w-[160px] px-5 py-4 font-semibold">{t("deployment.table.profile")}</th>
                  <th className="px-5 py-4 font-semibold">{t("deployment.table.useCase")}</th>
                  <th className="px-5 py-4 font-semibold">{t("deployment.table.operation")}</th>
                </tr>
              </thead>
              <tbody>
                <DeploymentRow operation={t("deployment.cloud.operation")} profile={t("deployment.cloud.profile")} useCase={t("deployment.cloud.useCase")} />
                <DeploymentRow operation={t("deployment.standalone.operation")} profile={t("deployment.standalone.profile")} useCase={t("deployment.standalone.useCase")} />
              </tbody>
            </table>
          </div>
          <p className="mt-6 border-l-2 border-amber-500 bg-amber-50 px-4 py-3 text-sm leading-6 text-amber-950 dark:bg-amber-400/10 dark:text-amber-100">{t("deployment.note")}</p>
        </DocumentationSection>

        <DocumentationSection description={t("agents.description")} eyebrow={t("agents.eyebrow")} id="agents" title={t("agents.title")}>
          <div className="mt-8 border-y border-zinc-200 py-5 dark:border-white/10">
            <span className="text-xs font-bold uppercase text-zinc-500 dark:text-zinc-400">{t("agents.supported")}</span>
            <ul className="mt-4 grid gap-x-6 gap-y-3 p-0 sm:grid-cols-2 lg:grid-cols-3">
              {supportedAgents.map((agent) => (
                <li className="flex min-w-0 items-center gap-2 text-sm font-semibold text-zinc-800 dark:text-zinc-200" key={agent}>
                  <Bot aria-hidden="true" className="shrink-0 text-blue-600 dark:text-blue-300" size={17} />
                  <span className="break-words">{agent}</span>
                </li>
              ))}
            </ul>
          </div>
          <StepList steps={agentSteps} t={t} />
          <a className="mt-8 inline-flex min-h-11 items-center gap-2 rounded-md border border-zinc-300 px-5 text-sm font-bold text-zinc-900 no-underline hover:border-blue-600 hover:bg-blue-50 hover:text-blue-800 dark:border-white/15 dark:text-white dark:hover:border-blue-400 dark:hover:bg-blue-400/10 dark:hover:text-blue-200" href={`${navigation.portalHref}#skill`}>
            <Bot aria-hidden="true" size={18} />
            {t("agents.openPortal")}
          </a>
        </DocumentationSection>

        <DocumentationSection description={t("security.description")} eyebrow={t("security.eyebrow")} id="security" title={t("security.title")}>
          <FeatureGrid columns={2} features={securityCapabilities} t={t} />
        </DocumentationSection>

        <DocumentationSection eyebrow={t("troubleshooting.eyebrow")} id="troubleshooting" title={t("troubleshooting.title")}>
          <div className="mt-8 border-t border-zinc-200 dark:border-white/10">
            {questions.map(([question, answer]) => (
              <article className="grid gap-3 border-b border-zinc-200 py-6 md:grid-cols-[minmax(220px,0.72fr)_minmax(0,1.28fr)] dark:border-white/10" key={question}>
                <h3 className="text-base font-bold">{t(question)}</h3>
                <p className="m-0 text-sm leading-7 text-zinc-600 dark:text-zinc-300">{t(answer)}</p>
              </article>
            ))}
          </div>
        </DocumentationSection>
      </div>
    </main>
  );
}

function DocumentationSection({
  children,
  description,
  eyebrow,
  id,
  title,
}: {
  children: React.ReactNode;
  description?: string;
  eyebrow: string;
  id: string;
  title: string;
}) {
  return (
    <section className="scroll-mt-28 border-b border-zinc-200 py-14 sm:py-16 dark:border-white/10" id={id}>
      <span className="text-xs font-bold uppercase text-blue-700 dark:text-blue-300">{eyebrow}</span>
      <h2 className="mt-3 max-w-[820px] text-3xl font-bold leading-tight">{title}</h2>
      {description ? <p className="mt-4 max-w-[850px] leading-7 text-zinc-600 dark:text-zinc-300">{description}</p> : null}
      {children}
    </section>
  );
}

function DocumentationFact({ bordered = false, label, value }: { bordered?: boolean; label: string; value: string }) {
  return (
    <div className={`py-4 ${bordered ? "border-t border-white/15 sm:border-l sm:border-t-0 sm:pl-5" : ""}`}>
      <dt className="text-xs text-zinc-400">{label}</dt>
      <dd className="mt-1 text-sm font-bold text-white">{value}</dd>
    </div>
  );
}

function FeatureGrid({ columns = 3, features, t }: { columns?: 2 | 3; features: readonly DocumentationFeature[]; t: DocumentationTranslator }) {
  return (
    <div className={`mt-9 grid border-y border-zinc-200 dark:border-white/10 ${columns === 3 ? "md:grid-cols-3" : "sm:grid-cols-2"}`}>
      {features.map(({ description, icon: Icon, title }, index) => (
        <article className={`py-7 ${index > 0 ? "border-t border-zinc-200 dark:border-white/10" : ""} ${columns === 3 ? "md:border-l md:border-t-0 md:px-7 md:first:border-l-0 md:first:pl-0" : "sm:border-l sm:px-7 sm:even:border-l sm:odd:border-l-0 sm:[&:nth-child(2)]:border-t-0"}`} key={title}>
          <Icon aria-hidden="true" className="text-blue-700 dark:text-blue-300" size={22} />
          <h3 className="mt-4 text-base font-bold">{t(title)}</h3>
          <p className="mt-2 text-sm leading-6 text-zinc-600 dark:text-zinc-400">{t(description)}</p>
        </article>
      ))}
    </div>
  );
}

function StepList({ steps, t }: { steps: readonly DocumentationStep[]; t: DocumentationTranslator }) {
  return (
    <ol className="mt-9 grid gap-0 border-y border-zinc-200 p-0 sm:grid-cols-2 dark:border-white/10">
      {steps.map(([title, description], index) => (
        <li className={`grid grid-cols-[32px_minmax(0,1fr)] gap-3 py-6 ${index > 0 ? "border-t border-zinc-200 dark:border-white/10" : ""} sm:px-6 sm:first:pl-0 sm:[&:nth-child(2)]:border-t-0 sm:[&:nth-child(even)]:border-l`} key={title}>
          <span className="grid size-7 place-items-center rounded-full bg-blue-100 text-xs font-bold text-blue-800 dark:bg-blue-400/15 dark:text-blue-200">{index + 1}</span>
          <span>
            <strong className="block text-sm">{t(title)}</strong>
            <span className="mt-2 block text-sm leading-6 text-zinc-600 dark:text-zinc-400">{t(description)}</span>
          </span>
        </li>
      ))}
    </ol>
  );
}

function DeploymentRow({ operation, profile, useCase }: { operation: string; profile: string; useCase: string }) {
  return (
    <tr className="border-t border-zinc-200 dark:border-white/10">
      <td className="px-5 py-5 align-top font-bold text-blue-800 dark:text-blue-200">{profile}</td>
      <td className="whitespace-normal px-5 py-5 align-top leading-6 text-zinc-700 dark:text-zinc-300">{useCase}</td>
      <td className="whitespace-normal px-5 py-5 align-top leading-6 text-zinc-700 dark:text-zinc-300">{operation}</td>
    </tr>
  );
}
