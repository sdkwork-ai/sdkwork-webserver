import { createDeploymentsConsoleClients } from "@sdkwork/deployments-pc-console-core";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { PublishingAppsPage } from "@sdkwork/deployments-pc-console-publishing";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { useMemo } from "react";

/**
 * Bridges the SDKWork Deployments publishing surface into the Web Server
 * console. The Applications menu entry stays in the host while the page
 * itself is the canonical sdkwork-deployments implementation over the
 * `deploy_app` entity, sharing the same IAM dual-token session through the
 * injected token manager.
 *
 * The page is host-agnostic by contract (`PublishingAppsPage` takes its two
 * generated clients as props), so this adapter owns exactly two things: turning
 * the host's base URLs plus token manager into those clients, and forwarding the
 * `surface` marker. Styles are scoped by `.deploy-surface`, the shared scope for
 * every deployments surface bridged into this host.
 *
 * `surface` is forwarded verbatim and nothing here branches on it — the page owns
 * what the marker changes (the ownership facet: the admin face reaches several
 * ownership levels and gets a tab row for it, the console reaches one and gets no
 * control). It defaults to `console`, the narrower surface, so the console mount
 * passes nothing and only `DeployAppsAdminSurface` has to say what it is.
 */
export interface DeployAppsManagementSurfaceProps {
  deployBaseUrl: string;
  driveBaseUrl: string;
  locale: DeploymentsLocale;
  tokenManager: AuthTokenManager;
  /** Which surface is rendering the page. Defaults to `"console"`. */
  surface?: "admin" | "console";
}

export function DeployAppsManagementSurface({
  deployBaseUrl,
  driveBaseUrl,
  locale,
  tokenManager,
  surface = "console",
}: DeployAppsManagementSurfaceProps) {
  const clients = useMemo(
    () => createDeploymentsConsoleClients({ deployBaseUrl, driveBaseUrl, tokenManager }),
    [deployBaseUrl, driveBaseUrl, tokenManager],
  );
  return (
    <div className="deploy-surface">
      <PublishingAppsPage
        deployClient={clients.deploy}
        driveClient={clients.drive}
        locale={locale}
        surface={surface}
      />
    </div>
  );
}
