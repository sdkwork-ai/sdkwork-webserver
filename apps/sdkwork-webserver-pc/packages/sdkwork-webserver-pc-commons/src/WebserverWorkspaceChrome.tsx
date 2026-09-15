import {
  Activity,
  AppWindow,
  Bell,
  Boxes,
  CloudCog,
  FolderOpen,
  Globe2,
  HardDrive,
  House,
  Layers3,
  LogOut,
  Puzzle,
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

/**
 * A resolved module tab. The workspace owns resolution (visible entries,
 * permission filtering, landing path) so the chrome stays presentational.
 */
export interface WorkspaceModuleTab {
  id: string;
  /** Landing route for the tab — the first entry the operator can open. */
  href: string;
  label: string;
  /** Tooltip text; falls back to the label when absent. */
  description?: string;
}

interface WorkspaceHeaderProps {
  adminRole?: string;
  basePath: string;
  /** Admin modules rendered as tabs. Omitted or single-entry → no tab bar. */
  moduleTabs?: readonly WorkspaceModuleTab[];
  notificationsHref?: string;
  onSignOut?(): void;
  portalHref?: string;
  surface: WorkspaceSurface;
  t: WorkspaceTranslator;
  userLabel?: string;
}

interface WorkspaceSidebarProps {
  basePath: string;
  entries: readonly { label?: string; path?: string; resource: WebserverResourceKey }[];
  surface: WorkspaceSurface;
  t: WorkspaceTranslator;
}

export function WorkspaceHeader({
  adminRole,
  basePath,
  moduleTabs,
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
  const hasModuleTabs = (moduleTabs?.length ?? 0) > 0;

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

      <div className={`workspace-header-actions${hasModuleTabs ? " has-modules" : ""}`}>
        {hasModuleTabs ? (
          <nav aria-label={t("nav.modules")} className="workspace-header-nav">
            {moduleTabs?.map((tab) => (
              <NavLink
                className={({ isActive }) => `workspace-module-tab${isActive ? " is-active" : ""}`}
                key={tab.id}
                title={tab.description ?? tab.label}
                to={tab.href}
              >
                <ModuleIcon moduleId={tab.id} />
                <span>{tab.label}</span>
              </NavLink>
            ))}
          </nav>
        ) : null}
        <div className="workspace-header-account-group">
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
      </div>
    </header>
  );
}

export function WorkspaceSidebar({ basePath, entries, surface, t }: WorkspaceSidebarProps) {
  return (
    <aside className="sidebar">
      <span className="sidebar-label">{t("nav.workspace")}</span>
      <nav aria-label={t("nav.primary")}>
        {entries.map((entry) => {
          const label = resourceText(t, entry.resource, entry.label, surface);
          const segment = entry.path?.trim() || entry.resource;
          return (
            <NavLink
              aria-label={label}
              key={entry.resource}
              title={label}
              to={`${basePath}/${segment}`}
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

/** Module tab icon, keyed by `WebserverAdminModuleId` (unknown ids fall back). */
function ModuleIcon({ moduleId }: { moduleId: string }): ReactNode {
  const iconProps = { "aria-hidden": true, size: 16 } as const;
  switch (moduleId) {
    case "storageCenter":
      return <CloudCog {...iconProps} />;
    default:
      return <House {...iconProps} />;
  }
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
    case "plugins":
      return <Puzzle {...iconProps} />;
    case "skills":
      return <Sparkles {...iconProps} />;
    case "mcp":
      return <Plug {...iconProps} />;
    case "nginx":
      return <ServerCog {...iconProps} />;
    case "servers":
      return <Server {...iconProps} />;
    case "servers-explorer":
      return <FolderOpen {...iconProps} />;
    case "audit":
      return <ScrollText {...iconProps} />;
    case "diagnostics":
      return <Activity {...iconProps} />;
    case "storage-providers":
      return <HardDrive {...iconProps} />;
    case "storage-kinds":
      return <CloudCog {...iconProps} />;
    case "storage-buckets":
      return <Boxes {...iconProps} />;
    case "storage-bindings":
      return <Plug {...iconProps} />;
    default:
      return <Activity {...iconProps} />;
  }
}

function resourceText(
  t: WorkspaceTranslator,
  resource: WebserverResourceKey,
  fallbackLabel?: string,
  surface: WorkspaceSurface = "app-console",
): string {
  const candidates: WebserverMessageKey[] =
    surface === "backend-admin"
      ? [
          `resource.${resource}.admin.label` as WebserverMessageKey,
          `resource.${resource}.label` as WebserverMessageKey,
        ]
      : [`resource.${resource}.label` as WebserverMessageKey];
  for (const key of candidates) {
    const translated = t(key);
    if (translated && translated !== key) {
      return translated;
    }
  }
  const fallback = fallbackLabel?.trim();
  // Prefer i18n; fall back to module entry label when a key is missing so new
  // console modules (skills/mcp) never render as blank sidebar text.
  if (fallback) {
    return fallback;
  }
  return resource;
}
