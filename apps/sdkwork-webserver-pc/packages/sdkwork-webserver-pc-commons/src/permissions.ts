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
] as const;

const WEBSERVER_SUPER_ADMIN_PERMISSIONS = [
  "web.applications.read",
  "web.applications.write",
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
