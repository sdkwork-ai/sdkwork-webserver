import { CertificateManagementPage, DomainManagementPage } from "@sdkwork/deployments-pc-console-delivery/management";
import { createDeploymentsConsoleClients, DeploymentsConsoleProvider } from "@sdkwork/deployments-pc-console-core";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { useMemo } from "react";

/**
 * Bridges the SDKWork Deployments domain management surface into the Web
 * Server console. The menu entries (Domains / Certificates) stay in place
 * while the pages themselves are the canonical sdkwork-deployments
 * implementation, sharing the same IAM dual-token session through the
 * injected token manager. Styles are scoped by `.deploy-domains-surface`.
 */
export interface DeployDomainManagementSurfaceProps {
  deployBaseUrl: string;
  driveBaseUrl: string;
  locale: DeploymentsLocale;
  resource: "certificates" | "domains";
  tokenManager: AuthTokenManager;
}

export function DeployDomainManagementSurface({
  deployBaseUrl,
  driveBaseUrl,
  locale,
  resource,
  tokenManager,
}: DeployDomainManagementSurfaceProps) {
  const clients = useMemo(
    () => createDeploymentsConsoleClients({ deployBaseUrl, driveBaseUrl, tokenManager }),
    [deployBaseUrl, driveBaseUrl, tokenManager],
  );
  const Page = resource === "domains" ? DomainManagementPage : CertificateManagementPage;
  return (
    <div className="deploy-surface">
      <DeploymentsConsoleProvider clients={clients}>
        <Page locale={locale} />
      </DeploymentsConsoleProvider>
    </div>
  );
}
