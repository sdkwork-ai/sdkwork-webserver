import {
  resolveWebserverHarmonyScreenStatus,
  type WebserverHarmonyScreenStatus,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-commons';

import type {
  WebserverHarmonyApplicationItem,
  WebserverHarmonyApplicationRow,
  WebserverHarmonyApplicationsScreenModel,
} from '../../models/ApplicationModels';
import { toWebserverHarmonyApplicationRow } from '../../models/ApplicationRecordMapping';
import {
  WEBSERVER_HARMONY_APPLICATION_KIND_LABELS,
  WEBSERVER_HARMONY_APPLICATION_STATUS_LABELS,
} from '../../i18n/ApplicationsMessages';
import type { WebserverHarmonyApplicationsService } from '../../services/ApplicationsService';

/**
 * Applications catalog view model.
 *
 * Everything the screen does — first load, pull-to-refresh, append, error
 * handling, stale-response guarding — lives here, as a platform-neutral state
 * machine that the ArkUI page only renders. The record->row mapping itself is a
 * pure injected function in `models/ApplicationRecordMapping.ets`, which the
 * repository's Node-side contract test executes directly.
 */

/** Key rendered for a token no closed-set label table covers. */
const UNKNOWN_LABEL_MESSAGE_KEY: string = 'applications.list.row.unknown';

export interface CreateWebserverHarmonyApplicationsCatalogViewModelOptions {
  readonly service: WebserverHarmonyApplicationsService;
  readonly resolveMessage: (key: string) => string;
  /** Receives the complete next state object. */
  readonly onStateChange?: (state: WebserverHarmonyApplicationsScreenModel) => void;
  /** Called once the platform gesture behind a load may be released. */
  readonly onSettled?: () => void;
  readonly pageSize?: number;
}

export interface WebserverHarmonyApplicationsCatalogViewModel {
  state(): WebserverHarmonyApplicationsScreenModel;
  /** First page, or a fresh reload after pull-to-refresh. */
  refresh(): Promise<void>;
  /** Next page when one exists and no request is already in flight. */
  loadMore(): Promise<void>;
  status(): WebserverHarmonyScreenStatus;
}

function createInitialState(
  resolveMessage: (key: string) => string,
): WebserverHarmonyApplicationsScreenModel {
  return {
    rows: [],
    loading: true,
    appending: false,
    hasMore: false,
    errorMessage: '',
    errorDetail: '',
    page: 1,
    totalItems: 0,
    emptyMessage: resolveMessage('applications.list.empty'),
    loadingMessage: resolveMessage('applications.list.loading'),
    loadingMoreMessage: resolveMessage('applications.list.loadingMore'),
    retryLabel: resolveMessage('applications.list.retry'),
    loadMoreLabel: resolveMessage('applications.list.loadMore'),
  };
}

export function createWebserverHarmonyApplicationsCatalogViewModel(
  options: CreateWebserverHarmonyApplicationsCatalogViewModelOptions,
): WebserverHarmonyApplicationsCatalogViewModel {
  const resolveMessage = options.resolveMessage;
  const service = options.service;
  const pageSize = options.pageSize;
  let model: WebserverHarmonyApplicationsScreenModel = createInitialState(resolveMessage);
  /**
   * Monotonic request id. A refresh issued while a page is in flight must not let
   * the older response land on top of the newer one, and a logout mid-request must
   * be able to invalidate it — both reduce to "the response id no longer matches".
   */
  let activeRequest = 0;

  const apply = (patch: Partial<WebserverHarmonyApplicationsScreenModel>): void => {
    model = { ...model, ...patch };
    options.onStateChange?.(model);
  };

  const fetchPage = async (page: number, append: boolean): Promise<void> => {
    activeRequest += 1;
    const requestId: number = activeRequest;
    apply(
      append
        ? { appending: true, errorMessage: '', errorDetail: '' }
        : { loading: true, appending: false, errorMessage: '', errorDetail: '' },
    );
    try {
      const result = await service.loadPage(page, pageSize);
      if (requestId !== activeRequest) {
        return;
      }
      const source: WebserverHarmonyApplicationItem[] = append
        ? model.rows.concat(result.items)
        : result.items;
      const rows: WebserverHarmonyApplicationRow[] = source.map(
        (item: WebserverHarmonyApplicationItem) =>
          toWebserverHarmonyApplicationRow(
            item,
            WEBSERVER_HARMONY_APPLICATION_KIND_LABELS,
            WEBSERVER_HARMONY_APPLICATION_STATUS_LABELS,
            resolveMessage,
            UNKNOWN_LABEL_MESSAGE_KEY,
          ),
      );
      apply({
        rows,
        loading: false,
        appending: false,
        hasMore: result.page.hasMore,
        page: result.page.page,
        totalItems: result.page.totalItems,
      });
    } catch (error) {
      if (requestId !== activeRequest) {
        return;
      }
      apply({
        loading: false,
        appending: false,
        errorMessage: resolveMessage('applications.list.error'),
        errorDetail: error instanceof Error ? error.message : String(error),
      });
    } finally {
      if (requestId === activeRequest) {
        options.onSettled?.();
      }
    }
  };

  return {
    state: () => model,
    refresh: async (): Promise<void> => {
      apply({ rows: [], page: 1, hasMore: false, totalItems: 0 });
      await fetchPage(1, false);
    },
    loadMore: async (): Promise<void> => {
      if (model.loading || model.appending || !model.hasMore) {
        return;
      }
      await fetchPage(model.page + 1, true);
    },
    status: () => resolveWebserverHarmonyScreenStatus(
      model.rows.length,
      model.loading,
      model.errorMessage,
    ),
  };
}
