import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createSkillsAppClients,
  createSkillsBackendClients,
  SkillsClientsProvider,
} from "@sdkwork/skills-pc-core";
import {
  AdminCategoriesPage,
  AdminSkillsPage,
  PackageArtifactsPage,
  SkillCapabilitiesPage,
  UpdateSkillPackagePage,
} from "@sdkwork/skills-pc-admin-skill";
import { useMemo } from "react";
import { Link, Route, Routes, useSearchParams } from "react-router-dom";

/** Compatible with IAM session-auth boundary attachment (dual-token clients). */
type AttachSdkClientBoundaries = (
  clients: readonly { http?: unknown }[],
) => readonly { http?: unknown }[];

/**
 * Bridges the SDKWork Skills admin surface into the Web Server backend-admin
 * console. The menu entry (Skills Admin) stays in the host while the pages
 * are the canonical sdkwork-skills implementation, sharing the IAM dual-token
 * session through the injected token manager. Styles are scoped by
 * `.skills-admin-surface`.
 */
export interface SkillsAdminSurfaceProps {
  appApiBaseUrl: string;
  attachSdkClientBoundaries?: AttachSdkClientBoundaries;
  backendApiBaseUrl: string;
  driveAppApiBaseUrl: string;
  resource: "skills";
  tokenManager: AuthTokenManager;
  permissionScope: readonly string[];
  roleCodes?: readonly string[];
}

function AdminSkillsListPage({
  grantedPermissions,
  roleCodes,
}: {
  grantedPermissions: readonly string[];
  roleCodes: readonly string[];
}) {
  const [searchParams] = useSearchParams();
  const initialEditPackageId = searchParams.get("edit");
  return (
    <AdminSkillsPage
      grantedPermissions={grantedPermissions}
      roleCodes={roleCodes}
      initialEditPackageId={initialEditPackageId}
    />
  );
}

export function SkillsAdminSurface({
  appApiBaseUrl,
  attachSdkClientBoundaries,
  backendApiBaseUrl,
  driveAppApiBaseUrl,
  tokenManager,
  permissionScope,
  roleCodes = [],
}: SkillsAdminSurfaceProps) {
  const clients = useMemo(() => {
    const next = {
      ...createSkillsAppClients({ appApiBaseUrl, driveAppApiBaseUrl, tokenManager }),
      ...createSkillsBackendClients({ backendApiBaseUrl, tokenManager }),
    };
    attachSdkClientBoundaries?.([next.app, next.backend, next.drive]);
    return next;
  }, [
    appApiBaseUrl,
    attachSdkClientBoundaries,
    backendApiBaseUrl,
    driveAppApiBaseUrl,
    tokenManager,
  ]);
  return (
    <div className="skills-admin-surface">
      <SkillsClientsProvider clients={clients}>
        <nav className="mb-4 flex gap-3 text-sm font-medium">
          <Link to="/admin/skills" className="text-blue-600 hover:text-blue-700">
            Packages
          </Link>
          <Link to="/admin/skills/categories" className="text-blue-600 hover:text-blue-700">
            Categories
          </Link>
          <Link to="/admin/skills/capabilities" className="text-blue-600 hover:text-blue-700">
            Capabilities
          </Link>
        </nav>
        <Routes>
          <Route
            path=""
            element={
              <AdminSkillsListPage grantedPermissions={permissionScope} roleCodes={roleCodes} />
            }
          />
          <Route path="edit/:packageId" element={<UpdateSkillPackagePage />} />
          <Route path="artifacts/:packageId" element={<PackageArtifactsPage />} />
          <Route path="categories" element={<AdminCategoriesPage />} />
          <Route path="capabilities" element={<SkillCapabilitiesPage />} />
        </Routes>
      </SkillsClientsProvider>
    </div>
  );
}
