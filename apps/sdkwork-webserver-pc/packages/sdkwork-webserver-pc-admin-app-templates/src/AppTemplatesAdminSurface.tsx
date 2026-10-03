import {
  createDeploymentsAdminClient,
  createDeploymentsAdminRegistry,
} from "@sdkwork/deployments-pc-admin-core";
import { DeploymentsResourceTable, type DeploymentsLocale, type DeploymentsModuleEntry } from "@sdkwork/deployments-pc-commons";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { useMemo } from "react";

/**
 * Bridges the sdkwork-deployments app-template catalog into the backend-admin
 * surface. The menu entries stay in the host; the pages are the shared
 * registry-driven table (`DeploymentsResourceTable` — the exact component the
 * deployments workspace falls back to) driven by the sibling's
 * `createDeploymentsAdminRegistry`, whose transport
 * (`createDeploymentsAdminClient`) is composed inside the sibling's
 * `@sdkwork/deployments-pc-admin-core` from the injected backend API base URL
 * and the shared IAM token manager. No table, data source, or moderation
 * action is re-implemented here.
 *
 * The host's resource keys are kebab-case while the deployments registry and
 * its i18n catalog key the same resources camelCase, so this map is the one
 * place that translates between the two vocabularies — entries carry the
 * deployments-native `resource` id the registry and the shared catalog
 * resolve against, plus the copy the host module declares for its own menus.
 */
export type AppTemplatesAdminResource =
  | "template-categories"
  | "app-templates"
  | "app-template-versions";

const DEPLOYMENTS_ENTRIES: Readonly<
  Record<AppTemplatesAdminResource, DeploymentsModuleEntry>
> = {
  "template-categories": {
    resource: "templateCategories",
    label: "Template Categories",
    description: "Marketplace taxonomy for app templates",
    order: 1,
  },
  "app-templates": {
    resource: "appTemplates",
    label: "App Templates",
    description: "Author-published listings and moderation",
    order: 2,
  },
  "app-template-versions": {
    resource: "appTemplateVersions",
    label: "Template Versions",
    description: "Published version snapshots of one listing",
    order: 3,
  },
};

export interface AppTemplatesAdminSurfaceProps {
  backendApiBaseUrl: string;
  locale: DeploymentsLocale;
  resource: AppTemplatesAdminResource;
  tokenManager: AuthTokenManager;
}

export function AppTemplatesAdminSurface({
  backendApiBaseUrl,
  locale,
  resource,
  tokenManager,
}: AppTemplatesAdminSurfaceProps) {
  const deploymentsResource = DEPLOYMENTS_ENTRIES[resource].resource;
  const registry = useMemo(
    () =>
      createDeploymentsAdminRegistry(
        createDeploymentsAdminClient(backendApiBaseUrl, tokenManager),
      ),
    [backendApiBaseUrl, tokenManager],
  );
  return (
    <div className="deploy-surface">
      <DeploymentsResourceTable
        entry={DEPLOYMENTS_ENTRIES[resource]}
        locale={locale}
        source={registry[deploymentsResource]}
      />
    </div>
  );
}
