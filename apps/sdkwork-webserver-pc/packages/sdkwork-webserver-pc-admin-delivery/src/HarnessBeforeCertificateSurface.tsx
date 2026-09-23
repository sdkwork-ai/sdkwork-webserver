import { useWebserverAdminSdk } from "@sdkwork/webserver-pc-admin-core";
import type { ApplicationDomainResponse, CertificateResponse } from "@sdkwork/webserver-pc-admin-core";
import { translateWebserver, type WebserverLocale } from "@sdkwork/webserver-pc-commons";
import { Button, Checkbox, DataTable, StatusBadge, type DataTableColumn } from "@sdkwork/ui-pc-react";
import { useCallback, useEffect, useState } from "react";

/**
 * TLS certificate lifecycle for the hostnames this edge serves.
 *
 * The certificate plane is the Web Server's own (`webserver_certificate` and its
 * version / operation / binding tables), so an operator can see here which of the
 * served hostnames are covered and what state each certificate is in, without
 * leaving the edge that terminates the TLS in the first place.
 *
 * Issuance needs an identifier set, so the issue form is driven by the served
 * domain inventory (`/domains`) rather than by free text: the operator picks the
 * hostnames, and the ids come from the same rows the Domains page shows. That
 * keeps a certificate from naming a hostname this edge does not answer for.
 *
 * The page renders inside `WebserverAdminSdkProvider`; no transport is built here.
 */

const PAGE_SIZE = 100;

const REVOKE_REASONS = [
  "keyCompromise",
  "affiliationChanged",
  "superseded",
  "cessationOfOperation",
  "privilegeWithdrawn",
] as const;

type RevokeReason = (typeof REVOKE_REASONS)[number];
type CertificateType = 1 | 3;

export interface ServedCertificateAdminSurfaceProps {
  locale: WebserverLocale;
  resource: "certificates";
}

export function ServedCertificateAdminSurface({ locale, resource }: ServedCertificateAdminSurfaceProps) {
  const client = useWebserverAdminSdk();
  const [certificates, setCertificates] = useState<CertificateResponse[] | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [page, setPage] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [issueOpen, setIssueOpen] = useState(false);
  const [identifiers, setIdentifiers] = useState<ApplicationDomainResponse[]>([]);
  const [selectedDomainIds, setSelectedDomainIds] = useState<string[]>([]);
  const [certType, setCertType] = useState<CertificateType>(1);
  const [autoRenew, setAutoRenew] = useState(true);
  const [revokeReason, setRevokeReason] = useState<RevokeReason>("superseded");

  const t = (key: Parameters<typeof translateWebserver>[1], values?: Record<string, string | number>) =>
    translateWebserver(locale, key, values);

  const load = useCallback(async () => {
    try {
      const result = await client.certificate.list({ page, pageSize: PAGE_SIZE });
      setCertificates(result.items);
      // `PageInfo.hasMore` is optional on the wire (`PageInfo` at
      // `types/page-info.ts`); an absent flag means "no continuation", never
      // "unknown", so it folds to `false` rather than leaking `undefined` into
      // the pager.
      setHasMore(result.pageInfo.hasMore === true);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [client, page]);

  useEffect(() => {
    void load();
  }, [load]);

  const openIssue = async () => {
    setIssueOpen(true);
    if (identifiers.length > 0) return;
    try {
      const served = await client.domain.list({ page: 1, pageSize: PAGE_SIZE });
      setIdentifiers(served.items);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const run = async (operation: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await operation();
      setError(null);
      await load();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const issue = () =>
    run(async () => {
      if (selectedDomainIds.length === 0) return;
      await client.certificate.issue(
        { domainIds: selectedDomainIds, certType, autoRenew },
        { idempotencyKey: newIdempotencyKey() },
      );
      setSelectedDomainIds([]);
      setIssueOpen(false);
    });

  const renew = (certificate: CertificateResponse) =>
    run(async () => {
      await client.certificate.renew(certificate.id, { idempotencyKey: newIdempotencyKey() });
    });

  const revoke = (certificate: CertificateResponse) =>
    run(async () => {
      if (!window.confirm(t("resource.certificates.revokeConfirm", { name: certificate.certName }))) return;
      await client.certificate.revoke(
        certificate.id,
        { reason: revokeReason },
        { idempotencyKey: newIdempotencyKey() },
      );
    });

  const remove = (certificate: CertificateResponse) =>
    run(async () => {
      if (!window.confirm(t("resource.certificates.deleteConfirm", { name: certificate.certName }))) return;
      await client.certificate.delete(certificate.id, { idempotencyKey: newIdempotencyKey() });
    });

  const toggleAutoRenew = (certificate: CertificateResponse) =>
    run(async () => {
      await client.certificate.update(
        certificate.id,
        { autoRenew: certificate.autoRenew !== true },
        { idempotencyKey: newIdempotencyKey() },
      );
    });

  // Built per render rather than memoized: every cell closes over the current
  // `busy` / `revokeReason`, and the translation helper is itself render-scoped,
  // so a memo would only be able to hold a stale copy of both.
  const columns: DataTableColumn<CertificateResponse>[] = [
      { id: "certName", header: t("resource.certificates.certName"), cell: (certificate) => certificate.certName },
      {
        id: "identifiers",
        header: t("resource.certificates.identifiers"),
        cell: (certificate) => (
          <span>
            {certificate.identifiers.length > 0
              ? certificate.identifiers.map((identifier) => identifier.hostname).join(", ")
              : "-"}
          </span>
        ),
      },
      {
        id: "status",
        header: t("resource.certificates.status"),
        cell: (certificate) => (
          <StatusBadge status={certificate.status} variant={certificateStatusVariant(certificate.status)} />
        ),
      },
      { id: "issuer", header: t("resource.certificates.issuer"), cell: (certificate) => certificate.issuer ?? "-" },
      { id: "keyAlgorithm", header: t("resource.certificates.keyAlgorithm"), cell: (certificate) => certificate.keyAlgorithm },
      { id: "notAfter", header: t("resource.certificates.notAfter"), cell: (certificate) => formatInstant(certificate.notAfter, locale) },
      {
        id: "autoRenew",
        header: t("resource.certificates.autoRenew"),
        cell: (certificate) =>
          certificate.autoRenew === true ? t("resource.domains.yes") : t("resource.domains.no"),
      },
      {
        id: "actions",
        header: t("resource.domains.actions"),
        cell: (certificate) => (
          <span>
            <Button disabled={busy} onClick={() => void renew(certificate)} size="sm" variant="ghost">
              {t("resource.certificates.renew")}
            </Button>
            <Button disabled={busy} onClick={() => void toggleAutoRenew(certificate)} size="sm" variant="ghost">
              {certificate.autoRenew === true ? t("resource.certificates.autoRenewOff") : t("resource.certificates.autoRenewOn")}
            </Button>
            <Button disabled={busy} onClick={() => void revoke(certificate)} size="sm" variant="ghost">
              {t("resource.certificates.revoke")}
            </Button>
            <Button disabled={busy} onClick={() => void remove(certificate)} size="sm" variant="ghost">
              {t("resource.domains.delete")}
            </Button>
          </span>
        ),
      },
  ];

  return (
    <section className="data-surface" data-resource={resource}>
      <header className="resource-toolbar">
        <h2>{t("resource.certificates.admin.label")}</h2>
        <span className="toolbar-meta">
          {certificates ? t("resource.certificates.countHint", { count: certificates.length }) : t("resource.certificates.loading")}
        </span>
      </header>

      <div className="resource-toolbar">
        <Button disabled={busy} onClick={() => void openIssue()}>
          {t("resource.certificates.issue")}
        </Button>
        <Button disabled={busy} onClick={() => void load()} variant="secondary">
          {t("resource.domains.refresh")}
        </Button>
        <label>
          {t("resource.certificates.revokeReason")}
          <select
            aria-label={t("resource.certificates.revokeReason")}
            onChange={(event) => setRevokeReason(event.target.value as RevokeReason)}
            value={revokeReason}
          >
            {REVOKE_REASONS.map((reason) => (
              <option key={reason} value={reason}>
                {t(`resource.certificates.reason.${reason}` as Parameters<typeof translateWebserver>[1])}
              </option>
            ))}
          </select>
        </label>
      </div>

      {issueOpen ? (
        <div className="resource-toolbar">
          <label>
            {t("resource.certificates.selectIdentifiers")}
            <select
              aria-label={t("resource.certificates.selectIdentifiers")}
              multiple
              onChange={(event) => {
                const next = Array.from(event.target.selectedOptions).map((option) => option.value);
                setSelectedDomainIds(next);
              }}
              size={Math.min(8, Math.max(3, identifiers.length))}
              value={selectedDomainIds}
            >
              {identifiers.map((domain) => (
                <option key={domain.id} value={domain.id}>
                  {domain.hostname}
                </option>
              ))}
            </select>
          </label>
          <label>
            <input
              checked={certType === 1}
              name="certType"
              onChange={() => setCertType(1)}
              type="radio"
            />
            {t("resource.certificates.letsEncrypt")}
          </label>
          <label>
            <input
              checked={certType === 3}
              name="certType"
              onChange={() => setCertType(3)}
              type="radio"
            />
            {t("resource.certificates.selfSigned")}
          </label>
          <label>
            <Checkbox checked={autoRenew} onCheckedChange={(checked) => setAutoRenew(checked === true)} />
            {t("resource.certificates.autoRenew")}
          </label>
          <Button disabled={busy || selectedDomainIds.length === 0} onClick={() => void issue()}>
            {t("resource.certificates.issueSubmit")}
          </Button>
          <Button disabled={busy} onClick={() => setIssueOpen(false)} variant="secondary">
            {t("resource.certificates.cancel")}
          </Button>
        </div>
      ) : null}

      {error ? <p className="bootstrap-state" role="alert">{t("resource.certificates.loadFailed")}: {error}</p> : null}

      {certificates === null ? (
        <p className="bootstrap-state" role="status">{t("resource.certificates.loading")}</p>
      ) : (
        <>
          <DataTable<CertificateResponse>
            columns={columns}
            density="compact"
            emptyState={<span>{t("resource.certificates.noCertificates")}</span>}
            getRowId={(certificate) => certificate.id}
            rows={certificates}
            stickyHeader
          />
          {hasMore || page > 1 ? (
            <div className="resource-toolbar">
              <span className="toolbar-meta">{t("resource.domains.page", { page })}</span>
              {page > 1 ? (
                <Button onClick={() => setPage((current) => Math.max(1, current - 1))} size="sm" variant="secondary">
                  {t("resource.domains.previous")}
                </Button>
              ) : null}
              {hasMore ? (
                <Button onClick={() => setPage((current) => current + 1)} size="sm" variant="secondary">
                  {t("resource.domains.next")}
                </Button>
              ) : null}
            </div>
          ) : null}
        </>
      )}
    </section>
  );
}

function certificateStatusVariant(status: CertificateResponse["status"]): "success" | "warning" | "danger" | "secondary" {
  if (status === "ISSUED") return "success";
  if (status === "PENDING") return "warning";
  if (status === "FAILED" || status === "EXPIRED" || status === "REVOKED") return "danger";
  return "secondary";
}

function formatInstant(instant: string | undefined, locale: WebserverLocale): string {
  if (!instant) return "-";
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
