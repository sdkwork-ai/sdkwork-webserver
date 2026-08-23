import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createSkillsAppClients,
  createSkillsBackendClients,
  SkillsClientsProvider,
} from "@sdkwork/skills-pc-core";
import {
  CreateSkillPage,
  EditSkillPage,
  MySkillsPage,
  SkillsConsoleLocaleProvider,
} from "@sdkwork/skills-pc-console-skills";
import { useMemo } from "react";
import { Route, Routes } from "react-router-dom";

/** Compatible with IAM session-auth boundary attachment (dual-token clients). */
type AttachSdkClientBoundaries = (
  clients: readonly { http?: unknown }[],
) => readonly { http?: unknown }[];

/**
 * Bridges the SDKWork Skills self-service console into the Web Server
 * console. The menu entry (My Skills) stays in the host while the pages are
 * the canonical sdkwork-skills implementation, sharing the IAM dual-token
 * session through the injected token manager. Styles are scoped by
 * `.skills-console-surface`.
 */
export interface SkillsConsoleSurfaceProps {
  appApiBaseUrl: string;
  attachSdkClientBoundaries?: AttachSdkClientBoundaries;
  backendApiBaseUrl: string;
  driveAppApiBaseUrl: string;
  locale?: string | null;
  resource: "skills";
  tokenManager: AuthTokenManager;
}

export function SkillsConsoleSurface({
  appApiBaseUrl,
  attachSdkClientBoundaries,
  backendApiBaseUrl,
  driveAppApiBaseUrl,
  locale,
  tokenManager,
}: SkillsConsoleSurfaceProps) {
  const clients = useMemo(() => {
    const next = {
      ...createSkillsAppClients({ appApiBaseUrl, driveAppApiBaseUrl, tokenManager }),
      ...createSkillsBackendClients({ backendApiBaseUrl, tokenManager }),
    };
    // Dual-token only — never project x-sdkwork-tenant-id (API_SPEC §10.2).
    attachSdkClientBoundaries?.([next.app, next.backend, next.drive]);
    return next;
  }, [
    appApiBaseUrl,
    attachSdkClientBoundaries,
    backendApiBaseUrl,
    driveAppApiBaseUrl,
    tokenManager,
  ]);
  const localeKey = locale?.trim() || "en-US";
  return (
    <div className="skills-console-surface" lang={localeKey}>
      <SkillsConsoleLocaleProvider key={localeKey} locale={locale}>
        <SkillsClientsProvider clients={clients}>
          <Routes>
            <Route path="" element={<MySkillsPage />} />
            <Route path="mine" element={<MySkillsPage />} />
            <Route path="create" element={<CreateSkillPage />} />
            <Route path="edit/:packageId" element={<EditSkillPage />} />
          </Routes>
        </SkillsClientsProvider>
      </SkillsConsoleLocaleProvider>
    </div>
  );
}
