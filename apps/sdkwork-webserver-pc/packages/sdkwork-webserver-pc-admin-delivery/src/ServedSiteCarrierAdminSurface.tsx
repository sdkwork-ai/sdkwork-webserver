import { useWebserverAdminSdk } from "@sdkwork/webserver-pc-admin-core";
import type {
  ApplicationResponse,
  CreateApplicationRequest,
  UpdateApplicationRequest,
} from "@sdkwork/webserver-pc-admin-core";
import { hasWebserverPermission, type WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";
import { CirclePause, Pencil, Plus, RefreshCw, Server } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";

import {
  ConfirmDialog,
  DialogBackdrop,
  DialogCloseButton,
  errorText,
  formatInstant,
  newIdempotencyKey,
  translator,
  type MessageKey,
  type Translator,
} from "./AdminSurfaceAtoms.tsx";

/**
 * The Web Server carrier ledger: one row per `webserver_application` that pairs a
 * Deploy application with a site, its domains, certificates, and reverse-proxy
 * configuration.
 *
 * ## Why this page exists, and what it deliberately is not
 *
 * Deploy (`sdkwork-deployments`) owns application identity and publishing. A
 * Web Server application is a *carrier*: it exists only to serve one Deploy
 * application, and since migration `0031` it must name that application
 * (`deployApplicationId`, unique per live row) or the service refuses the create
 * with a validation problem naming the field. Both the service and the storage
 * boundary enforce that rule, so a carrier row cannot be created without it.
 *
 * This page is the operator's way to author exactly that row. It therefore:
 *
 * - requires `deployApplicationId` and says so by name when it is missing,
 *   instead of letting the request reach the service and come back as a 422;
 * - sends no `appKind`. `CreateApplicationRequest` no longer carries the member
 *   (`MIG-2026-0071` deletion list item 6): the carrier row's identity is the
 *   Deploy application, and the application kind is a server-owned default. The
 *   page therefore never declares a second identity next to Deploy's;
 * - offers no publish, release, source-version, or template entry. Those belong
 *   to Deploy, and the page says so. `applications.activate` is not offered
 *   either: it requires a successful `webserver_deployment`, which the retired
 *   publishing writes can no longer produce, so the control would only ever
 *   fail. Pausing is offered because the service accepts it unconditionally.
 *
 * The page reads and writes through the generated backend SDK injected by
 * `WebserverAdminSdkProvider` (`client.application.*`); no transport is built
 * here. The table is the framework `DataTable`, not hand-rolled markup: this is a
 * host-owned resource page, and dense table chrome belongs on the composite.
 */

/** Page size the carrier list requests; the wire has no total, only `hasMore`. */
const PAGE_SIZE = 20;

/**
 * Carrier status is the site row's own lifecycle, not the Deploy app status:
 * `0=draft, 1=active, 2=paused, 3=archived` (`webserver_site.status`).
 */
const STATUS_MESSAGE_KEYS: Readonly<Partial<Record<number, MessageKey>>> = {
  0: "resource.site-carriers.status.draft",
  1: "resource.site-carriers.status.active",
  2: "resource.site-carriers.status.paused",
  3: "resource.site-carriers.status.archived",
};

type CarrierField = "name" | "deployApplicationId" | "runtimeConfig";

interface FieldFailure {
  field: CarrierField;
  message: string;
}

export interface ServedSiteCarrierAdminSurfaceProps {
  locale: WebserverLocale;
  resource: "site-carriers";
  /**
   * The caller's permission scope.
   *
   * The menu entry is gated on `web.applications.read` (the same gate the Domains
   * ledger uses), while every write here is an `applications.create` / `.update` /
   * `.pause` call whose contract requires `web.applications.write`. Hiding the
   * write controls for a read-only operator keeps the page honest: the server
   * would refuse those calls anyway, and a button that only ever 403s reads as a
   * broken page rather than as a missing grant.
   */
  permissionScope: readonly string[];
}

export function ServedSiteCarrierAdminSurface({
  locale,
  permissionScope,
  resource,
}: ServedSiteCarrierAdminSurfaceProps) {
  return (
    <div className="deploy-surface" data-resource={resource}>
      <SiteCarrierLedger locale={locale} permissionScope={permissionScope} />
    </div>
  );
}

function SiteCarrierLedger({
  locale,
  permissionScope,
}: {
  locale: WebserverLocale;
  permissionScope: readonly string[];
}) {
  const client = useWebserverAdminSdk();
  // Memoized per locale: a fresh translator closure on every render would sit in
  // the list effect's dependency array below and refetch the ledger forever.
  const t = useMemo(() => translator(locale), [locale]);
  const canWrite = hasWebserverPermission(permissionScope, "web.applications.write");
  const [carriers, setCarriers] = useState<ApplicationResponse[] | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [page, setPage] = useState(1);
  // Mutations request a re-read by bumping this counter rather than by clearing
  // the rows: `setPage(1)` alone is an `Object.is` bail-out when the ledger
  // already sits on page one, and a cleared list with no effect run is the
  // permanent "Loading" state.
  const [build, setBuild] = useState(0);
  const [createOpen, setCreateOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<ApplicationResponse>();
  const [pauseTarget, setPauseTarget] = useState<ApplicationResponse>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let active = true;
    setBusy(true);
    setError(undefined);
    void client.application
      .list({ page, pageSize: PAGE_SIZE })
      .then((result) => {
        if (!active) return;
        setCarriers(result.items);
        // `PageInfo.hasMore` is optional on the wire; an absent flag means "no
        // continuation", never "unknown".
        setHasMore(result.pageInfo.hasMore === true);
      })
      .catch((cause) => {
        if (active) setError(errorText(cause, t));
      })
      .finally(() => {
        if (active) setBusy(false);
      });
    return () => {
      active = false;
    };
  }, [build, client, page, t]);

  const register = async (body: CreateApplicationRequest) => {
    await client.application.create(body, { idempotencyKey: newIdempotencyKey() });
    setCreateOpen(false);
    setBuild((value) => value + 1);
    setPage(1);
  };

  const save = async (carrier: ApplicationResponse, body: UpdateApplicationRequest) => {
    await client.application.update(carrier.id, body, { idempotencyKey: newIdempotencyKey() });
    setEditTarget(undefined);
    setBuild((value) => value + 1);
  };

  const pause = (carrier: ApplicationResponse) => {
    setBusy(true);
    void client.application
      .pause(carrier.id, { idempotencyKey: newIdempotencyKey() })
      .then(() => {
        setPauseTarget(undefined);
        setBuild((value) => value + 1);
      })
      .catch((cause) => {
        setPauseTarget(undefined);
        setError(errorText(cause, t));
      })
      .finally(() => setBusy(false));
  };

  const columns = useMemo<DataTableColumn<ApplicationResponse>[]>(() => [
    {
      cell: (carrier) => (
        <span className="primary-cell-link">
          <Server size={17} />
          <span>
            <strong>{carrier.name}</strong>
            <small className="cell-subtitle">{carrier.slug}</small>
          </span>
        </span>
      ),
      header: t("resource.site-carriers.name"),
      id: "name",
    },
    {
      // The link is shown exactly as the wire carries it: an int64 decimal
      // string, never coerced through `Number`.
      cell: (carrier) => (carrier.deployApplicationId === undefined ? (
        <small className="cell-subtitle" title={t("resource.site-carriers.unlinkedHint")}>
          {t("resource.site-carriers.unlinked")}
        </small>
      ) : (
        <code>{carrier.deployApplicationId}</code>
      )),
      header: t("resource.site-carriers.deployApplicationId"),
      id: "deployApplicationId",
    },
    {
      cell: (carrier) => {
        const statusKey: MessageKey | undefined = STATUS_MESSAGE_KEYS[carrier.status];
        return (
          <>
            <span className={`status-badge status-${carrier.status}`}>
              {statusKey ? t(statusKey) : String(carrier.status)}
            </span>
            {/* A paused carrier cannot be resumed here: `applications.activate`
                requires a successful `webserver_deployment`, which the retired
                publishing writes can no longer create. Saying whose action it is
                beats offering a control that always fails. */}
            {carrier.status === 2 ? (
              <small className="cell-subtitle">{t("resource.site-carriers.deployOwnsLifecycle")}</small>
            ) : null}
          </>
        );
      },
      header: t("resource.site-carriers.status"),
      id: "status",
    },
    {
      cell: (carrier) => formatInstant(carrier.updatedAt, locale),
      header: t("resource.site-carriers.updatedAt"),
      id: "updatedAt",
    },
  ], [locale, t]);

  return (
    <section className="resource-page domain-page">
      <div className="resource-commandbar">
        <div className="resource-identity">
          <h1>{t("resource.site-carriers.admin.label")}</h1>
        </div>
        <div className="actions">
          <button
            aria-label={t("resource.site-carriers.refresh")}
            className="icon-button"
            disabled={busy}
            onClick={() => {
              setBuild((value) => value + 1);
              setPage(1);
            }}
            title={t("resource.site-carriers.refresh")}
            type="button"
          >
            <RefreshCw size={17} />
          </button>
          {canWrite ? (
            <button className="command-button" onClick={() => setCreateOpen(true)} type="button">
              <Plus size={16} />
              {t("resource.site-carriers.register")}
            </button>
          ) : null}
        </div>
      </div>
      {/* The contract this page lives under, stated where the operator works
          rather than only in a migration note: Deploy owns identity and
          publishing, and this ledger only carries sites for Deploy apps. */}
      <p className="form-hint">{t("resource.site-carriers.notice")}</p>

      {error ? (
        <div className="error-banner" role="alert">
          {error}
        </div>
      ) : null}

      {carriers === null && !error ? (
        <div className="resource-loading" role="status">
          <p>{t("resource.site-carriers.loading")}</p>
        </div>
      ) : (
        <div className="data-surface">
          <DataTable<ApplicationResponse>
            columns={columns}
            density="compact"
            emptyState={(
              <div className="empty-state">{t("resource.site-carriers.empty")}</div>
            )}
            getRowId={(carrier) => carrier.id}
            pagination={{
              hasMore,
              mode: "server",
              onPageChange: (next) => setPage(next),
              page,
            }}
            rowActions={(carrier) => (
              <div className="row-actions">
                {canWrite ? (
                  <>
                    <button
                      aria-label={`${t("resource.site-carriers.edit")} ${carrier.name}`}
                      className="table-action"
                      onClick={() => setEditTarget(carrier)}
                      title={t("resource.site-carriers.edit")}
                      type="button"
                    >
                      <Pencil size={16} />
                    </button>
                    {carrier.status === 1 ? (
                      <button
                        aria-label={`${t("resource.site-carriers.pause")} ${carrier.name}`}
                        className="table-action"
                        onClick={() => setPauseTarget(carrier)}
                        title={t("resource.site-carriers.pause")}
                        type="button"
                      >
                        <CirclePause size={16} />
                      </button>
                    ) : null}
                  </>
                ) : null}
              </div>
            )}
            rowActionsLabel={t("resource.site-carriers.operations")}
            rows={carriers ?? []}
            stickyHeader
          />
        </div>
      )}

      {createOpen ? (
        <CarrierFormDialog
          close={() => setCreateOpen(false)}
          mode="create"
          onCreate={register}
          t={t}
        />
      ) : null}

      {editTarget ? (
        <CarrierFormDialog
          carrier={editTarget}
          close={() => setEditTarget(undefined)}
          mode="edit"
          onUpdate={(body) => save(editTarget, body)}
          t={t}
        />
      ) : null}

      {pauseTarget ? (
        <ConfirmDialog
          close={() => setPauseTarget(undefined)}
          confirmLabel={t("resource.site-carriers.pauseAction")}
          message={t("resource.site-carriers.pauseConfirm", { name: pauseTarget.name })}
          onConfirm={() => pause(pauseTarget)}
          t={t}
          title={t("resource.site-carriers.pauseTitle")}
        />
      ) : null}
    </section>
  );
}

/**
 * Create/edit form.
 *
 * The two modes differ in exactly the places the contract differs: the Deploy
 * link is chosen on create and read-only afterwards (`UpdateApplicationRequest`
 * has no `deployApplicationId`), and neither request carries an `appKind` — the
 * carrier row's kind is the server-owned default. Validation
 * runs here, before the request, because the failure the operator would otherwise
 * see is the server's 422 — this form is what turns that into a sentence naming
 * the field while the operator is still looking at it.
 */
function CarrierFormDialog({
  carrier,
  close,
  mode,
  onCreate,
  onUpdate,
  t,
}: {
  carrier?: ApplicationResponse;
  close(): void;
  mode: "create" | "edit";
  onCreate?(body: CreateApplicationRequest): Promise<void>;
  onUpdate?(body: UpdateApplicationRequest): Promise<void>;
  t: Translator;
}) {
  const creating = mode === "create";
  const [name, setName] = useState(carrier?.name ?? "");
  const [slug, setSlug] = useState(carrier?.slug ?? "");
  const [deployApplicationId, setDeployApplicationId] = useState(carrier?.deployApplicationId ?? "");
  // The stored config is rendered as the JSON the operator would edit. Absent
  // means "no config", which is the empty textarea rather than the string
  // `undefined`.
  const originalRuntime = carrier?.runtimeConfig === undefined
    ? ""
    : JSON.stringify(carrier.runtimeConfig, null, 2);
  const [runtimeConfig, setRuntimeConfig] = useState(originalRuntime);
  const [failure, setFailure] = useState<FieldFailure>();
  const [submitError, setSubmitError] = useState<string>();
  const [busy, setBusy] = useState(false);

  const dirty = creating
    ? name.trim() !== "" || deployApplicationId.trim() !== "" || runtimeConfig.trim() !== ""
    : name.trim() !== (carrier?.name ?? "") || runtimeConfig !== originalRuntime;

  const run = (pending: Promise<void>) => {
    setBusy(true);
    void pending
      .then(() => close())
      .catch((cause) => setSubmitError(errorText(cause, t)))
      .finally(() => setBusy(false));
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSubmitError(undefined);
    const trimmedName = name.trim();
    if (trimmedName === "") {
      setFailure({ field: "name", message: t("resource.site-carriers.nameRequired") });
      return;
    }
    const runtime = parseRuntimeConfig(runtimeConfig);
    if (!runtime.ok) {
      setFailure({ field: "runtimeConfig", message: t("resource.site-carriers.runtimeConfigInvalid") });
      return;
    }
    setFailure(undefined);

    if (creating) {
      const id = deployApplicationId.trim();
      if (id === "") {
        // Names the field and the place the value comes from. This is the 422
        // the page exists to keep the operator from producing.
        setFailure({
          field: "deployApplicationId",
          message: t("resource.site-carriers.deployApplicationIdRequired"),
        });
        return;
      }
      if (!isPositiveInt64(id)) {
        setFailure({
          field: "deployApplicationId",
          message: t("resource.site-carriers.deployApplicationIdInvalid"),
        });
        return;
      }
      const body: CreateApplicationRequest = {
        deployApplicationId: id,
        name: trimmedName,
      };
      const trimmedSlug = slug.trim();
      if (trimmedSlug !== "") body.slug = trimmedSlug;
      if (runtime.value !== undefined) body.runtimeConfig = runtime.value;
      const pending = onCreate?.(body);
      if (pending === undefined) {
        setSubmitError(t("resource.site-carriers.submitUnavailable"));
        return;
      }
      run(pending);
      return;
    }

    if (!dirty) {
      setSubmitError(t("resource.site-carriers.editNeedsAValue"));
      return;
    }
    const body: UpdateApplicationRequest = {};
    if (trimmedName !== (carrier?.name ?? "")) body.name = trimmedName;
    // Only send the config when it changed: `UpdateApplicationRequest` treats an
    // absent member as "leave it alone", which is what an untouched textarea
    // means. A cleared textarea is an explicit empty object, not an omission.
    if (runtimeConfig !== originalRuntime) body.runtimeConfig = runtime.value ?? {};
    const pending = onUpdate?.(body);
    if (pending === undefined) {
      setSubmitError(t("resource.site-carriers.submitUnavailable"));
      return;
    }
    run(pending);
  };

  const invalid = (field: CarrierField): boolean => failure?.field === field;

  return (
    <DialogBackdrop close={close}>
      <form
        aria-labelledby="site-carrier-dialog-title"
        aria-modal="true"
        className="dialog delivery-dialog"
        onSubmit={onSubmit}
        role="dialog"
      >
        <header>
          <h2 id="site-carrier-dialog-title">
            {creating
              ? t("resource.site-carriers.createTitle")
              : t("resource.site-carriers.editTitle")}
          </h2>
          <DialogCloseButton close={close} label={t("resource.site-carriers.cancel")} />
        </header>
        <div className="form-grid single-column">
          <label>
            <span>{t("resource.site-carriers.name")}</span>
            <input
              aria-invalid={invalid("name") || undefined}
              aria-required
              maxLength={100}
              name="name"
              onChange={(event) => setName(event.target.value)}
              type="text"
              value={name}
            />
          </label>
          <label>
            <span>{t("resource.site-carriers.slug")}</span>
            {creating ? (
              <input
                maxLength={100}
                name="slug"
                onChange={(event) => setSlug(event.target.value)}
                type="text"
                value={slug}
              />
            ) : (
              <input name="slug" readOnly type="text" value={carrier?.slug ?? ""} />
            )}
            <small className="form-hint">{t("resource.site-carriers.slugHint")}</small>
          </label>
          <label>
            <span>{t("resource.site-carriers.deployApplicationId")}</span>
            {creating ? (
              <input
                aria-invalid={invalid("deployApplicationId") || undefined}
                aria-required
                inputMode="numeric"
                name="deployApplicationId"
                onChange={(event) => setDeployApplicationId(event.target.value)}
                pattern="[0-9]+"
                type="text"
                value={deployApplicationId}
              />
            ) : (
              // Named as well as read-only: the value still belongs to the
              // `deployApplicationId` field, and a form that dropped the name
              // would hide it from assistive technology and from the payload
              // review.
              <input name="deployApplicationId" readOnly type="text" value={carrier?.deployApplicationId ?? ""} />
            )}
            <small className="form-hint">
              {t("resource.site-carriers.deployApplicationIdHint")}
            </small>
          </label>
          <label>
            <span>{t("resource.site-carriers.runtimeConfig")}</span>
            <textarea
              aria-invalid={invalid("runtimeConfig") || undefined}
              name="runtimeConfig"
              onChange={(event) => setRuntimeConfig(event.target.value)}
              placeholder={t("resource.site-carriers.runtimeConfigPlaceholder")}
              rows={5}
              value={runtimeConfig}
            />
            <small className="form-hint">{t("resource.site-carriers.runtimeConfigHint")}</small>
          </label>
        </div>
        {/* The field failure is announced and also pinned to the control it
            names (`aria-invalid`), so a screen-reader user hears which field the
            sentence is about instead of a floating alert. */}
        {failure ? (
          <div className="error-banner" role="alert">
            {failure.message}
          </div>
        ) : null}
        {submitError ? (
          <div className="error-banner" role="alert">
            {submitError}
          </div>
        ) : null}
        <footer className="dialog-footer">
          <button className="secondary-button" onClick={close} type="button">
            {t("resource.site-carriers.cancel")}
          </button>
          <button className="command-button" disabled={busy} type="submit">
            {creating
              ? t("resource.site-carriers.submitCreate")
              : t("resource.site-carriers.submitEdit")}
          </button>
        </footer>
      </form>
    </DialogBackdrop>
  );
}

/**
 * A `deploy_app.id` is an int64 carried as a decimal string
 * (`x-sdkwork-int64-string`): digits only, and positive. The value is never
 * converted to a number — that is the whole reason the wire keeps it a string.
 */
function isPositiveInt64(value: string): boolean {
  return /^[0-9]+$/.test(value) && !/^0+$/.test(value);
}

/**
 * Parses the runtime-config textarea.
 *
 * Empty means "no config", which the callers express differently: an omitted
 * member on create, and (on edit) only reachable at all when the text changed.
 * Anything that parses to a non-object — an array, a scalar, `null` — is a
 * failure: the wire type is `Record<string, unknown>`.
 */
function parseRuntimeConfig(
  text: string,
): { ok: true; value?: Record<string, unknown> } | { ok: false } {
  const trimmed = text.trim();
  if (trimmed === "") return { ok: true };
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { ok: false };
    }
    return { ok: true, value: parsed as Record<string, unknown> };
  } catch {
    return { ok: false };
  }
}
