import { hasPermissionInScope } from "@sdkwork/iam-contracts";

const WEBSERVER_ADMIN_PERMISSIONS = [
  "web.nginx.write",
  "web.servers.read",
  "web.servers.files.read",
  "web.auditLogs.read",
  "skills.packages.manage",
  "skills.categories.manage",
  "skills.capabilities.manage",
  "skills.artifacts.manage",
  "mcp.admin.server.manage",
  "mcp.admin.category.manage",
  "mcp.admin.invocation.read",
  // Storage Center is an operations capability of this edge: the drive-owned
  // admin storage plane is only reachable through the webserver gateway, so a
  // storage operator must be able to open the backend-admin surface at all —
  // otherwise the module is permanently filtered out of their menu.
  "drive.storage.admin",
  // The cluster plane is a platform-operations capability of this edge: a
  // cluster operator must be able to open the backend-admin surface, or the
  // module is permanently filtered out of their menu.
  "web.cluster.read",
  "web.cluster.write",
  // Served Domains and Served Certificates are the delivery module's entry
  // gates (`web.applications.read` / `web.certificates.read`): an operator
  // granted exactly that pair must be able to open the backend-admin surface,
  // or both entries are permanently filtered out of their menu and /admin
  // redirects them to /console with no explanation. Every entry below the
  // gates keeps its own per-resource check.
  "web.applications.read",
  "web.certificates.read",
  // The platform-wide traffic reading is an operations capability of this edge:
  // without it in this list a traffic operator cannot open the backend-admin
  // surface at all, so the module would be permanently filtered out of their
  // menu even though the reading is exactly what they were granted. The
  // tenant-scoped reading needs no entry here — the console surface renders its
  // entries for any authenticated user.
  "web.traffic.read",
  // Identity & Access is an IAM-owned operations capability of this edge: the
  // IAM backend API answers in-process, so an identity operator must be able to
  // open the backend-admin surface or the module is permanently filtered out of
  // their menu. These are the entry-gate codes the `iam` module's menu checks —
  // each entry below them is still filtered by its own code, so an operator
  // holding only, say, `iam.users.read` sees the directory and nothing else.
  "iam.users.read",
  "iam.organizations.read",
  "iam.tenants.read",
  "iam.tenant_applications.update",
  "iam.roles.read",
  "iam.permissions.read",
  "iam.policies.read",
  "iam.role_bindings.read",
  "iam.oauth.read",
  "iam.account_binding_policy.read",
  "iam.audit_events.read",
  // The app-template catalog is a deployments-owned operations capability: a
  // template moderator must be able to open the backend-admin surface or the
  // module is permanently filtered out of their menu. These are the entry-gate
  // codes the `app-templates` module's menu checks — the write codes the
  // moderation transitions perform stay on the entries' own checks.
  "deploy.templateCategories.read",
  "deploy.appTemplates.read",
  "deploy.appTemplateVersions.read",
] as const;

const WEBSERVER_SUPER_ADMIN_PERMISSIONS = [
  "web.certificates.read",
  "web.certificates.write",
  "web.nginx.write",
  "web.servers.read",
  "web.servers.write",
  "web.auditLogs.read",
] as const;

export function hasWebserverPermission(
  permissionScope: readonly string[],
  requiredPermission: string,
): boolean {
  return hasPermissionInScope(permissionScope, requiredPermission);
}

/** Console pages are reachable for any authenticated user; admin keeps IAM checks. */
export function canAccessWebserverResource(
  surface: "app-console" | "backend-admin",
  permissionScope: readonly string[],
  requiredPermission: string,
): boolean {
  if (surface === "app-console") {
    return true;
  }
  return hasWebserverPermission(permissionScope, requiredPermission);
}

export function hasWebserverAdminAccess(permissionScope: readonly string[]): boolean {
  return WEBSERVER_ADMIN_PERMISSIONS.some((permission) =>
    hasWebserverPermission(permissionScope, permission),
  );
}

export function hasWebserverSuperAdminAccess(permissionScope: readonly string[]): boolean {
  return WEBSERVER_SUPER_ADMIN_PERMISSIONS.every((permission) =>
    hasWebserverPermission(permissionScope, permission),
  );
}

export function hasPlatformSuperAdminAccess(permissionScope: readonly string[]): boolean {
  return permissionScope.includes("*");
}
