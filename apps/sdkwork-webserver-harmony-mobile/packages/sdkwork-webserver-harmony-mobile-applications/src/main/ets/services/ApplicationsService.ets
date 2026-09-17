import {
  toWebserverHarmonyListPage,
  type WebserverApplicationsListReader,
  type WebserverHarmonyListPage,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-core';

import type { WebserverHarmonyApplicationItem } from '../models/ApplicationModels';
import { mapWebserverHarmonyApplicationRecord } from '../models/ApplicationRecordMapping';

/**
 * Use-case orchestration for the applications catalog.
 *
 * The reader is injected by root bootstrap; this service maps records and
 * interprets pagination only, and never constructs a transport
 * (`APP_SDK_INTEGRATION_SPEC.md` §2).
 */

export interface WebserverHarmonyApplicationsPage {
  readonly items: WebserverHarmonyApplicationItem[];
  readonly page: WebserverHarmonyListPage;
}

export interface WebserverHarmonyApplicationsService {
  loadPage(page: number, pageSize?: number): Promise<WebserverHarmonyApplicationsPage>;
}

/**
 * Map one generated record onto the view model. Returns `null` for a record the
 * list cannot render (no identity), so a malformed row is dropped instead of
 * producing a blank card.
 */
export function mapWebserverHarmonyApplicationItem(
  record: WebserverApplicationRecord | null | undefined,
): WebserverHarmonyApplicationItem | null {
  if (record === null || record === undefined) {
    return null;
  }
  const id: string = typeof record.id === 'string' ? record.id : '';
  if (id.length === 0) {
    return null;
  }
  const name: string = typeof record.name === 'string' && record.name.length > 0
    ? record.name
    : id;
  const platformTargetCount: number = Number.isFinite(record.platformTargetCount)
    ? record.platformTargetCount
    : 0;
  return {
    id,
    name,
    slug: typeof record.slug === 'string' ? record.slug : '',
    appKind: typeof record.appKind === 'string' ? record.appKind : '',
    appStatus: typeof record.appStatus === 'string' ? record.appStatus : '',
    description: typeof record.description === 'string' ? record.description : '',
    defaultEnvironment: typeof record.defaultEnvironment === 'string'
      ? record.defaultEnvironment
      : '',
    latestReleaseTag: typeof record.latestReleaseTag === 'string'
      ? record.latestReleaseTag
      : '',
    platformTargetCount,
    updatedAt: typeof record.updatedAt === 'string' ? record.updatedAt : '',
  };
}

/**
 * One-page-at-a-time `deploy_app` reader.
 *
 * `PAGINATION_SPEC.md` forbids full-set loads for interactive lists, so this
 * service asks the server for a single page and returns the canonical `pageInfo`
 * it got back.
 */
export function createWebserverHarmonyApplicationsService(
  reader: WebserverApplicationsListReader,
  pageSize: number = 20,
): WebserverHarmonyApplicationsService {
  const resolvedPageSize: number = pageSize > 0 ? Math.floor(pageSize) : 20;
  return {
    loadPage: async (
      page: number,
      pageSizeOverride?: number,
    ): Promise<WebserverHarmonyApplicationsPage> => {
      if (!Number.isInteger(page) || page < 1) {
        throw new Error('page must be a positive integer');
      }
      const size: number = pageSizeOverride !== undefined && pageSizeOverride > 0
        ? Math.floor(pageSizeOverride)
        : resolvedPageSize;
      const response = await reader.list(page, size);
      const items: WebserverHarmonyApplicationItem[] = [];
      for (const record of response.items) {
        const mapped: WebserverHarmonyApplicationItem | null =
          mapWebserverHarmonyApplicationRecord(record);
        if (mapped !== null) {
          items.push(mapped);
        }
      }
      return { items, page: toWebserverHarmonyListPage(response.pageInfo) };
    },
  };
}
