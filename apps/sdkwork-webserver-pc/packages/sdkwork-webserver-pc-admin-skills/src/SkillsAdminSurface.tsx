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
import { Link, Route, Routes } from "react-router-dom";

/**
 * Bridges the SDKWork Skills admin surface into the Web Server backend-admin
 * console. The menu entry (Skills Admin) stays in the host while the pages
 * are the canonical sdkwork-skills implementation, sharing the IAM dual-token
 * session through the injected token manager. Styles are scoped by
 * `.skills-admin-surface`.
 */
export interface SkillsAdminSurfaceProps {
  appApiBaseUrl: string;
  backendApiBaseUrl: string;
  driveAppApiBaseUrl: string;
  resource: "skills";
  tokenManager: AuthTokenManager;
  permissionScope: readonly string[];
  roleCodes?: readonly string[];
}

export function SkillsAdminSurface({
  appApiBaseUrl,
  backendApiBaseUrl,
  driveAppApiBaseUrl,
  tokenManager,
  permissionScope,
  roleCodes = [],
}: SkillsAdminSurfaceProps) {
  const clients = useMemo(
    () => ({
      ...createSkillsAppClients({ appApiBaseUrl, driveAppApiBaseUrl, tokenManager }),
      ...createSkillsBackendClients({ backendApiBaseUrl, tokenManager }),
    }),
    [appApiBaseUrl, backendApiBaseUrl, driveAppApiBaseUrl, tokenManager],
  );
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
            element={<AdminSkillsPage grantedPermissions={permissionScope} roleCodes={roleCodes} />}
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
