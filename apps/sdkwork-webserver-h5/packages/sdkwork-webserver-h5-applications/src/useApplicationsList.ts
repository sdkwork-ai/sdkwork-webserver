import { useCallback, useEffect, useState } from "react";

import {
  DEFAULT_H5_LIST_PAGE_SIZE,
  toWebserverH5ListPage,
  toWebserverH5ListQuery,
  type DeployAppPageInfo,
  type DeployAppResponse,
  type WebserverH5ListPage,
} from "@sdkwork/webserver-h5-core/sdk";

export type ApplicationsListStatus = "error" | "loading" | "ready";

/**
 * The exact slice of the generated deployments app client this screen needs.
 *
 * Naming the slice instead of the whole client keeps the dependency honest — the
 * list screen cannot reach for a create/update/delete operation it never
 * declared — while still being **structurally satisfied** by
 * `WebserverH5DeployAppClient`, so the application root passes the real
 * generated client straight through with no adapter. It also lets verification
 * inject a deterministic double without casting one client type into another.
 */
export interface ApplicationsListReader {
  readonly app: {
    list(params: {
      page: number;
      pageSize: number;
    }): Promise<{ items: DeployAppResponse[]; pageInfo: DeployAppPageInfo }>;
  };
}

export interface UseApplicationsListResult {
  /** Rows loaded so far, in server order. */
  readonly items: readonly DeployAppResponse[];
  /** Canonical page envelope of the last successful response. */
  readonly page: WebserverH5ListPage;
  readonly status: ApplicationsListStatus;
  /** True while an *additional* page is in flight rather than the first one. */
  readonly appending: boolean;
  /**
   * Technical detail of the last failure. The screen renders localized copy and
   * treats this only as a secondary, non-authoritative hint.
   */
  readonly failureDetail: string | undefined;
  reload: () => void;
  loadMore: () => void;
}

/**
 * One-page-at-a-time `deploy_app` list reader.
 *
 * `PAGINATION_SPEC.md` forbids full-set loads for interactive lists, so this
 * hook asks the server for a single page, keeps the canonical `pageInfo` it gets
 * back, and appends the next page only when the operator asks for it. The client
 * is injected — the hook never constructs one and never builds a transport.
 */
export function useApplicationsList(
  deployClient: ApplicationsListReader,
  pageSize: number = DEFAULT_H5_LIST_PAGE_SIZE,
): UseApplicationsListResult {
  const [items, setItems] = useState<readonly DeployAppResponse[]>([]);
  const [page, setPage] = useState<WebserverH5ListPage>(() => toWebserverH5ListPage(undefined));
  const [status, setStatus] = useState<ApplicationsListStatus>("loading");
  const [failureDetail, setFailureDetail] = useState<string | undefined>(undefined);
  const [requestedPage, setRequestedPage] = useState(1);
  const [reloadToken, setReloadToken] = useState(0);

  useEffect(() => {
    let cancelled = false;
    const isFirstPage = requestedPage <= 1;
    setStatus("loading");

    void (async () => {
      try {
        const response = await deployClient.app.list(
          toWebserverH5ListQuery(requestedPage, pageSize),
        );
        if (cancelled) return;
        setItems((previous) => (isFirstPage ? response.items : [...previous, ...response.items]));
        setPage(toWebserverH5ListPage(response.pageInfo));
        setFailureDetail(undefined);
        setStatus("ready");
      } catch (error) {
        if (cancelled) return;
        setFailureDetail(error instanceof Error ? error.message : String(error));
        setStatus("error");
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [deployClient, pageSize, reloadToken, requestedPage]);

  const reload = useCallback(() => {
    setItems([]);
    setRequestedPage(1);
    setReloadToken((token) => token + 1);
  }, []);

  const loadMore = useCallback(() => {
    setRequestedPage((current) => current + 1);
  }, []);

  return {
    appending: status === "loading" && requestedPage > 1,
    failureDetail,
    items,
    loadMore,
    page,
    reload,
    status,
  };
}
