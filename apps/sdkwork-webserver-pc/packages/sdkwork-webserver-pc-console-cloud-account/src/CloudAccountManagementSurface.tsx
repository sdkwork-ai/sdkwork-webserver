import { resolveIamCloudAccountManageableScopeLevels } from "@sdkwork/iam-contracts";
import {
  createSdkworkIamConsoleCloudAccountController,
  SdkworkIamConsoleCloudAccountWorkspace,
} from "@sdkwork/iam-pc-console-cloud-account";
import { useWebserverConsoleSdk } from "@sdkwork/webserver-pc-console-core";
import { useMemo } from "react";

/**
 * Props of the cloud account page.
 *
 * Only three session facts are taken: the IAM transport is not this package's
 * business. A capability package consumes SDK and service ports through a core
 * package's public exports and never constructs a generated client (`verify-repo`
 * rejects a capability package whose source imports `@sdkwork/iam-app-sdk` or
 * `@sdkwork/iam-backend-sdk`), so the already-composed `SdkworkIamService` facade
 * is read off the console SDK provider instead.
 *
 * `tenantId` is read because the admin level projection needs it, not because the
 * page enforces anything with it: the `platform` level is reserved for members of
 * the platform tenant, and a caller's tenant is the only session fact that decides
 * it.
 */
export interface CloudAccountManagementSurfaceProps {
  /** Session permission codes, used to decide whether the shared levels may be offered. */
  permissionScope: readonly string[];
  /** Session tenant, used to decide whether the global `platform` level may be offered. */
  tenantId?: string;
  /**
   * Which surface is rendering the page. It selects descriptive copy **and** the
   * width of the offered ownership levels; the default is the tenant console, so a
   * host that is not the platform admin does not have to pass anything.
   */
  surface?: "admin" | "console";
}

/**
 * The cloud account center page.
 *
 * Cloud accounts are an IAM-owned resource served by the IAM backend API. This
 * host contributes the menu entry, the resource key, and the wiring: the page,
 * the controller, the scope vocabulary, and the list pagination all come from the
 * shared `@sdkwork/iam-pc-console-cloud-account` package, and the service facade
 * driving them is composed by `@sdkwork/webserver-pc-console-core`. No list, form,
 * or request shape is re-declared here.
 *
 * The offered ownership levels come from the shared contract helper rather than
 * from a local `manageShared ? a : b`, so the tenant console and the platform
 * admin cannot drift apart in which levels they project — and the surface is one
 * of the inputs, which is what keeps the console personal-only. That projection is
 * a *rendering* decision, not an authorization one: the server refuses a
 * shared-level write from a caller without `iam.provider_accounts.manage_shared`, a
 * `platform` write from any tenant but the platform one, and a `platform` read
 * from anyone else, so hiding a level only avoids offering a dead end — showing it
 * would still be safe.
 */
export function CloudAccountManagementSurface({ permissionScope, surface = "console", tenantId }: CloudAccountManagementSurfaceProps) {
  const { iam } = useWebserverConsoleSdk();
  const controller = useMemo(
    () => createSdkworkIamConsoleCloudAccountController({ service: iam.service }),
    [iam.service],
  );

  const manageableScopeLevels = useMemo(
    () => resolveIamCloudAccountManageableScopeLevels(permissionScope, { surface, tenantId }),
    [permissionScope, surface, tenantId],
  );

  return (
    /*
     * The console shell's `<main class="workspace">` is a bare grid cell: it
     * sets no padding of its own, so every host-bridged page carries its own
     * content gutter (`.resource-page`, `.deploy-surface`, `.skills-console-surface`
     * … all do). Omitting it put this page's section header rule flush against
     * the sidebar, ran the account table's right border off the viewport edge,
     * and jammed the "new account" button into the corner — measured as
     * inset { top: 0, left: 0, right: 0 } against the workspace content box.
     *
     * The class also gives the page the one thing the shell cannot: it is a
     * *document* (listing plus a detail dialog), so it scrolls as a whole
     * rather than managing an inner scroll region.
     */
    <div className="cloud-account-surface">
      <SdkworkIamConsoleCloudAccountWorkspace
        controller={controller}
        manageableScopeLevels={manageableScopeLevels}
        surface={surface}
      />
    </div>
  );
}
