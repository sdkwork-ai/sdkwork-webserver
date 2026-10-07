import { hasPermissionInScope } from "@sdkwork/iam-contracts";
import { LoaderCircle } from "lucide-react";
import { lazy, Suspense, useMemo, type ComponentType } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { translateWebserver, type WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { createSdkworkIamUserDriveUploadImageService } from "./avatar-upload.ts";
import { createSdkworkIamOrganizationDriveUploadImageService, createSdkworkIamOrganizationLogoService } from "./organization-logo-upload.ts";
import { createSdkworkIamOauthAccountDriveUploadImageService } from "./oauth-account-logo-upload.ts";
import { useWebserverConsoleSdk } from "@sdkwork/webserver-pc-console-core";

/**
 * Identity & Access admin capability integration for the backend-admin surface.
 *
 * Each page lazy-loads the matching `@sdkwork/iam-pc-admin-*` capability
 * workspace — the same packages sdkwork-cloudrouter mounts — and wires it with
 * the shared `SdkworkIamService` facade, the session permission scope, and the
 * locale. Workspaces never create HTTP clients; the facade is composed by
 * `@sdkwork/webserver-pc-console-core` and read off the console SDK provider
 * that wraps both surfaces.
 *
 * The host's flat resource routes are one level deep (`/iam/<entry>/*`), so the
 * two workspaces with their own sub-pages (organization structure, OAuth custom
 * menus) read their sub-path off the route's trailing splat instead of owning
 * routes of their own — exactly the segments cloudrouter carries as dedicated
 * routes (`/admin/iam/organizations/:organizationId/structure`,
 * `/admin/iam/oauth/official-accounts/:resourceAccountId/custom-menus`).
 */

export type IamAdminResource =
  | "iam-users"
  | "iam-organizations"
  | "iam-tenants"
  | "iam-applications"
  | "iam-roles"
  | "iam-permissions"
  | "iam-policies"
  | "iam-authorizations"
  | "iam-oauth-providers"
  | "iam-oauth-mini-programs"
  | "iam-oauth-official-accounts"
  | "iam-oauth-scan-login"
  | "iam-account-binding"
  | "iam-audit";

export interface IamAdminSurfaceProps {
  locale: WebserverLocale;
  /** Session permission codes; gate the workspaces' mutation affordances. */
  permissionScope: readonly string[];
  resource: IamAdminResource;
  /** Session tenant; seeds the tenant applications page's initial selection. */
  tenantId?: string;
}

type IamAdminContentProps = Omit<IamAdminSurfaceProps, "resource">;

/** Sub-path segment matcher: `<id>/structure` under the organizations entry. */
const ORGANIZATION_STRUCTURE_SPLAT = /^([^/]+)\/structure$/;
/** Sub-path segment matcher: `<id>/custom-menus` under the official accounts entry. */
const OAUTH_CUSTOM_MENU_SPLAT = /^([^/]+)\/custom-menus$/;

function IamAdminLoading({ locale }: { locale: WebserverLocale }) {
  return (
    <div className="iam-admin-loading" role="status">
      <LoaderCircle aria-hidden="true" className="is-spinning" size={18} />
      <span>{translateWebserver(locale, "iam.loading")}</span>
    </div>
  );
}

function useIamAdminService() {
  const { iam } = useWebserverConsoleSdk();
  return iam.service;
}

function useIamAdminPermission(permissionScope: readonly string[]): (permission: string) => boolean {
  return useMemo(
    () => (permission: string) => hasPermissionInScope(permissionScope, permission),
    [permissionScope],
  );
}

const LazyIamUsersAdmin = lazy(async () => {
  const { createSdkworkIamUserAdminController, SdkworkIamUserAdminWorkspace } =
    await import("@sdkwork/iam-pc-admin-user");
  return {
    default: function IamUsersAdminContent({ locale, permissionScope }: IamAdminContentProps) {
      const { drive, iam } = useWebserverConsoleSdk();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(() => createSdkworkIamUserAdminController(iam.service), [iam.service]);
      const driveUploadImageService = useMemo(
        () => createSdkworkIamUserDriveUploadImageService(drive),
        [drive],
      );
      return (
        <SdkworkIamUserAdminWorkspace
          controller={controller}
          driveUploadImageService={driveUploadImageService}
          locale={locale}
          permissions={{
            create: can("iam.users.create"),
            delete: can("iam.users.delete"),
            update: can("iam.users.update"),
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamTenantsAdmin = lazy(async () => {
  const { createSdkworkIamTenantController, SdkworkIamTenantAdminWorkspace } =
    await import("@sdkwork/iam-pc-admin-tenant");
  return {
    default: function IamTenantsAdminContent({ permissionScope }: IamAdminContentProps) {
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(
        () => createSdkworkIamTenantController({ permissionScope, service }),
        [permissionScope, service],
      );
      return (
        <SdkworkIamTenantAdminWorkspace
          controller={controller}
          permissions={{
            members: {
              create: can("iam.tenant_members.create"),
              delete: can("iam.tenant_members.delete"),
              read: can("iam.tenant_members.read"),
              update: can("iam.tenant_members.update"),
            },
            tenants: {
              create: can("iam.tenants.create"),
              delete: can("iam.tenants.delete"),
              update: can("iam.tenants.update"),
            },
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamApplicationsAdmin = lazy(async () => {
  const { createSdkworkIamTenantController, SdkworkIamTenantApplicationsAdminWorkspace } =
    await import("@sdkwork/iam-pc-admin-tenant");
  return {
    default: function IamApplicationsAdminContent({ permissionScope, tenantId }: IamAdminContentProps) {
      const service = useIamAdminService();
      // The applications page targets the operator's current tenant; the
      // session-scoped tenant id seeds the controller's initial selection so
      // no tenant picker is rendered.
      const controller = useMemo(
        () => createSdkworkIamTenantController({ permissionScope, selectedTenantId: tenantId, service }),
        [permissionScope, service, tenantId],
      );
      return <SdkworkIamTenantApplicationsAdminWorkspace controller={controller} />;
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamOrganizationsAdmin = lazy(async () => {
  const {
    createSdkworkIamOrganizationController,
    SdkworkIamOrganizationAdminWorkspace,
    SdkworkIamOrganizationStructureWorkspace,
  } = await import("@sdkwork/iam-pc-admin-organization");
  return {
    default: function IamOrganizationsAdminContent({ permissionScope }: IamAdminContentProps) {
      const { drive, iam } = useWebserverConsoleSdk();
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(() => createSdkworkIamOrganizationController(service), [service]);
      const logoService = useMemo(() => createSdkworkIamOrganizationLogoService(drive), [drive]);
      const driveUploadImageService = useMemo(
        () => createSdkworkIamOrganizationDriveUploadImageService(drive),
        [drive],
      );
      const navigate = useNavigate();
      const structureMatch = ORGANIZATION_STRUCTURE_SPLAT.exec(useParams()["*"] ?? "");
      if (structureMatch) {
        return (
          <SdkworkIamOrganizationStructureWorkspace
            controller={controller}
            onBack={() => navigate("/admin/iam/organizations")}
            organizationId={decodeURIComponent(structureMatch[1])}
            permissions={{
              assignments: {
                create: can("iam.assignments.create"),
                read: can("iam.assignments.read"),
                update: can("iam.assignments.update"),
              },
              departments: {
                create: can("iam.departments.create"),
                delete: can("iam.departments.delete"),
                update: can("iam.departments.update"),
              },
              memberships: { read: can("iam.memberships.read") },
            }}
          />
        );
      }
      return (
        <SdkworkIamOrganizationAdminWorkspace
          controller={controller}
          driveUploadImageService={driveUploadImageService}
          logoService={logoService}
          onOpenStructure={(organization) =>
            navigate(`/admin/iam/organizations/${encodeURIComponent(organization.organizationId)}/structure`)}
          permissions={{
            departments: {
              create: can("iam.departments.create"),
              delete: can("iam.departments.delete"),
              read: can("iam.departments.read"),
              update: can("iam.departments.update"),
            },
            memberships: {
              create: can("iam.memberships.create"),
              read: can("iam.memberships.read"),
              update: can("iam.memberships.update"),
            },
            organizations: {
              create: can("iam.organizations.create"),
              delete: can("iam.organizations.delete"),
              update: can("iam.organizations.update"),
            },
            positions: { read: can("iam.positions.read") },
            roleBindings: { read: can("iam.role_bindings.read") },
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamRolesAdmin = lazy(async () => {
  const { createSdkworkIamPermissionController, SdkworkIamRoleAdminWorkspace } =
    await import("@sdkwork/iam-pc-admin-permission");
  return {
    default: function IamRolesAdminContent({ locale, permissionScope }: IamAdminContentProps) {
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(
        () => createSdkworkIamPermissionController({ permissionScope, service }),
        [permissionScope, service],
      );
      return (
        <SdkworkIamRoleAdminWorkspace
          controller={controller}
          locale={locale}
          permissions={{
            roleBindings: {
              create: can("iam.role_bindings.create"),
              delete: can("iam.role_bindings.delete"),
            },
            rolePermissions: {
              create: can("iam.role_permissions.create"),
              delete: can("iam.role_permissions.delete"),
            },
            roles: {
              create: can("iam.roles.create"),
              delete: can("iam.roles.delete"),
              update: can("iam.roles.update"),
            },
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamPermissionsAdmin = lazy(async () => {
  const { createSdkworkIamPermissionController, SdkworkIamPermissionAdminWorkspace } =
    await import("@sdkwork/iam-pc-admin-permission");
  return {
    default: function IamPermissionsAdminContent({ locale, permissionScope }: IamAdminContentProps) {
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(
        () => createSdkworkIamPermissionController({ permissionScope, service }),
        [permissionScope, service],
      );
      return (
        <SdkworkIamPermissionAdminWorkspace
          controller={controller}
          locale={locale}
          permissions={{
            permissions: {
              create: can("iam.permissions.create"),
              delete: can("iam.permissions.delete"),
              update: can("iam.permissions.update"),
            },
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamPoliciesAdmin = lazy(async () => {
  const { createSdkworkIamPermissionController, SdkworkIamPolicyAdminWorkspace } =
    await import("@sdkwork/iam-pc-admin-permission");
  return {
    default: function IamPoliciesAdminContent({ locale, permissionScope }: IamAdminContentProps) {
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(
        () => createSdkworkIamPermissionController({ permissionScope, service }),
        [permissionScope, service],
      );
      return (
        <SdkworkIamPolicyAdminWorkspace
          controller={controller}
          locale={locale}
          permissions={{
            policies: {
              create: can("iam.policies.create"),
              delete: can("iam.policies.delete"),
              update: can("iam.policies.update"),
            },
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamAuthorizationsAdmin = lazy(async () => {
  const { createSdkworkIamPermissionController, SdkworkIamAuthorizationAdminWorkspace } =
    await import("@sdkwork/iam-pc-admin-permission");
  return {
    default: function IamAuthorizationsAdminContent({ locale, permissionScope }: IamAdminContentProps) {
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(
        () => createSdkworkIamPermissionController({ permissionScope, service }),
        [permissionScope, service],
      );
      return (
        <SdkworkIamAuthorizationAdminWorkspace
          controller={controller}
          locale={locale}
          permissions={{
            roleBindings: {
              create: can("iam.role_bindings.create"),
              delete: can("iam.role_bindings.delete"),
            },
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamOauthProvidersAdmin = lazy(async () => {
  const { createSdkworkIamOauthAdminController, SdkworkIamOauthProviderConnectionsPage } =
    await import("@sdkwork/iam-pc-admin-oauth");
  return {
    default: function IamOauthProvidersAdminContent({ permissionScope }: IamAdminContentProps) {
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(() => createSdkworkIamOauthAdminController(service), [service]);
      return (
        <SdkworkIamOauthProviderConnectionsPage
          controller={controller}
          permissions={{
            create: can("iam.oauth.integrations.create"),
            delete: can("iam.oauth.integrations.delete"),
            update: can("iam.oauth.integrations.update"),
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamOauthMiniProgramsAdmin = lazy(async () => {
  const { createSdkworkIamOauthAdminController, SdkworkIamOauthMiniProgramAccountsPage } =
    await import("@sdkwork/iam-pc-admin-oauth");
  return {
    default: function IamOauthMiniProgramsAdminContent({ permissionScope }: IamAdminContentProps) {
      const { drive } = useWebserverConsoleSdk();
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(() => createSdkworkIamOauthAdminController(service), [service]);
      const driveUploadImageService = useMemo(
        () => createSdkworkIamOauthAccountDriveUploadImageService(drive),
        [drive],
      );
      return (
        <SdkworkIamOauthMiniProgramAccountsPage
          controller={controller}
          driveUploadImageService={driveUploadImageService}
          permissions={{
            create: can("iam.oauth.resourceAccounts.create"),
            delete: can("iam.oauth.resourceAccounts.delete"),
            update: can("iam.oauth.resourceAccounts.update"),
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamOauthOfficialAccountsAdmin = lazy(async () => {
  const {
    createSdkworkIamOauthAdminController,
    SdkworkIamOauthOfficialAccountCustomMenuPage,
    SdkworkIamOauthOfficialAccountsPage,
  } = await import("@sdkwork/iam-pc-admin-oauth");
  return {
    default: function IamOauthOfficialAccountsAdminContent({ permissionScope }: IamAdminContentProps) {
      const { drive } = useWebserverConsoleSdk();
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(() => createSdkworkIamOauthAdminController(service), [service]);
      const driveUploadImageService = useMemo(
        () => createSdkworkIamOauthAccountDriveUploadImageService(drive),
        [drive],
      );
      const navigate = useNavigate();
      const customMenuMatch = OAUTH_CUSTOM_MENU_SPLAT.exec(useParams()["*"] ?? "");
      // The custom menu manager opens as a full-screen view inside the page;
      // the sub-path remains for deep links. Its editor answers to the
      // dedicated customMenus codes rather than the account-mutation family.
      if (customMenuMatch) {
        return (
          <SdkworkIamOauthOfficialAccountCustomMenuPage
            controller={controller}
            onClose={() => navigate("/admin/iam/oauth/official-accounts")}
            permissions={{
              publish: can("iam.oauth.resourceAccounts.customMenus.publish"),
              update: can("iam.oauth.resourceAccounts.customMenus.update"),
            }}
            resourceAccountId={decodeURIComponent(customMenuMatch[1])}
          />
        );
      }
      return (
        <SdkworkIamOauthOfficialAccountsPage
          controller={controller}
          customMenuPermissions={{
            publish: can("iam.oauth.resourceAccounts.customMenus.publish"),
            update: can("iam.oauth.resourceAccounts.customMenus.update"),
          }}
          driveUploadImageService={driveUploadImageService}
          permissions={{
            create: can("iam.oauth.resourceAccounts.create"),
            delete: can("iam.oauth.resourceAccounts.delete"),
            update: can("iam.oauth.resourceAccounts.update"),
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamOauthScanLoginAdmin = lazy(async () => {
  const { createSdkworkIamOauthAdminController, SdkworkIamOauthScanLoginSettingsPage } =
    await import("@sdkwork/iam-pc-admin-oauth");
  return {
    default: function IamOauthScanLoginAdminContent({ permissionScope }: IamAdminContentProps) {
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(() => createSdkworkIamOauthAdminController(service), [service]);
      return (
        <SdkworkIamOauthScanLoginSettingsPage
          accountPermissions={{ create: can("iam.oauth.resourceAccounts.create") }}
          controller={controller}
          permissions={{
            create: can("iam.oauth.scanLoginPreviews.create"),
            update: can("iam.oauth.scanLoginSettings.update"),
          }}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamAccountBindingAdmin = lazy(async () => {
  const { createSdkworkIamAccountBindingController, SdkworkIamAccountBindingSettings } =
    await import("@sdkwork/iam-pc-admin-account-binding");
  return {
    default: function IamAccountBindingAdminContent({ permissionScope }: IamAdminContentProps) {
      const service = useIamAdminService();
      const can = useIamAdminPermission(permissionScope);
      const controller = useMemo(() => createSdkworkIamAccountBindingController(service), [service]);
      return (
        <SdkworkIamAccountBindingSettings
          canUpdate={can("iam.account_binding_policy.update")}
          controller={controller}
        />
      );
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

const LazyIamAuditAdmin = lazy(async () => {
  const { createSdkworkIamAuditController, SdkworkIamAuditAdminWorkspace } =
    await import("@sdkwork/iam-pc-admin-audit");
  return {
    default: function IamAuditAdminContent() {
      const service = useIamAdminService();
      const controller = useMemo(() => createSdkworkIamAuditController(service), [service]);
      return <SdkworkIamAuditAdminWorkspace controller={controller} />;
    } satisfies ComponentType<IamAdminContentProps>,
  };
});

/**
 * Page registry behind the flat resource routes. Keyed by `IamAdminResource`
 * (== the module entry's `resource`), one lazy workspace per entry — the same
 * page set, in the same menu order, sdkwork-cloudrouter's IAM admin mounts.
 */
const IAM_ADMIN_PAGES: Readonly<Record<IamAdminResource, ComponentType<IamAdminContentProps>>> = {
  "iam-users": LazyIamUsersAdmin,
  "iam-organizations": LazyIamOrganizationsAdmin,
  "iam-tenants": LazyIamTenantsAdmin,
  "iam-applications": LazyIamApplicationsAdmin,
  "iam-roles": LazyIamRolesAdmin,
  "iam-permissions": LazyIamPermissionsAdmin,
  "iam-policies": LazyIamPoliciesAdmin,
  "iam-authorizations": LazyIamAuthorizationsAdmin,
  "iam-oauth-providers": LazyIamOauthProvidersAdmin,
  "iam-oauth-mini-programs": LazyIamOauthMiniProgramsAdmin,
  "iam-oauth-official-accounts": LazyIamOauthOfficialAccountsAdmin,
  "iam-oauth-scan-login": LazyIamOauthScanLoginAdmin,
  "iam-account-binding": LazyIamAccountBindingAdmin,
  "iam-audit": LazyIamAuditAdmin,
};

export function IamAdminSurface({ locale, permissionScope, resource, tenantId }: IamAdminSurfaceProps) {
  const Page = IAM_ADMIN_PAGES[resource];
  return (
    /*
     * The workspace's `<main class="workspace">` is a bare grid cell: it sets
     * no padding of its own, so every host-bridged page carries its own
     * content gutter (`.cloud-account-surface`, `.deploy-surface`, … all do).
     */
    <div className="iam-admin-surface">
      <Suspense fallback={<IamAdminLoading locale={locale} />}>
        <Page locale={locale} permissionScope={permissionScope} tenantId={tenantId} />
      </Suspense>
    </div>
  );
}
