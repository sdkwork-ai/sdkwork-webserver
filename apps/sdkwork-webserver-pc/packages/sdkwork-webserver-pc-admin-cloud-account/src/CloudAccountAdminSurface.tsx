import {
  CloudAccountManagementSurface,
  type CloudAccountManagementSurfaceProps,
} from "@sdkwork/webserver-pc-console-cloud-account";

/**
 * Props of the backend-admin cloud account page.
 *
 * The console and the admin answer the same question from the same session facts,
 * so this is the console's prop type rather than a parallel declaration: a second
 * shape would be a second thing to keep in step, and the two hosts already pass
 * exactly the same two values.
 */
export type CloudAccountAdminSurfaceProps = CloudAccountManagementSurfaceProps;

/**
 * The platform admin's cloud account page.
 *
 * Cloud accounts are an IAM-owned resource with a single route set, so the admin
 * surface renders the tenant console's page instead of a second implementation —
 * the same act the tenant console performs when it renders IAM's page. This
 * component exists for one reason: to mark the render as the admin one, which
 * selects the admin descriptive copy.
 *
 * It deliberately does **not** widen the ownership levels. Which levels a caller
 * may be offered is derived from the session inside the shared adapter (via
 * `resolveIamCloudAccountManageableScopeLevels`), so the platform admin naturally
 * gains the `platform` level when the session is the platform tenant *and* holds
 * `iam.provider_accounts.manage_shared` — the same pair the server demands — and
 * does not gain it otherwise. Setting the levels here would be a second, local copy
 * of that rule — and the copy that drifts is always the one that keeps offering a
 * level the server refuses.
 */
export function CloudAccountAdminSurface({ permissionScope, tenantId }: CloudAccountAdminSurfaceProps) {
  return (
    <CloudAccountManagementSurface
      permissionScope={permissionScope}
      surface="admin"
      tenantId={tenantId}
    />
  );
}
