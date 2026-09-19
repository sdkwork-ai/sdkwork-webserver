import {
  resolveWebserverMpScreenStatus,
  type WebserverMpScreenStatus,
} from "@sdkwork/webserver-mp-commons";
import type { DeployAppKind, DeployAppStatus } from "@sdkwork/webserver-mp-core/sdk";

import type { WebserverMpApplicationsService } from "../services/ApplicationsService";
import type { WebserverMpApplicationItem } from "../types/applicationModels";

/**
 * Applications list page model.
 *
 * Everything the native page does — first load, pull-to-refresh, append, error
 * handling, stale-response guarding — lives here, as a platform-neutral state
 * machine. `src/pages/applications/index.js` only forwards `setData` into it, so
 * the behaviour is exercised by the Node-side tests instead of only on a device
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §4 — `pages/` owns the SDKWork source
 * page; the platform page is a projection target).
 */

/**
 * Closed-set label maps: adding an `AppKind`/`AppStatus` member to the generated
 * SDK is a compile error here until copy exists for it, which is how a new server
 * enum reaches the UI instead of rendering a raw token.
 */
const STATUS_MESSAGE_KEYS: Record<DeployAppStatus, string> = {
  DRAFT: "applications.list.status.draft",
  READY: "applications.list.status.ready",
  ACTIVE: "applications.list.status.active",
  PAUSED: "applications.list.status.paused",
  ARCHIVED: "applications.list.status.archived",
  FAILED: "applications.list.status.failed",
};

const KIND_MESSAGE_KEYS: Record<DeployAppKind, string> = {
  STATIC_WEB: "applications.list.kind.static-web",
  SPA_WEB: "applications.list.kind.spa-web",
  API_SERVICE: "applications.list.kind.api-service",
  DESKTOP_APP: "applications.list.kind.desktop-app",
  WECHAT_MINIPROGRAM: "applications.list.kind.wechat-miniprogram",
  DOUYIN_MINIPROGRAM: "applications.list.kind.douyin-miniprogram",
  IOS_APP: "applications.list.kind.ios-app",
  ANDROID_APP: "applications.list.kind.android-app",
  HARMONYOS_APP: "applications.list.kind.harmonyos-app",
};

/** A list row: the view model plus the labels the mini program template renders. */
export interface WebserverMpApplicationRow extends WebserverMpApplicationItem {
  readonly kindLabel: string;
  readonly statusLabel: string;
}

export function toWebserverMpApplicationRow(
  item: WebserverMpApplicationItem,
  resolveMessage: (key: string) => string,
): WebserverMpApplicationRow {
  return {
    ...item,
    kindLabel: resolveMessage(KIND_MESSAGE_KEYS[item.kind]),
    statusLabel: resolveMessage(STATUS_MESSAGE_KEYS[item.status]),
  };
}

/**
 * Exactly the shape `Page.setData` receives. Keeping it flat and primitive keeps
 * the native template a pure projection of this object.
 */
export interface WebserverMpApplicationListPageData {
  readonly items: readonly WebserverMpApplicationRow[];
  readonly loading: boolean;
  /** True while an *additional* page is in flight rather than the first one. */
  readonly appending: boolean;
  readonly hasMore: boolean;
  readonly errorMessage: string;
  /** Technical detail of the last failure; never treated as display copy. */
  readonly errorDetail: string;
  readonly page: number;
  readonly totalItems: number;
  readonly emptyMessage: string;
  readonly loadingMessage: string;
  readonly loadingMoreMessage: string;
  readonly retryLabel: string;
  readonly loadMoreLabel: string;
}

export interface CreateWebserverMpApplicationListPageModelOptions {
  readonly service: WebserverMpApplicationsService;
  readonly resolveMessage: (key: string) => string;
  /** Receives the complete next data object; the page forwards it to setData. */
  readonly onDataChange: (data: WebserverMpApplicationListPageData) => void;
  /** Called once the platform gesture behind a load may be released. */
  readonly onSettled?: () => void;
}

export interface WebserverMpApplicationListPageModel {
  getData(): WebserverMpApplicationListPageData;
  /** First page, or a fresh reload after pull-to-refresh. */
  load(): void;
  /** Next page when one exists and no request is already in flight. */
  loadMore(): void;
  status(): WebserverMpScreenStatus;
}

export function createInitialWebserverMpApplicationListPageData(
  resolveMessage: (key: string) => string,
): WebserverMpApplicationListPageData {
  return {
    items: [],
    loading: true,
    appending: false,
    hasMore: false,
    errorMessage: "",
    errorDetail: "",
    page: 1,
    totalItems: 0,
    emptyMessage: resolveMessage("applications.list.empty"),
    loadingMessage: resolveMessage("applications.list.loading"),
    loadingMoreMessage: resolveMessage("applications.list.loadingMore"),
    retryLabel: resolveMessage("applications.list.retry"),
    loadMoreLabel: resolveMessage("applications.list.loadMore"),
  };
}

export function createWebserverMpApplicationListPageModel(
  options: CreateWebserverMpApplicationListPageModelOptions,
): WebserverMpApplicationListPageModel {
  const { onDataChange, onSettled, resolveMessage, service } = options;
  let data = createInitialWebserverMpApplicationListPageData(resolveMessage);
  /**
   * Monotonic request id. A pull-to-refresh issued while a page is in flight must
   * not let the older response land on top of the newer one, and a logout mid
   * request must be able to invalidate it — both reduce to "the response id no
   * longer matches".
   */
  let activeRequest = 0;

  const apply = (patch: Partial<WebserverMpApplicationListPageData>): void => {
    data = { ...data, ...patch };
    onDataChange(data);
  };

  const settle = (): void => {
    onSettled?.();
  };

  const fetchPage = (page: number, append: boolean): void => {
    const requestId = (activeRequest += 1);
    apply(
      append
        ? { appending: true, errorMessage: "", errorDetail: "" }
        : { loading: true, appending: false, errorMessage: "", errorDetail: "" },
    );

    void service
      .loadPage(page)
      .then((result) => {
        if (requestId !== activeRequest) return;
        const items = (append ? [...data.items, ...result.items] : result.items)
          .map((item) => toWebserverMpApplicationRow(item, resolveMessage));
        apply({
          items,
          loading: false,
          appending: false,
          hasMore: result.page.hasMore,
          page: result.page.page,
          totalItems: result.page.totalItems,
        });
      })
      .catch((error: unknown) => {
        if (requestId !== activeRequest) return;
        apply({
          loading: false,
          appending: false,
          errorMessage: resolveMessage("applications.list.error"),
          errorDetail: error instanceof Error ? error.message : String(error),
        });
      })
      .finally(() => {
        if (requestId !== activeRequest) return;
        settle();
      });
  };

  return {
    getData: () => data,
    load: () => {
      apply({ items: [], page: 1, hasMore: false, totalItems: 0 });
      fetchPage(1, false);
    },
    loadMore: () => {
      if (data.loading || data.appending || !data.hasMore) return;
      fetchPage(data.page + 1, true);
    },
    status: () => resolveWebserverMpScreenStatus(
      data.items.length,
      data.loading,
      data.errorMessage,
    ),
  };
}
