import { Bell, Boxes, Menu, SquareTerminal, X } from "lucide-react";
import { useState } from "react";
import type { PortalMessageKey } from "../i18n/index.ts";
import type { PortalTranslator } from "../services/portal-translator.ts";
import type { PortalNavigation, PortalViewer } from "../types.ts";

export function PortalHeader({
  navigation,
  t,
  viewer,
}: {
  navigation: PortalNavigation;
  t: PortalTranslator;
  viewer?: PortalViewer;
}) {
  const [navigationOpen, setNavigationOpen] = useState(false);
  const viewerLabel = viewer?.label?.trim() || t("header.account");
  const viewerInitial = Array.from(viewerLabel)[0]?.toLocaleUpperCase() ?? "U";
  const navigationItems = [
    { href: "/", label: "nav.home" },
    { href: "#skill", label: "nav.skill" },
    { href: "#capabilities", label: "nav.capabilities" },
    { href: "#workflow", label: "nav.workflow" },
    { href: "#security", label: "nav.security" },
    { href: navigation.documentationHref, label: "nav.documentation" },
  ] as const satisfies readonly { href: string; label: PortalMessageKey }[];

  return (
    <header className="sticky top-0 z-50 bg-[#020617]/95 text-white backdrop-blur-xl">
      <div className="grid h-14 w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-2 px-4 sm:px-6 lg:px-8 xl:grid-cols-[minmax(220px,1fr)_auto_minmax(220px,1fr)] 2xl:px-10">
        <a className="flex min-w-0 items-center gap-2 justify-self-start whitespace-nowrap text-inherit no-underline" href="/" aria-label={t("brand.name")}>
          <span className="grid size-8 shrink-0 place-items-center rounded bg-blue-400 text-blue-950">
            <Boxes aria-hidden="true" size={17} strokeWidth={2.2} />
          </span>
          <strong className="hidden min-w-0 truncate text-sm font-bold sm:block">{t("brand.name")}</strong>
        </a>

        <nav className="hidden items-center gap-1 justify-self-center xl:flex" aria-label={t("header.navigation")}>
          {navigationItems.map((item) => (
            <a
              className="flex min-h-8 items-center whitespace-nowrap rounded px-3 text-[13px] font-medium text-zinc-300 no-underline transition-colors hover:bg-white/[0.06] hover:text-white focus-visible:bg-white/[0.06] focus-visible:text-white"
              href={item.href}
              key={item.href}
            >
              {t(item.label)}
            </a>
          ))}
        </nav>

        <div className="flex min-w-0 items-center gap-1.5 justify-self-end whitespace-nowrap">
          <button
            aria-controls="portal-mobile-navigation"
            aria-expanded={navigationOpen}
            aria-label={navigationOpen ? t("header.closeNavigation") : t("header.openNavigation")}
            className="inline-flex size-8 shrink-0 items-center justify-center rounded bg-white/[0.07] text-zinc-200 transition-colors hover:bg-white/[0.12] hover:text-blue-300 xl:hidden"
            onClick={() => setNavigationOpen((current) => !current)}
            title={navigationOpen ? t("header.closeNavigation") : t("header.openNavigation")}
            type="button"
          >
            {navigationOpen ? <X aria-hidden="true" size={18} /> : <Menu aria-hidden="true" size={18} />}
          </button>

          <a
            aria-label={t("header.notifications")}
            className="inline-flex size-8 shrink-0 items-center justify-center rounded text-zinc-300 no-underline transition-colors hover:bg-white/[0.07] hover:text-blue-300"
            href={navigation.notificationsHref}
            title={t("header.notifications")}
          >
            <Bell aria-hidden="true" size={18} />
          </a>

          <a
            aria-label={t("header.console")}
            className="inline-flex size-8 shrink-0 items-center justify-center gap-2 rounded bg-blue-400 text-[13px] font-bold text-blue-950 no-underline transition-colors hover:bg-blue-300 sm:w-auto sm:px-3"
            href={navigation.consoleHref}
          >
            <SquareTerminal aria-hidden="true" size={17} />
            <span className="hidden sm:inline">{t("header.console")}</span>
          </a>

          {viewer ? (
            <>
              <span aria-hidden="true" className="hidden h-5 w-px bg-white/15 sm:block" />
              <a
                aria-label={t("header.accountAria", { user: viewerLabel })}
                className="flex min-w-0 items-center gap-2 whitespace-nowrap text-zinc-200 no-underline transition-colors hover:text-blue-300"
                href={navigation.consoleHref}
                title={t("header.accountAria", { user: viewerLabel })}
              >
                <span aria-hidden="true" className="grid size-8 shrink-0 place-items-center rounded-full bg-white/10 text-xs font-bold text-blue-200">
                  {viewerInitial}
                </span>
                <span className="hidden max-w-[160px] truncate text-sm font-semibold md:block">{viewerLabel}</span>
              </a>
            </>
          ) : null}
        </div>
      </div>

      {navigationOpen ? (
        <nav
          aria-label={t("header.navigation")}
          className="border-t border-white/[0.06] bg-[#0f172a] shadow-xl shadow-black/20 xl:hidden"
          id="portal-mobile-navigation"
        >
          <div className="grid grid-cols-2 gap-x-4 px-4 py-2 sm:grid-cols-3 sm:px-6 lg:grid-cols-6 lg:px-8">
            {navigationItems.map((item) => (
              <a
                className="flex min-h-10 items-center whitespace-nowrap text-sm font-semibold text-zinc-200 no-underline transition-colors hover:text-blue-300"
                href={item.href}
                key={item.href}
                onClick={() => setNavigationOpen(false)}
              >
                {t(item.label)}
              </a>
            ))}
          </div>
        </nav>
      ) : null}
    </header>
  );
}
