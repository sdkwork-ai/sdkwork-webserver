import { Bot, Check, Clipboard, ShieldCheck, Terminal, TriangleAlert } from "lucide-react";
import { portalAgentCount } from "../data/portal-agent-catalog.ts";
import { useSkillInstruction } from "../hooks/use-skill-instruction.ts";
import { createPortalAgentInstruction } from "../services/portal-agent-instruction.ts";
import type { PortalTranslator } from "../services/portal-translator.ts";
import type { PortalClipboardPort } from "../types.ts";

export function SkillIntegrationPanel({
  clipboard,
  t,
}: {
  clipboard: PortalClipboardPort;
  t: PortalTranslator;
}) {
  const instruction = createPortalAgentInstruction(t);
  const { copyInstruction, copyState } = useSkillInstruction(clipboard, instruction);
  const copyLabel = copyState === "copied"
    ? t("skill.copied")
    : copyState === "copying"
      ? t("skill.copying")
      : t("skill.copy");

  return (
    <aside
      aria-labelledby="skill-panel-title"
      className="scroll-mt-[64px] overflow-hidden rounded bg-[#0f172a]"
      id="skill"
    >
      <header className="flex min-h-11 items-center justify-between gap-4 bg-white/[0.045] px-5">
        <span className="flex min-w-0 items-center gap-2 text-xs font-bold uppercase text-blue-300">
          <Bot aria-hidden="true" size={16} />
          <span className="truncate">{t("skill.eyebrow")}</span>
        </span>
        <span className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-[11px] text-blue-200">
          <ShieldCheck aria-hidden="true" size={14} />
          {t("skill.governed")}
        </span>
      </header>

      <div className="p-5 sm:p-6">
        <h2 className="m-0 text-[22px] font-bold leading-tight text-white" id="skill-panel-title">
          {t("skill.title")}
        </h2>
        <p className="mt-3 max-w-[520px] text-sm leading-6 text-blue-50/70">{t("skill.description")}</p>
        <span className="mt-3 inline-flex items-baseline gap-1.5 text-xs text-blue-100/60">
          <strong className="text-base text-white">{portalAgentCount}</strong>
          {t("skill.agentCountShort")}
        </span>

        <div className="mt-5 overflow-hidden rounded bg-[#020617]">
          <div className="flex min-h-9 items-center gap-2 bg-white/[0.035] px-4 text-xs font-semibold text-blue-100/60">
            <Terminal aria-hidden="true" size={14} />
            {t("skill.commandLabel")}
          </div>
          <pre className="m-0 min-h-28 whitespace-pre-wrap break-words p-4 font-mono text-[13px] leading-6 text-blue-100">{instruction}</pre>
        </div>
      </div>

      <footer className="flex flex-col gap-3 bg-black/15 px-5 py-4 sm:px-6">
        <span
          className={`flex min-h-5 items-center gap-2 text-xs ${copyState === "error" ? "text-amber-200" : "text-blue-200"}`}
          role={copyState === "error" ? "alert" : "status"}
        >
          {copyState === "error" ? <TriangleAlert aria-hidden="true" size={15} /> : copyState === "copied" ? <Check aria-hidden="true" size={15} /> : null}
          {copyState === "error" ? t("skill.copyError") : copyState === "copied" ? t("skill.copied") : t("skill.ready")}
        </span>
        <button
          className="inline-flex min-h-11 w-full items-center justify-center gap-2 rounded bg-blue-400 px-4 text-sm font-bold text-blue-950 transition-colors hover:bg-blue-300 disabled:cursor-wait disabled:opacity-70"
          disabled={copyState === "copying"}
          onClick={() => void copyInstruction()}
          type="button"
        >
          {copyState === "copied" ? <Check aria-hidden="true" size={17} /> : <Clipboard aria-hidden="true" size={17} />}
          {copyLabel}
        </button>
      </footer>
    </aside>
  );
}
