import { Check, Cloud, Code2, Globe2, ServerCog } from "lucide-react";
import type { PortalTranslator } from "../services/portal-translator.ts";

export function CloudTopologyScene({ t }: { t: PortalTranslator }) {
  return (
    <div className="bg-[#0f172a]" aria-label={t("scene.ariaLabel")}>
      <div className="grid md:grid-cols-3 xl:grid-cols-[repeat(3,minmax(0,1fr))_minmax(280px,0.95fr)]">
        <SceneNode icon={Code2} label={t("scene.source")} status={t("scene.sourceStatus")} />
        <SceneNode icon={ServerCog} label={t("scene.orchestration")} status={t("scene.orchestrationStatus")} emphasized />
        <SceneNode icon={Globe2} label={t("scene.edge")} status={t("scene.edgeStatus")} />
        <div className="flex min-h-24 items-center justify-between gap-3 bg-black/10 px-5 py-5 md:col-span-3 xl:col-span-1">
          <RegionStatus icon={Cloud} label={t("scene.regionPrimary")} />
          <span className="h-px min-w-6 flex-1 bg-blue-300/30" />
          <RegionStatus icon={ServerCog} label={t("scene.regionEdge")} />
        </div>
      </div>
    </div>
  );
}

function SceneNode({
  emphasized = false,
  icon: Icon,
  label,
  status,
}: {
  emphasized?: boolean;
  icon: typeof Code2;
  label: string;
  status: string;
}) {
  return (
    <div className={`flex min-h-24 items-center gap-4 px-5 py-5 ${emphasized ? "bg-white/[0.035]" : "bg-transparent"}`}>
      <span className="grid size-10 shrink-0 place-items-center rounded-md bg-white/[0.07] text-blue-200">
        <Icon size={18} />
      </span>
      <span className="min-w-0">
        <strong className="block truncate text-sm text-white">{label}</strong>
        <span className="mt-1 flex items-center gap-1.5 text-xs text-blue-200">
          <Check aria-hidden="true" size={13} />
          {status}
        </span>
      </span>
    </div>
  );
}

function RegionStatus({ icon: Icon, label }: { icon: typeof Cloud; label: string }) {
  return (
    <span className="flex min-w-0 items-center gap-2 text-xs font-medium text-zinc-200">
      <Icon aria-hidden="true" className="shrink-0 text-sky-300" size={15} />
      {label}
    </span>
  );
}
