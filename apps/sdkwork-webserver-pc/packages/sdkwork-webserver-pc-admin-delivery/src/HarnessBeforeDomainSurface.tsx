import { useWebserverAdminSdk } from "@sdkwork/webserver-pc-admin-core";
import type { ApplicationDomainResponse, RootDomainResponse } from "@sdkwork/webserver-pc-admin-core";
import { translateWebserver, type WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { Button, DataTable, Input, StatusBadge, type DataTableColumn } from "@sdkwork/ui-pc-react";
import { useCallback, useEffect, useState } from "react";

/**
 * The served-domain inventory: which root domains this edge answers for, and
 * which hostname under each one is registered.
 *
 * The rows are not hand-maintained. The gateway reconciles them from the
 * configuration it actually serves (the effective nginx sidecar plus every
 * configured module import) once the database is up, so opening this page after
 * a config change shows what the edge is really serving, not what somebody
 * remembered to type. Manual entries are still possible — a hostname the edge
 * does not serve yet can be registered here — but they are a supplement to the
 * reconciled inventory, never its replacement.
 *
 * Both levels are tenant-level: rows are written with `user_id IS NULL`, which
 * is why this page lives on the operations surface and not in the tenant
 * console.
 *
 * The page renders inside `WebserverAdminSdkProvider`, so the client is
 * injected through the admin-core hook; no transport is constructed here.
 */

const PAGE_SIZE = 100;

export interface ServedDomainAdminSurfaceProps {
  locale: WebserverLocale;
  resource: "domains";
}

interface RootSnapshot {
  items: RootDomainResponse[];
  hasMore: boolean;
  page: number;
}

interface SubdomainSnapshot {
  items: ApplicationDomainResponse[];
  hasMore: boolean;
  page: number;
}

export function ServedDomainAdminSurface({ locale, resource }: ServedDomainAdminSurfaceProps) {
  const client = useWebserverAdminSdk();
  const [roots, setRoots] = useState<RootSnapshot | null>(null);
  const [selectedRoot, setSelectedRoot] = useState<RootDomainResponse | null>(null);
  const [subdomains, setSubdomains] = useState<SubdomainSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [rootPage, setRootPage] = useState(1);
  const [subdomainPage, setSubdomainPage] = useState(1);
  const [rootDraft, setRootDraft] = useState("");
  const [subdomainDraft, setSubdomainDraft] = useState("");

  const t = (key: Parameters<typeof translateWebserver>[1], values?: Record<string, string | number>) =>
    translateWebserver(locale, key, values);

  const loadRoots = useCallback(async () => {
    try {
      const page = await client.domain.rootDomains.list({ page: rootPage, pageSize: PAGE_SIZE });
      // `PageInfo.hasMore` is optional on the wire; absent means no continuation.
      setRoots({ items: page.items, hasMore: page.pageInfo.hasMore === true, page: rootPage });
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [client, rootPage]);

  useEffect(() => {
    void loadRoots();
  }, [loadRoots]);

  const loadSubdomains = useCallback(async (root: RootDomainResponse, page: number) => {
    try {
      const result = await client.domain.rootDomains.subdomains.list(root.id, { page, pageSize: PAGE_SIZE });
      setSubdomains({ items: result.items, hasMore: result.pageInfo.hasMore === true, page });
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [client]);

  useEffect(() => {
    if (!selectedRoot) {
      setSubdomains(null);
      return;
    }
    void loadSubdomains(selectedRoot, subdomainPage);
  }, [selectedRoot, subdomainPage, loadSubdomains]);

  const run = async (operation: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await operation();
      setError(null);
      await loadRoots();
      if (selectedRoot) await loadSubdomains(selectedRoot, subdomainPage);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const createRoot = () =>
    run(async () => {
      const hostname = rootDraft.trim();
      if (!hostname) return;
      await client.domain.rootDomains.create({ hostname }, { idempotencyKey: newIdempotencyKey() });
      setRootDraft("");
    });

  const createSubdomain = () =>
    run(async () => {
      const recordName = subdomainDraft.trim();
      if (!recordName || !selectedRoot) return;
      await client.domain.rootDomains.subdomains.create(
        selectedRoot.id,
        { recordName, sslEnabled: true },
        { idempotencyKey: newIdempotencyKey() },
      );
      setSubdomainDraft("");
    });

  const deleteRoot = (root: RootDomainResponse) =>
    run(async () => {
      if (!window.confirm(t("resource.domains.deleteRootConfirm", { hostname: root.hostname }))) return;
      await client.domain.rootDomains.delete(root.id, { idempotencyKey: newIdempotencyKey() });
      if (selectedRoot?.id === root.id) {
        setSelectedRoot(null);
        setSubdomainPage(1);
      }
    });

  const deleteSubdomain = (domain: ApplicationDomainResponse) =>
    run(async () => {
      if (!window.confirm(t("resource.domains.deleteSubdomainConfirm", { hostname: domain.hostname }))) return;
      await client.domain.delete(domain.id, { idempotencyKey: newIdempotencyKey() });
    });

  const rootColumns: DataTableColumn<RootDomainResponse>[] = [
    { id: "hostname", header: t("resource.domains.rootHostname"), cell: (root) => <code>{root.hostname}</code> },
    { id: "subdomainCount", header: t("resource.domains.subdomainCount"), cell: (root) => root.subdomainCount },
    { id: "verifiedSubdomainCount", header: t("resource.domains.verifiedCount"), cell: (root) => root.verifiedSubdomainCount },
    { id: "httpsSubdomainCount", header: t("resource.domains.httpsCount"), cell: (root) => root.httpsSubdomainCount },
    {
      id: "status",
      header: t("resource.domains.status"),
      cell: (root) => <StatusBadge status={root.status === 1 ? "active" : "disabled"} variant={root.status === 1 ? "success" : "secondary"} />,
    },
    { id: "createdAt", header: t("resource.domains.createdAt"), cell: (root) => formatInstant(root.createdAt, locale) },
    {
      id: "actions",
      header: t("resource.domains.actions"),
      cell: (root) => (
        // `stopPropagation` is load-bearing: the row itself selects the root on
        // click, and without it a delete would also re-select the row it just
        // removed — leaving the subdomain panel pointing at a deleted root.
        <Button
          disabled={busy}
          onClick={(event) => {
            event.stopPropagation();
            void deleteRoot(root);
          }}
          size="sm"
          variant="ghost"
        >
          {t("resource.domains.delete")}
        </Button>
      ),
    },
  ];

  const subdomainColumns: DataTableColumn<ApplicationDomainResponse>[] = [
    { id: "hostname", header: t("resource.domains.subdomainHostname"), cell: (domain) => <code>{domain.hostname}</code> },
    { id: "recordName", header: t("resource.domains.recordName"), cell: (domain) => domain.recordName ?? "-" },
    {
      id: "isVerified",
      header: t("resource.domains.verification"),
      cell: (domain) => (
        <StatusBadge
          status={domain.isVerified ? "verified" : "pending"}
          variant={domain.isVerified ? "success" : "warning"}
        />
      ),
    },
    {
      id: "sslEnabled",
      header: t("resource.domains.ssl"),
      cell: (domain) => (domain.sslEnabled ? domain.sslProvider ?? "enabled" : t("resource.domains.sslOff")),
    },
    { id: "isPrimary", header: t("resource.domains.primary"), cell: (domain) => (domain.isPrimary ? t("resource.domains.yes") : "") },
    { id: "applicationName", header: t("resource.domains.application"), cell: (domain) => domain.applicationName ?? "-" },
    { id: "certificateCount", header: t("resource.domains.certificateCount"), cell: (domain) => domain.certificateCount },
    {
      id: "actions",
      header: t("resource.domains.actions"),
      cell: (domain) => (
        <Button disabled={busy} onClick={() => void deleteSubdomain(domain)} size="sm" variant="ghost">
          {t("resource.domains.delete")}
        </Button>
      ),
    },
  ];

  return (
    <section className="data-surface" data-resource={resource}>
      <header className="resource-toolbar">
        <h2>{t("resource.domains.admin.label")}</h2>
        <span className="toolbar-meta">
          {roots ? t("resource.domains.reconciledHint", { count: roots.items.length }) : t("resource.domains.loading")}
        </span>
      </header>

      <div className="resource-toolbar">
        <Input
          aria-label={t("resource.domains.addRoot")}
          onChange={(event) => setRootDraft(event.target.value)}
          placeholder={t("resource.domains.addRootPlaceholder")}
          value={rootDraft}
        />
        <Button disabled={busy || rootDraft.trim().length === 0} onClick={() => void createRoot()}>
          {t("resource.domains.addRoot")}
        </Button>
        <Button disabled={busy} onClick={() => void loadRoots()} variant="secondary">
          {t("resource.domains.refresh")}
        </Button>
      </div>

      {error ? <p className="bootstrap-state" role="alert">{t("resource.domains.loadFailed")}: {error}</p> : null}

      {roots === null ? (
        <p className="bootstrap-state" role="status">{t("resource.domains.loading")}</p>
      ) : (
        <>
          <DataTable<RootDomainResponse>
            columns={rootColumns}
            density="compact"
            emptyState={<span>{t("resource.domains.noRoots")}</span>}
            getRowId={(root) => root.id}
            onRowClick={(root) => {
              setSelectedRoot(root);
              setSubdomainPage(1);
            }}
            rows={roots.items}
            selectedRowIds={selectedRoot ? [selectedRoot.id] : []}
            stickyHeader
          />
          <Pager
            hasMore={roots.hasMore}
            label={t("resource.domains.page", { page: roots.page })}
            nextLabel={t("resource.domains.next")}
            onNext={() => setRootPage((current) => current + 1)}
            onPrevious={() => setRootPage((current) => Math.max(1, current - 1))}
            previousLabel={t("resource.domains.previous")}
            showPrevious={roots.page > 1}
          />

          {selectedRoot ? (
            <>
              <h3>{t("resource.domains.subdomainsOf", { hostname: selectedRoot.hostname })}</h3>
              <div className="resource-toolbar">
                <Input
                  aria-label={t("resource.domains.addSubdomain")}
                  onChange={(event) => setSubdomainDraft(event.target.value)}
                  placeholder={t("resource.domains.addSubdomainPlaceholder")}
                  value={subdomainDraft}
                />
                <Button disabled={busy || subdomainDraft.trim().length === 0} onClick={() => void createSubdomain()}>
                  {t("resource.domains.addSubdomain")}
                </Button>
              </div>
              <DataTable<ApplicationDomainResponse>
                columns={subdomainColumns}
                density="compact"
                emptyState={<span>{t("resource.domains.noSubdomains")}</span>}
                getRowId={(domain) => domain.id}
                rows={subdomains?.items ?? []}
                stickyHeader
              />
              {subdomains ? (
                <Pager
                  hasMore={subdomains.hasMore}
                  label={t("resource.domains.page", { page: subdomains.page })}
                  nextLabel={t("resource.domains.next")}
                  onNext={() => setSubdomainPage((current) => current + 1)}
                  onPrevious={() => setSubdomainPage((current) => Math.max(1, current - 1))}
                  previousLabel={t("resource.domains.previous")}
                  showPrevious={subdomains.page > 1}
                />
              ) : null}
            </>
          ) : (
            <p className="bootstrap-state">{t("resource.domains.selectRoot")}</p>
          )}
        </>
      )}
    </section>
  );
}

function Pager({
  hasMore,
  label,
  nextLabel,
  onNext,
  onPrevious,
  previousLabel,
  showPrevious,
}: {
  hasMore: boolean;
  label: string;
  nextLabel: string;
  onNext(): void;
  onPrevious(): void;
  previousLabel: string;
  showPrevious: boolean;
}) {
  if (!hasMore && !showPrevious) return null;
  return (
    <div className="resource-toolbar">
      <span className="toolbar-meta">{label}</span>
      {showPrevious ? <Button onClick={onPrevious} size="sm" variant="secondary">{previousLabel}</Button> : null}
      {hasMore ? <Button onClick={onNext} size="sm" variant="secondary">{nextLabel}</Button> : null}
    </div>
  );
}

function formatInstant(instant: string, locale: WebserverLocale): string {
  const parsed = new Date(instant);
  if (Number.isNaN(parsed.getTime())) return instant;
  return parsed.toLocaleString(locale === "zh-CN" ? "zh-CN" : "en-US", { hour12: false });
}

/**
 * Idempotency key for one mutation.
 *
 * Same shape `webserver-config-client.ts` uses: `crypto.randomUUID` is absent —
 * or throws — outside a secure context, and a dev edge reached over plain HTTP at
 * a LAN address is exactly that, so the UUID is assembled from
 * `getRandomValues` rather than assumed.
 */
function newIdempotencyKey(): string {
  const crypto = globalThis.crypto;
  if (typeof crypto?.randomUUID === "function") {
    try {
      return crypto.randomUUID();
    } catch {
      // Non-secure contexts may reject randomUUID; fall through.
    }
  }
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
