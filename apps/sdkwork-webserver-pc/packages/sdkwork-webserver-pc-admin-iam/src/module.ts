import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "iam",
  label: "iam",
  surface: "backend-admin",
  // Every entry sits under the module's own `/admin/iam` prefix, exactly like
  // Storage Center's `/admin/storage/<child>` and Cluster's `/admin/cluster/*`.
  // No entry claims the bare `iam` path: `/admin/iam` itself is the tab landing
  // route (the first visible entry wins), and an entry owning it would make one
  // operator URL resolve to a page while the tab treats it as its own root.
  // Entry order mirrors sdkwork-cloudrouter's IAM admin menu (directory, access
  // control, OAuth, federation, audit) so the two applications read the same.
  entries: [
    { resource: "iam-users", label: "Users", description: "Identity accounts and their lifecycle", permission: "iam.users.read", order: 1, path: "iam/users" },
    { resource: "iam-organizations", label: "Organizations", description: "Organization, department, and position structure", permission: "iam.organizations.read", order: 2, path: "iam/organizations" },
    { resource: "iam-tenants", label: "Tenants", description: "Tenants and their membership", permission: "iam.tenants.read", order: 3, path: "iam/tenants" },
    { resource: "iam-applications", label: "Applications", description: "Applications and endpoints registered for the tenant", permission: "iam.tenant_applications.update", order: 4, path: "iam/applications" },
    { resource: "iam-roles", label: "Roles", description: "Role catalog and role permission bindings", permission: "iam.roles.read", order: 5, path: "iam/roles" },
    { resource: "iam-permissions", label: "Permissions", description: "Backend operation permission catalog", permission: "iam.permissions.read", order: 6, path: "iam/permissions" },
    { resource: "iam-policies", label: "Policies", description: "Authorization policy management", permission: "iam.policies.read", order: 7, path: "iam/policies" },
    { resource: "iam-authorizations", label: "Authorizations", description: "Role bindings and grant relationships", permission: "iam.role_bindings.read", order: 8, path: "iam/authorizations" },
    { resource: "iam-oauth-providers", label: "Third-Party Platform Login", description: "Third-party OAuth provider connections", permission: "iam.oauth.read", order: 9, path: "iam/oauth/providers" },
    { resource: "iam-oauth-mini-programs", label: "Mini Program Accounts", description: "Mini program account management", permission: "iam.oauth.read", order: 10, path: "iam/oauth/mini-programs" },
    { resource: "iam-oauth-official-accounts", label: "Official Accounts", description: "Official accounts and their custom menus", permission: "iam.oauth.read", order: 11, path: "iam/oauth/official-accounts" },
    { resource: "iam-oauth-scan-login", label: "Scan Login", description: "Scan-login mode configuration", permission: "iam.oauth.read", order: 12, path: "iam/oauth/scan-login" },
    { resource: "iam-account-binding", label: "Account Binding", description: "Account binding policy", permission: "iam.account_binding_policy.read", order: 13, path: "iam/account-binding" },
    { resource: "iam-audit", label: "Audit Logs", description: "IAM audit event trail", permission: "iam.audit_events.read", order: 14, path: "iam/audit" }
  ],
} as const satisfies WebserverPcModuleDefinition;
