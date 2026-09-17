import {
  DEFAULT_MP_LIST_PAGE_SIZE,
  toWebserverMpListPage,
  toWebserverMpListQuery,
  type DeployAppPageInfo,
  type DeployAppResponse,
  type WebserverMpListPage,
} from "@sdkwork/webserver-mp-core/sdk";

import type { WebserverMpApplicationItem } from "../types/applicationModels";

/**
 * The exact slice of the generated deployments app client this screen needs.
 *
 * Naming the slice instead of the whole client keeps the dependency honest — the
 * list screen cannot reach for a create/update/delete operation it never
 * declared — while still being **structurally satisfied** by
 * `WebserverMpDeployAppClient`, so the runtime passes the real generated client
 * straight through with no adapter. It also lets verification inject a
 * deterministic double without casting one client type into another.
 */
export interface ApplicationsListReader {
  readonly app: {
    list(params: {
      page: number;
      pageSize: number;
    }): Promise<{ items: DeployAppResponse[]; pageInfo: DeployAppPageInfo }>;
  };
}

export interface WebserverMpApplicationsPage {
  readonly items: WebserverMpApplicationItem[];
  readonly page: WebserverMpListPage;
}

export interface WebserverMpApplicationsService {
  loadPage(page: number): Promise<WebserverMpApplicationsPage>;
}

/**
 * Map one generated record onto the view model. Returns `null` for a record the
 * list cannot render (no identity), so a malformed row is dropped instead of
 * producing a blank card.
 */
export function mapWebserverMpApplicationItem(
  record: DeployAppResponse | null | undefined,
): WebserverMpApplicationItem | null {
  if (!record || typeof record !== "object") return null;
  const id = typeof record.id === "string" && record.id.length > 0 ? record.id : undefined;
  if (!id) return null;
  return {
    id,
    name: typeof record.name === "string" && record.name.length > 0 ? record.name : id,
    slug: typeof record.slug === "string" ? record.slug : "",
    kind: record.appKind,
    status: record.appStatus,
    description: typeof record.description === "string" ? record.description : "",
    defaultEnvironment: typeof record.defaultEnvironment === "string" ? record.defaultEnvironment : "",
    latestReleaseTag: typeof record.latestReleaseTag === "string" ? record.latestReleaseTag : "",
    platformTargetCount: Number.isFinite(record.platformTargetCount)
      ? Number(record.platformTargetCount)
      : 0,
    updatedAt: typeof record.updatedAt === "string" ? record.updatedAt : "",
  };
}

/**
 * One-page-at-a-time `deploy_app` reader.
 *
 * `PAGINATION_SPEC.md` forbids full-set loads for interactive lists, so this
 * service asks the server for a single page and returns the canonical `pageInfo`
 * it got back. The client is injected — the service never constructs one and
 * never builds a transport.
 */
export function createWebserverMpApplicationsService(
  client: ApplicationsListReader,
  pageSize: number = DEFAULT_MP_LIST_PAGE_SIZE,
): WebserverMpApplicationsService {
  return {
    async loadPage(page: number): Promise<WebserverMpApplicationsPage> {
      if (!Number.isInteger(page) || page < 1) {
        throw new Error("page must be a positive integer");
      }
      const response = await client.app.list(toWebserverMpListQuery(page, pageSize));
      return {
        items: response.items
          .map(mapWebserverMpApplicationItem)
          .filter((item): item is WebserverMpApplicationItem => item !== null),
        page: toWebserverMpListPage(response.pageInfo),
      };
    },
  };
}
