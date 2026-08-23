import { Bot, Cloud, PackageCheck, ShieldCheck } from "lucide-react";
import { usePortalStatistics } from "../hooks/use-portal-statistics.ts";
import type { PortalTranslator } from "../services/portal-translator.ts";
import type { PortalStatisticsPort } from "../types.ts";

export function PortalStatistics({
  agentCount,
  statistics,
  t,
}: {
  agentCount: number;
  statistics?: PortalStatisticsPort;
  t: PortalTranslator;
}) {
  const state = usePortalStatistics(statistics);
  const deployedApplications = state.status === "ready"
    ? state.snapshot.deployedApplications
    : state.status === "loading"
      ? t("metrics.loading")
      : state.status === "error"
        ? t("metrics.unavailable")
        : t("metrics.signIn");
  const metrics = [
    {
      description: t("metrics.applications.description"),
      icon: Cloud,
      label: t("metrics.applications.label"),
      value: deployedApplications,
    },
    {
      description: t("metrics.agents.description"),
      icon: Bot,
      label: t("metrics.agents.label"),
      value: String(agentCount),
    },
    {
      description: t("metrics.profiles.description"),
      icon: PackageCheck,
      label: t("metrics.profiles.label"),
      value: "2",
    },
    {
      description: t("metrics.controls.description"),
      icon: ShieldCheck,
      label: t("metrics.controls.label"),
      value: "4",
    },
  ] as const;

  return (
    <div className="mt-16 grid w-full grid-cols-2 bg-white/[0.035] px-2 sm:px-4 lg:grid-cols-4" aria-label={t("metrics.ariaLabel")}>
      {metrics.map(({ description, icon: Icon, label, value }, index) => (
        <div
          className={`min-w-0 px-3 py-5 sm:px-4 lg:px-5 ${index >= 2 ? "bg-black/[0.06] lg:bg-transparent" : ""}`}
          key={label}
        >
          <span className="flex items-center gap-2 text-xs font-semibold text-blue-200">
            <Icon aria-hidden="true" size={15} />
            {label}
          </span>
          <strong className="mt-2 block min-h-8 break-words text-xl font-bold text-white sm:text-2xl">{value}</strong>
          <span className="mt-1 block text-[11px] leading-4 text-zinc-400">{description}</span>
        </div>
      ))}
    </div>
  );
}
