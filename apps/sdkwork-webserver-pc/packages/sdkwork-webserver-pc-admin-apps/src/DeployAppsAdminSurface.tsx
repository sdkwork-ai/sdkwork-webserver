import { createDeploymentsConsoleClients } from "@sdkwork/deployments-pc-console-core";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import { PublishingAppsPage } from "@sdkwork/deployments-pc-console-publishing";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { useMemo } from "react";

/**
 * Backend-admin face of the SDKWork Deployments publishing surface. It is the
 * same canonical `deploy_app` page the app-console mounts; only the menu entry
 * and the surface it lives on differ, so this adapter stays as thin as its
 * console sibling: base URLs plus token manager in, `PublishingAppsPage` out.
 *
 * The page, its service, its dialogs, and its message catalog are all shared
 * from `@sdkwork/deployments-pc-console-publishing` — nothing about the
 * application lifecycle is re-implemented on this surface.
 */
export interface DeployAppsAdminSurfaceProps {
  deployBaseUrl: string;
  driveBaseUrl: string;
  locale: DeploymentsLocale;
  tokenManager: AuthTokenManager;
}

export function DeployAppsAdminSurface({
  deployBaseUrl,
  driveBaseUrl,
  locale,
  tokenManager,
}: DeployAppsAdminSurfaceProps) {
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
