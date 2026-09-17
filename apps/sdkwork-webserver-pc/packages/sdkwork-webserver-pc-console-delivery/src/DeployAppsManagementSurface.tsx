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
 * generated clients as props), so this adapter owns exactly one thing: turning
 * the host's base URLs plus token manager into those clients. Styles are scoped
 * by `.deploy-surface`, the shared scope for every deployments surface bridged
 * into this host.
 */
export interface DeployAppsManagementSurfaceProps {
  deployBaseUrl: string;
  driveBaseUrl: string;
  locale: DeploymentsLocale;
  tokenManager: AuthTokenManager;
}

export function DeployAppsManagementSurface({
  deployBaseUrl,
  driveBaseUrl,
  locale,
  tokenManager,
}: DeployAppsManagementSurfaceProps) {
  const clients = useMemo(
    () => createDeploymentsConsoleClients({ deployBaseUrl, driveBaseUrl, tokenManager }),
    [deployBaseUrl, driveBaseUrl, tokenManager],
  );
  return (
    <div className="deploy-surface">
      <PublishingAppsPage deployClient={clients.deploy} driveClient={clients.drive} locale={locale} />
    </div>
  );
}
