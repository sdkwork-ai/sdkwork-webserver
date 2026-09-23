// @vitest-environment jsdom

import { WebserverAdminSdkProvider, type WebserverAdminSdkClient } from "@sdkwork/webserver-pc-admin-core";
import { ServedCertificateAdminSurface, ServedDomainAdminSurface } from "@sdkwork/webserver-pc-admin-delivery";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

/**
 * The Domains page is the operator-visible half of the startup reconcile: the
 * gateway folds the hostnames it actually serves into `webserver_root_domain` /
 * `webserver_domain`, and this page is where that inventory is read.
 *
 * Four properties are pinned, each one a way the page could silently diverge
 * from the data it claims to show:
 *
 * 1. The root list renders the reconcile counters (`subdomainCount`,
 *    `verifiedSubdomainCount`, `httpsSubdomainCount`), not just the hostname —
 *    an operator has to be able to see that a root was created *and* that its
 *    hostnames are registered under it.
 * 2. Opening a root navigates to the root-scoped route, which is the only thing
 *    that expresses the parent/child relation; a flat `/domains` list would show
 *    the hostnames with no root to read them under.
 * 3. Deleting a root asks for confirmation in the page's own dialog and, once
 *    confirmed, does not also open the root it just removed.
 * 4. Both mutations carry an idempotency key the server can dedupe on.
 *
 * ## Why the router is part of the fixture
 *
 * The page is a two-level ledger whose levels are routes, exactly as the console
 * page is. It is therefore mounted the way the host mounts it —
 * `WebserverWorkspace` puts every entry at `<surface>/<resource>/*` — so the test
 * exercises the same nesting production does. Rendering the surface bare would
 * leave the relative links the page emits resolving against the router root,
 * which is not a state the application can reach.
 *
 * Matchers are plain DOM assertions: this app's vitest setup does not load
 * jest-dom.
 */

const ROOT_ID = "360974393209286656";

const ROOTS = [
  {
    activeDeploymentCount: "0",
    boundSubdomainCount: "0",
    createdAt: "2026-09-23T02:23:21.936919Z",
    hostname: "sdkwork.com",
    httpsSubdomainCount: "3",
    id: ROOT_ID,
    status: 1,
    subdomainCount: "3",
    updatedAt: "2026-09-23T02:23:21.936919Z",
    verifiedSubdomainCount: "3",
  },
  {
    activeDeploymentCount: "0",
    boundSubdomainCount: "0",
    createdAt: "2026-09-23T02:23:21.936919Z",
    hostname: "zowalk.com",
    httpsSubdomainCount: "0",
    id: "360974393242841088",
    status: 1,
    subdomainCount: "0",
    updatedAt: "2026-09-23T02:23:21.936919Z",
    verifiedSubdomainCount: "0",
  },
];

const SUBDOMAINS = [
  {
    certificateCount: "0",
    createdAt: "2026-09-23T02:23:21.936919Z",
    hostname: "server-dev.sdkwork.com",
    id: "d-server-dev",
    isPrimary: false,
    isVerified: true,
    recordName: "server-dev",
    rootDomainId: ROOT_ID,
    sslEnabled: true,
    sslProvider: "letsencrypt",
    status: 1,
  },
  {
    certificateCount: "0",
    createdAt: "2026-09-23T02:23:21.936919Z",
    hostname: "server-admin-dev.sdkwork.com",
    id: "d-server-admin-dev",
    isPrimary: false,
    isVerified: true,
    recordName: "server-admin-dev",
    rootDomainId: ROOT_ID,
    sslEnabled: true,
    sslProvider: "letsencrypt",
    status: 1,
  },
];

const CERTIFICATES = [
  {
    autoRenew: true,
    certName: "sdkwork-served",
    createdAt: "2026-09-23T02:30:00Z",
    id: "cert-1",
    identifiers: [
      { domainId: "d-server-dev", hostname: "server-dev.sdkwork.com", identifierType: "EXACT", position: 0 },
      { domainId: "d-server-admin-dev", hostname: "server-admin-dev.sdkwork.com", identifierType: "EXACT", position: 1 },
    ],
    issuer: "Pebble Intermediate CA",
    keyAlgorithm: "ECDSA",
    notAfter: "2026-12-22T02:30:00Z",
    status: "ISSUED",
  },
];

interface Stubs {
  client: WebserverAdminSdkClient;
  createRootDomain: ReturnType<typeof vi.fn>;
  createSubdomain: ReturnType<typeof vi.fn>;
  deleteRootDomain: ReturnType<typeof vi.fn>;
  deleteDomain: ReturnType<typeof vi.fn>;
  listSubdomains: ReturnType<typeof vi.fn>;
  retrieveRootDomain: ReturnType<typeof vi.fn>;
}

function stubClient(): Stubs {
  const page = (items: readonly unknown[]) => ({
    items,
    pageInfo: { hasMore: false, mode: "offset", page: 1, pageSize: 50 },
  });
  const listSubdomains = vi.fn().mockResolvedValue(page(SUBDOMAINS));
  const createRootDomain = vi.fn().mockResolvedValue(ROOTS[0]);
  const createSubdomain = vi.fn().mockResolvedValue(SUBDOMAINS[0]);
  const deleteRootDomain = vi.fn().mockResolvedValue(undefined);
  const deleteDomain = vi.fn().mockResolvedValue(undefined);
  const retrieveRootDomain = vi.fn().mockResolvedValue(ROOTS[0]);
  const client = {
    certificate: { list: vi.fn().mockResolvedValue(page(CERTIFICATES)) },
    domain: {
      delete: deleteDomain,
      list: vi.fn().mockResolvedValue(page(SUBDOMAINS)),
      rootDomains: {
        create: createRootDomain,
        delete: deleteRootDomain,
        list: vi.fn().mockResolvedValue(page(ROOTS)),
        retrieve: retrieveRootDomain,
        subdomains: { create: createSubdomain, list: listSubdomains },
      },
    },
  } as unknown as WebserverAdminSdkClient;
  return { client, createRootDomain, createSubdomain, deleteDomain, deleteRootDomain, listSubdomains, retrieveRootDomain };
}

/** Mounts a surface the way `WebserverWorkspace` does: at `<surface>/<resource>/*`. */
function renderInProvider(ui: ReactNode, client: WebserverAdminSdkClient, entry: string) {
  return render(
    <WebserverAdminSdkProvider client={client}>
      <MemoryRouter initialEntries={[entry]}>
        <Routes>
          <Route element={ui} path={`${entry}/*`} />
        </Routes>
      </MemoryRouter>
    </WebserverAdminSdkProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("served domain admin surface", () => {
  it("renders each reconciled root domain with its subdomain counters", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    expect(await screen.findByText("sdkwork.com")).toBeTruthy();
    expect(screen.getByText("zowalk.com")).toBeTruthy();
    // The counters are the evidence that the reconcile actually registered the
    // hostnames under the root, not just the root itself.
    const row = screen.getByText("sdkwork.com").closest("tr");
    expect(row?.textContent).toContain("3");
  });

  it("reads subdomains from the root-scoped route once a root is opened", async () => {
    const { client, listSubdomains, retrieveRootDomain } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    // Nothing is read under a root until one is opened.
    expect(listSubdomains).not.toHaveBeenCalled();

    fireEvent.click(await screen.findByText("sdkwork.com"));

    expect(await screen.findByText("server-dev.sdkwork.com")).toBeTruthy();
    expect(screen.getByText("server-admin-dev.sdkwork.com")).toBeTruthy();
    expect(retrieveRootDomain).toHaveBeenCalledWith(ROOT_ID);
    expect(listSubdomains).toHaveBeenCalledWith(ROOT_ID, { page: 1, pageSize: 50 });
  });

  it("deletes a root only after its own confirmation, without opening that root", async () => {
    const { client, deleteRootDomain, listSubdomains } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    const rootRow = (await screen.findByText("sdkwork.com")).closest("tr");
    const deleteButton = rootRow?.querySelector("button");
    expect(deleteButton).not.toBeNull();

    fireEvent.click(deleteButton as HTMLElement);

    // The action opens the page's own dialog rather than window.confirm, so the
    // destructive step is a second, explicit click.
    expect(deleteRootDomain).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    expect(deleteRootDomain).toHaveBeenCalledWith(
      ROOT_ID,
      expect.objectContaining({ idempotencyKey: expect.any(String) }),
    );
    // ...and the delete did not navigate into the root that was just removed.
    expect(listSubdomains).not.toHaveBeenCalled();
  });

  it("registers a new root domain with an idempotency key the server can dedupe on", async () => {
    const { client, createRootDomain } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    fireEvent.click(await screen.findByRole("button", { name: "Define root domain" }));
    fireEvent.change(screen.getByLabelText("Root domain"), { target: { value: "example.com" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(createRootDomain).toHaveBeenCalledWith(
      { hostname: "example.com" },
      { idempotencyKey: expect.stringMatching(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u) },
    );
  });

  it("registers a subdomain under the opened root using its relative record name", async () => {
    const { client, createSubdomain } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    fireEvent.click(await screen.findByText("sdkwork.com"));
    await screen.findByText("server-dev.sdkwork.com");

    fireEvent.click(screen.getByRole("button", { name: "Add subdomain" }));
    fireEvent.change(screen.getByLabelText("Record name"), { target: { value: "api" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(createSubdomain).toHaveBeenCalledWith(
      ROOT_ID,
      { recordName: "api", sslEnabled: true },
      { idempotencyKey: expect.any(String) },
    );
  });
});

describe("served certificate admin surface", () => {
  it("lists a certificate with every hostname identifier it covers", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    expect(await screen.findByText("sdkwork-served")).toBeTruthy();
    const row = screen.getByText("sdkwork-served").closest("tr");
    expect(row?.textContent).toContain("server-dev.sdkwork.com");
    expect(row?.textContent).toContain("server-admin-dev.sdkwork.com");
    expect(row?.textContent).toContain("Issued");
  });

  it("offers the certificate lifecycle actions on each row", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    const row = (await screen.findByText("sdkwork-served")).closest("tr");
    const labels = Array.from(row?.querySelectorAll("button") ?? []).map((button) =>
      button.getAttribute("aria-label"),
    );
    expect(labels).toEqual([
      "Renew sdkwork-served",
      "Turn auto renew off sdkwork-served",
      "Revoke sdkwork-served",
      "Delete sdkwork-served",
    ]);
  });
});
