import { createClient as createIamAppClient } from "@sdkwork/iam-app-sdk";
import {
  DEFAULT_SELF_SERVICE_CAPABILITIES,
  SdkworkOfficialAccountsPanel,
  createOfficialAccountsController,
  createSelfServiceTransport,
  type OfficialAccountsController,
} from "@sdkwork/iam-pc-official-accounts";
import type { ReactNode } from "react";

type ConsoleTokenManager = NonNullable<
  Parameters<typeof createIamAppClient>[0]["tokenManager"]
>;

/**
 * Console-side official-account self-service page.
 *
 * The tenant console reaches the same unified component the desktop host mounts
 * (`@sdkwork/iam-pc-official-accounts`), in its self-service posture: the
 * caller sees and maintains only the `iam_oauth_resource_account` rows whose
 * `maintainer_user_id` is their own identity, over the IAM app API. No backend
 * admin client, no tenant-wide listing, no operator capability.
 *
 * The controller is cached per token manager + origin so React re-renders reuse
 * one store instead of rebuilding the list on every pass.
 */
const controllers = new WeakMap<object, Map<string, OfficialAccountsController>>();

function readController(
  tokenManager: ConsoleTokenManager,
  appApiBaseUrl: string,
): OfficialAccountsController {
  const baseUrl = appApiBaseUrl.trim().replace(/\/+$/u, "");
  const byOrigin = controllers.get(tokenManager) ?? new Map<string, OfficialAccountsController>();
  controllers.set(tokenManager, byOrigin);
  const existing = byOrigin.get(baseUrl);
  if (existing !== undefined) return existing;
  const client = createIamAppClient({
    authMode: "dual-token",
    baseUrl,
    platform: "pc",
    tokenManager,
  });
  const controller = createOfficialAccountsController(
    createSelfServiceTransport(client.iam.oauth.officialAccounts),
    DEFAULT_SELF_SERVICE_CAPABILITIES,
  );
  byOrigin.set(baseUrl, controller);
  return controller;
}

/** One console menu entry: the caller's own official accounts. */
export const officialAccountsConsoleModule = {
  entries: [
    {
      description: "Maintain the official accounts your own identity owns",
      label: "公众号账号 / Official Accounts",
      order: 90,
      permission: "iam:self",
      resource: "official-accounts" as const,
    },
  ],
  id: "channels",
  label: "渠道 / Channels",
  surface: "app-console" as const,
};

export function OfficialAccountsConsoleSurface({
  appApiBaseUrl,
  tokenManager,
}: {
  appApiBaseUrl: string;
  tokenManager: ConsoleTokenManager;
}): ReactNode {
  const controller = readController(tokenManager, appApiBaseUrl);
  return (
    <SdkworkOfficialAccountsPanel
      capabilities={DEFAULT_SELF_SERVICE_CAPABILITIES}
      controller={controller}
    />
  );
}
