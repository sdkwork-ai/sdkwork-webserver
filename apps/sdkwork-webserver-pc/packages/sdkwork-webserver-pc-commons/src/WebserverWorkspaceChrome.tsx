import {
  Activity,
  AppWindow,
  Bell,
  Boxes,
  Globe2,
  House,
  Layers3,
  LogOut,
  Plug,
  Rocket,
  ScrollText,
  Server,
  ServerCog,
  Settings2,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import type { ReactNode } from "react";
import { NavLink } from "react-router-dom";

import type { WebserverMessageKey } from "./i18n/index.ts";
import type { WebserverResourceKey } from "./types.ts";

type WorkspaceSurface = "app-console" | "backend-admin";
type WorkspaceTranslator = (key: WebserverMessageKey, values?: Record<string, string | number>) => string;

interface WorkspaceHeaderProps {
  adminRole?: string;
  basePath: string;
  notificationsHref?: string;
  onSignOut?(): void;
  portalHref?: string;
  surface: WorkspaceSurface;
  t: WorkspaceTranslator;
  userLabel?: string;
}

interface WorkspaceSidebarProps {
  basePath: string;
  entries: readonly { label?: string; resource: WebserverResourceKey }[];
  t: WorkspaceTranslator;
}

export function WorkspaceHeader({
  adminRole,
  basePath,
  notificationsHref,
  onSignOut,
  portalHref,
  surface,
  t,
  userLabel,
}: WorkspaceHeaderProps) {
  const accountLabel = userLabel?.trim() || t("auth.user");
  const accountInitial = Array.from(accountLabel)[0]?.toLocaleUpperCase() ?? "U";
  const brandHref = portalHref ?? basePath;

  return (
    <header className="workspace-header">
      <a
        aria-label={`${t("brand.name")} ${t(`surface.${surface}`)}`}
        className="workspace-brand"
        href={brandHref}
      >
        <span className="workspace-brand-mark"><Boxes aria-hidden="true" size={19} strokeWidth={2.2} /></span>
        <span className="workspace-brand-copy">
          <strong>{t("brand.name")}</strong>
          <small>{t(`surface.${surface}`)}</small>
        </span>
      </a>

      <div className="workspace-header-actions">
        {portalHref ? (
          <a className="workspace-header-command" href={portalHref}>
            <House aria-hidden="true" size={17} />
            <span>{t("navigation.portal")}</span>
          </a>
        ) : null}
        {notificationsHref ? (
          <a
            aria-label={t("navigation.notifications")}
            className="workspace-header-icon"
            href={notificationsHref}
            title={t("navigation.notifications")}
          >
            <Bell aria-hidden="true" size={18} />
          </a>
        ) : null}
        <span aria-hidden="true" className="workspace-header-divider" />
        <div className="workspace-account" title={t("auth.account", { user: accountLabel })}>
          <span aria-hidden="true" className="workspace-account-avatar">{accountInitial}</span>
          <span className="workspace-account-copy">
            <strong>{accountLabel}</strong>
            <small>{adminRole ?? t(`surface.${surface}`)}</small>
          </span>
        </div>
        {onSignOut ? (
          <button
            aria-label={t("auth.signOut")}
            className="workspace-header-icon"
            onClick={onSignOut}
            title={t("auth.signOut")}
            type="button"
          >
            <LogOut aria-hidden="true" size={17} />
          </button>
        ) : null}
      </div>
    </header>
  );
}

export function WorkspaceSidebar({ basePath, entries, t }: WorkspaceSidebarProps) {
  return (
    <aside className="sidebar">
      <span className="sidebar-label">{t("nav.workspace")}</span>
      <nav aria-label={t("nav.primary")}>
        {entries.map((entry) => {
          const label = resourceText(t, entry.resource, entry.label);
          return (
            <NavLink
              aria-label={label}
              key={entry.resource}
              title={label}
              to={`${basePath}/${entry.resource}`}
            >
              <ResourceIcon resource={entry.resource} />
              <span>{label}</span>
            </NavLink>
          );
        })}
      </nav>
    </aside>
  );
}

function ResourceIcon({ resource }: { resource: WebserverResourceKey }): ReactNode {
  const iconProps = { "aria-hidden": true, size: 17 } as const;
  switch (resource) {
    case "applications":
      return <AppWindow {...iconProps} />;
    case "sites":
      return <Globe2 {...iconProps} />;
    case "configuration":
      return <Settings2 {...iconProps} />;
    case "domains":
      return <Globe2 {...iconProps} />;
    case "certificates":
      return <ShieldCheck {...iconProps} />;
    case "deployments":
    case "application-deployments":
      return <Rocket {...iconProps} />;
    case "source-versions":
    case "application-source-versions":
      return <Layers3 {...iconProps} />;
    case "skills":
      return <Sparkles {...iconProps} />;
    case "mcp":
      return <Plug {...iconProps} />;
    case "nginx":
      return <ServerCog {...iconProps} />;
    case "servers":
      return <Server {...iconProps} />;
    case "audit":
      return <ScrollText {...iconProps} />;
    case "diagnostics":
      return <Activity {...iconProps} />;
    default:
      return <Activity {...iconProps} />;
  }
}

function resourceText(
  t: WorkspaceTranslator,
  resource: WebserverResourceKey,
  fallbackLabel?: string,
): string {
  const key = `resource.${resource}.label` as WebserverMessageKey;
  const translated = t(key);
  const fallback = fallbackLabel?.trim();
  // Prefer i18n; fall back to module entry label when a key is missing so new
  // console modules (skills/mcp) never render as blank sidebar text.
  if (translated && translated !== key) {
    return translated;
  }
  if (fallback) {
    return fallback;
  }
  return resource;
}
