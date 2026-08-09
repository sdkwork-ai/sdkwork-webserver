import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createSkillsAppClients,
  createSkillsBackendClients,
  SkillsClientsProvider,
} from "@sdkwork/skills-pc-core";
import { CreateSkillPage, MySkillsPage } from "@sdkwork/skills-pc-console-skills";
import { useMemo } from "react";
import { Route, Routes } from "react-router-dom";

/**
 * Bridges the SDKWork Skills self-service console into the Web Server
 * console. The menu entry (My Skills) stays in the host while the pages are
 * the canonical sdkwork-skills implementation, sharing the IAM dual-token
 * session through the injected token manager. Styles are scoped by
 * `.skills-console-surface`.
 */
export interface SkillsConsoleSurfaceProps {
  appApiBaseUrl: string;
  backendApiBaseUrl: string;
  driveAppApiBaseUrl: string;
  resource: "skills";
  tokenManager: AuthTokenManager;
}

export function SkillsConsoleSurface({
  appApiBaseUrl,
  backendApiBaseUrl,
  driveAppApiBaseUrl,
  tokenManager,
}: SkillsConsoleSurfaceProps) {
  const clients = useMemo(
    () => ({
      ...createSkillsAppClients({ appApiBaseUrl, driveAppApiBaseUrl, tokenManager }),
      ...createSkillsBackendClients({ backendApiBaseUrl, tokenManager }),
    }),
    [appApiBaseUrl, backendApiBaseUrl, driveAppApiBaseUrl, tokenManager],
  );
  return (
    <div className="skills-console-surface">
      <SkillsClientsProvider clients={clients}>
        <Routes>
          <Route path="" element={<MySkillsPage />} />
          <Route path="mine" element={<MySkillsPage />} />
          <Route path="create" element={<CreateSkillPage />} />
        </Routes>
      </SkillsClientsProvider>
    </div>
  );
}
