import { createDeploymentsConsoleClients } from "@sdkwork/deployments-pc-console-core";
import { MarketplacePage, MyTemplatesPage } from "@sdkwork/deployments-pc-console-marketplace";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { useMemo } from "react";

/**
 * Bridges the sdkwork-deployments app-template marketplace into the Web Server
 * console. The menu entries stay in the host while the pages are the canonical
 * sdkwork-deployments implementations over `deploy_app_template`, sharing the
 * same IAM dual-token session through the injected token manager.
 *
 * Two clients arrive as props to the pages, built together by the sibling's
 * own `createDeploymentsConsoleClients` factory — the deploy app client owns
 * the catalog and the order app client owns trade: acquiring a listing is an
 * `app_template_orders` command on the platform order center (a FREE listing
 * settles inside order creation; a PAID one hands back the cashier the order
 * center returned), so this bridge composes exactly what the pages require
 * and nothing more.
 *
 * The two resources are one module with two pages rather than one page with a
 * mode, so `resource` selects which canonical page mounts and nothing here
 * branches beyond that selection. Styles are scoped by `.deploy-surface`, the
 * shared scope for every deployments surface bridged into this host.
 */
export type MarketplaceConsoleResource = "marketplace" | "my-templates";

export interface MarketplaceConsoleSurfaceProps {
  deployBaseUrl: string;
  driveBaseUrl: string;
  locale: DeploymentsLocale;
  resource: MarketplaceConsoleResource;
  tokenManager: AuthTokenManager;
}

export function MarketplaceConsoleSurface({
  deployBaseUrl,
  driveBaseUrl,
  locale,
  resource,
  tokenManager,
}: MarketplaceConsoleSurfaceProps) {
  const clients = useMemo(
    () => createDeploymentsConsoleClients({ deployBaseUrl, driveBaseUrl, tokenManager }),
    [deployBaseUrl, driveBaseUrl, tokenManager],
  );
  return (
    <div className="deploy-surface">
      {resource === "marketplace" ? (
        <MarketplacePage deployClient={clients.deploy} locale={locale} orderClient={clients.order} />
      ) : (
        <MyTemplatesPage deployClient={clients.deploy} locale={locale} />
      )}
    </div>
  );
}
