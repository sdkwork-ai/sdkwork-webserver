import { ScreenFrame, StateBlock, cx } from "@sdkwork/webserver-h5-commons";
import type { DeployAppKind, DeployAppResponse, DeployAppStatus } from "@sdkwork/webserver-h5-core/sdk";

import { useApplicationsList, type ApplicationsListReader } from "./useApplicationsList.ts";

export interface ApplicationsListViewProps {
  /**
   * Injected by the application root. The feature never constructs an SDK
   * client, so the same screen runs against a real deployments app API, a
   * standalone gateway, or a deterministic double without a code change.
   */
  readonly deployClient: ApplicationsListReader;
  /** Root-owned catalog resolver; the package carries no locale strategy. */
  readonly resolveMessage: (key: string) => string;
  /** Root-owned navigation: the feature reports a selection, it does not route. */
  readonly onOpenApplication?: (application: DeployAppResponse) => void;
}

/** `AppStatus` is a closed enum, so an unmapped value is a compile error. */
const STATUS_MESSAGE_KEYS: Readonly<Record<DeployAppStatus, string>> = {
  ACTIVE: "applications.list.status.active",
  ARCHIVED: "applications.list.status.archived",
  DRAFT: "applications.list.status.draft",
  FAILED: "applications.list.status.failed",
  PAUSED: "applications.list.status.paused",
  READY: "applications.list.status.ready",
};

/** `AppKind` is a closed enum, so an unmapped value is a compile error. */
const KIND_MESSAGE_KEYS: Readonly<Record<DeployAppKind, string>> = {
  ANDROID_APP: "applications.list.kind.android-app",
  API_SERVICE: "applications.list.kind.api-service",
  DOUYIN_MINIPROGRAM: "applications.list.kind.douyin-miniprogram",
  HARMONYOS_APP: "applications.list.kind.harmonyos-app",
  IOS_APP: "applications.list.kind.ios-app",
  SPA_WEB: "applications.list.kind.spa-web",
  STATIC_WEB: "applications.list.kind.static-web",
  WECHAT_MINIPROGRAM: "applications.list.kind.wechat-miniprogram",
};

export function ApplicationsListView({
  deployClient,
  onOpenApplication,
  resolveMessage,
}: ApplicationsListViewProps) {
  const { appending, failureDetail, items, loadMore, page, reload, status } =
    useApplicationsList(deployClient);

  const renderRow = (application: DeployAppResponse) => {
    const rowBody = (
      <>
        <div className={cx("h5-app-card__heading")}>
          <span className={cx("h5-app-card__name")}>{application.name}</span>
          <span
            className={cx(
              "h5-app-card__status",
              `h5-app-card__status--${application.appStatus.toLowerCase()}`,
            )}
          >
            {resolveMessage(STATUS_MESSAGE_KEYS[application.appStatus])}
          </span>
        </div>
        <p className={cx("h5-app-card__slug")}>{application.slug}</p>
        <dl className={cx("h5-app-card__meta")}>
          <div>
            <dt>{resolveMessage("applications.list.row.kind")}</dt>
            <dd>{resolveMessage(KIND_MESSAGE_KEYS[application.appKind])}</dd>
          </div>
          <div>
            <dt>{resolveMessage("applications.list.row.environment")}</dt>
            <dd>{application.defaultEnvironment}</dd>
          </div>
          <div>
            <dt>{resolveMessage("applications.list.row.release")}</dt>
            <dd>{application.latestReleaseTag ?? application.version}</dd>
          </div>
          <div>
            <dt>{resolveMessage("applications.list.row.platformTargets")}</dt>
            <dd>{application.platformTargetCount ?? "0"}</dd>
          </div>
        </dl>
      </>
    );

    return (
      <li key={application.id} className={cx("h5-app-card")}>
        {onOpenApplication ? (
          <button
            type="button"
            className={cx("h5-app-card__button")}
            onClick={() => onOpenApplication(application)}
          >
            {rowBody}
          </button>
        ) : (
          rowBody
        )}
      </li>
    );
  };

  return (
    <ScreenFrame
      title={resolveMessage("applications.list.title")}
      description={resolveMessage("applications.list.description")}
    >
      {status === "loading" && items.length === 0 ? (
        <StateBlock tone="loading" message={resolveMessage("applications.list.loading")} />
      ) : null}

      {status === "error" ? (
        <StateBlock
          tone="error"
          message={resolveMessage("applications.list.error")}
          onRetry={reload}
          retryLabel={resolveMessage("applications.list.retry")}
        />
      ) : null}

      {status === "ready" && items.length === 0 ? (
        <StateBlock tone="empty" message={resolveMessage("applications.list.empty")} />
      ) : null}

      {items.length > 0 ? (
        <ul className={cx("h5-app-list")}>{items.map(renderRow)}</ul>
      ) : null}

      {status === "error" && failureDetail ? (
        <p className={cx("h5-app-list__detail")}>{failureDetail}</p>
      ) : null}

      {page.hasMore && status !== "error" ? (
        <button
          type="button"
          className={cx("h5-app-list__more")}
          disabled={appending}
          onClick={loadMore}
        >
          {resolveMessage(
            appending ? "applications.list.loadingMore" : "applications.list.loadMore",
          )}
        </button>
      ) : null}
    </ScreenFrame>
  );
}
