/**
 * Narrows generated `pageInfo` payloads onto the mobile list model the H5
 * surfaces render. `PAGINATION_SPEC.md` forbids client-side full-set loads for
 * interactive lists, so feature packages paginate through the SDK cursor/offset
 * response and never re-derive their own totals.
 */
export interface WebserverH5ListPage {
  hasMore: boolean;
  page: number;
  pageSize: number;
  totalItems: number;
  totalPages: number;
}

export interface CanonicalPageInfoLike {
  hasMore?: boolean | null;
  page?: number | null;
  pageSize?: number | null;
  totalItems?: number | string | null;
  totalPages?: number | null;
}

export const DEFAULT_H5_LIST_PAGE_SIZE = 20;

export function toWebserverH5ListPage(
  pageInfo: CanonicalPageInfoLike | null | undefined,
): WebserverH5ListPage {
  const page = Math.max(1, pageInfo?.page ?? 1);
  const pageSize = Math.max(1, pageInfo?.pageSize ?? DEFAULT_H5_LIST_PAGE_SIZE);
  const totalPages = Math.max(0, pageInfo?.totalPages ?? 0);
  const rawTotalItems = Number(pageInfo?.totalItems ?? 0);
  const totalItems = Number.isFinite(rawTotalItems) ? rawTotalItems : 0;
  return {
    hasMore: pageInfo?.hasMore === true || (totalPages > 0 && page < totalPages),
    page,
    pageSize,
    totalItems,
    totalPages,
  };
}

/** Build the `page`/`page_size` query the generated list operations accept. */
export function toWebserverH5ListQuery(page: number, pageSize = DEFAULT_H5_LIST_PAGE_SIZE) {
  return { page: Math.max(1, page), pageSize: Math.max(1, pageSize) } as const;
}
