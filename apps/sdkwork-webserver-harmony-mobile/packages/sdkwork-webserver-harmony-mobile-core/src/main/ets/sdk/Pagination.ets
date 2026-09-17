/**
 * Domain-neutral pagination narrowing for HarmonyOS list surfaces.
 *
 * Authority: `PAGINATION_SPEC.md`. Capability packages page through the injected
 * reader and never re-derive their own totals, so every list on this root reports
 * the same `hasMore` semantics the server sent.
 */

export const DEFAULT_WEBSERVER_HARMONY_LIST_PAGE_SIZE: number = 20;

export interface WebserverHarmonyPageInfoLike {
  readonly page?: number;
  readonly pageSize?: number;
  readonly totalItems?: number;
  readonly totalPages?: number;
  readonly hasMore?: boolean;
}

export interface WebserverHarmonyListPage {
  readonly page: number;
  readonly pageSize: number;
  readonly totalItems: number;
  readonly totalPages: number;
  readonly hasMore: boolean;
}

function normalizeCount(value: number | undefined, fallback: number, minimum: number): number {
  const numeric: number = value === undefined ? fallback : value;
  if (!Number.isFinite(numeric)) {
    return fallback;
  }
  const rounded: number = Math.floor(numeric);
  return rounded < minimum ? fallback : rounded;
}

export function toWebserverHarmonyListPage(
  pageInfo: WebserverHarmonyPageInfoLike | null | undefined,
): WebserverHarmonyListPage {
  const page: number = normalizeCount(pageInfo?.page, 1, 1);
  const pageSize: number = normalizeCount(
    pageInfo?.pageSize,
    DEFAULT_WEBSERVER_HARMONY_LIST_PAGE_SIZE,
    1,
  );
  const totalPages: number = normalizeCount(pageInfo?.totalPages, 0, 1);
  const totalItems: number = normalizeCount(pageInfo?.totalItems, 0, 1);
  return {
    page,
    pageSize,
    totalItems,
    totalPages,
    hasMore: pageInfo?.hasMore === true || (totalPages > 0 && page < totalPages),
  };
}

/** Clamp a requested page/pageSize pair before it reaches the transport. */
export function normalizeWebserverHarmonyListPaging(
  page: number,
  pageSize: number = DEFAULT_WEBSERVER_HARMONY_LIST_PAGE_SIZE,
): WebserverHarmonyListPage {
  return {
    page: normalizeCount(page, 1, 1),
    pageSize: normalizeCount(pageSize, DEFAULT_WEBSERVER_HARMONY_LIST_PAGE_SIZE, 1),
    totalItems: 0,
    totalPages: 0,
    hasMore: false,
  };
}
