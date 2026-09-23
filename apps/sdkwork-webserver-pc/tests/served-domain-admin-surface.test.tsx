// @vitest-environment jsdom

import { WebserverAdminSdkProvider, type WebserverAdminSdkClient } from "@sdkwork/webserver-pc-admin-core";
import { ServedCertificateAdminSurface, ServedDomainAdminSurface } from "@sdkwork/webserver-pc-admin-delivery";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

/**
 * The Domains page is the operator-visible half of the startup reconcile: the
 * gateway folds the hostnames it actually serves into `webserver_root_domain` /
 * `webserver_domain`, and this page is where that inventory is read.
 *
 * Three properties are pinned, each one a way the page could silently diverge
 * from the data it claims to show:
 *
 * 1. The root list renders the reconcile counters (`subdomainCount`,
 *    `verifiedSubdomainCount`, `httpsSubdomainCount`), not just the hostname —
 *    an operator has to be able to see that a root was created *and* that its
 *    hostnames are registered under it.
 * 2. Opening a root reads its subdomains from the root-scoped route, which is the
 *    only thing that expresses the parent/child relation; a flat `/domains` list
 *    would show the hostnames with no root to read them under.
 * 3. Deleting a root from its row does not also select that row. The row click
 *    and the row action share a DOM ancestor, and selection drives the subdomain
 *    panel — so a bubbled delete click would leave the panel bound to a root that
 *    no longer exists.
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
    isPrimary: true,
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
}

function stubClient(): Stubs {
  const page = (items: readonly unknown[]) => ({
    items,
    pageInfo: { hasMore: false, mode: "offset", page: 1, pageSize: 100 },
  });
  const listSubdomains = vi.fn().mockResolvedValue(page(SUBDOMAINS));
  const createRootDomain = vi.fn().mockResolvedValue(ROOTS[0]);
  const createSubdomain = vi.fn().mockResolvedValue(SUBDOMAINS[0]);
  const deleteRootDomain = vi.fn().mockResolvedValue(undefined);
  const deleteDomain = vi.fn().mockResolvedValue(undefined);
  const client = {
    certificate: { list: vi.fn().mockResolvedValue(page(CERTIFICATES)) },
    domain: {
      delete: deleteDomain,
      list: vi.fn().mockResolvedValue(page(SUBDOMAINS)),
      rootDomains: {
        create: createRootDomain,
        delete: deleteRootDomain,
        list: vi.fn().mockResolvedValue(page(ROOTS)),
        subdomains: { create: createSubdomain, list: listSubdomains },
      },
    },
  } as unknown as WebserverAdminSdkClient;
  return { client, createRootDomain, createSubdomain, deleteDomain, deleteRootDomain, listSubdomains };
}

function renderInProvider(ui: ReactNode, client: WebserverAdminSdkClient) {
  return render(<WebserverAdminSdkProvider client={client}>{ui}</WebserverAdminSdkProvider>);
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("served domain admin surface", () => {
  it("renders each reconciled root domain with its subdomain counters", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client);

    expect(await screen.findByText("sdkwork.com")).toBeTruthy();
    expect(screen.getByText("zowalk.com")).toBeTruthy();
    // The counters are the evidence that the reconcile actually registered the
    // hostnames under the root, not just the root itself.
    const row = screen.getByText("sdkwork.com").closest("tr");
    expect(row?.textContent).toContain("3");
  });

  it("reads subdomains from the root-scoped route once a root is opened", async () => {
    const { client, listSubdomains } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client);

    const rootRow = (await screen.findByText("sdkwork.com")).closest("tr");
    expect(rootRow).not.toBeNull();
    // Nothing is read under a root until one is opened.
    expect(listSubdomains).not.toHaveBeenCalled();

    fireEvent.click(rootRow as HTMLElement);

    expect(await screen.findByText("server-dev.sdkwork.com")).toBeTruthy();
    expect(screen.getByText("server-admin-dev.sdkwork.com")).toBeTruthy();
    expect(listSubdomains).toHaveBeenCalledWith(ROOT_ID, { page: 1, pageSize: 100 });
  });

  it("deletes a root from its row without also selecting that row", async () => {
    const { client, deleteRootDomain, listSubdomains } = stubClient();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client);

    const rootRow = (await screen.findByText("sdkwork.com")).closest("tr");
    const deleteButton = rootRow?.querySelector("button");
    expect(deleteButton).not.toBeNull();

    fireEvent.click(deleteButton as HTMLElement);

    // The delete ran...
    expect(confirm).toHaveBeenCalledOnce();
    expect(deleteRootDomain).toHaveBeenCalledWith(ROOT_ID, expect.objectContaining({ idempotencyKey: expect.any(String) }));
    // ...and the click did not bubble into the row handler, which would have
    // opened the subdomain panel for the root that was just removed.
    expect(listSubdomains).not.toHaveBeenCalled();
    expect(screen.getByText("Select a root domain to see the hostnames registered under it.")).toBeTruthy();
  });

  it("registers a new root domain with an idempotency key the server can dedupe on", async () => {
    const { client, createRootDomain } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client);

    const input = await screen.findByLabelText("Add root domain");
    fireEvent.change(input, { target: { value: "example.com" } });
    fireEvent.click(screen.getByRole("button", { name: "Add root domain" }));

    expect(createRootDomain).toHaveBeenCalledWith(
      { hostname: "example.com" },
      { idempotencyKey: expect.stringMatching(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u) },
    );
  });

  it("registers a subdomain under the opened root using its relative record name", async () => {
    const { client, createSubdomain } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client);

    fireEvent.click((await screen.findByText("sdkwork.com")).closest("tr") as HTMLElement);
    await screen.findByText("server-dev.sdkwork.com");

    fireEvent.change(screen.getByLabelText("Add subdomain"), { target: { value: "api" } });
    fireEvent.click(screen.getByRole("button", { name: "Add subdomain" }));

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
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client);

    expect(await screen.findByText("sdkwork-served")).toBeTruthy();
    const row = screen.getByText("sdkwork-served").closest("tr");
    expect(row?.textContent).toContain("server-dev.sdkwork.com");
    expect(row?.textContent).toContain("server-admin-dev.sdkwork.com");
    expect(row?.textContent).toContain("ISSUED");
  });

  it("offers the certificate lifecycle actions on each row", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client);

    const row = (await screen.findByText("sdkwork-served")).closest("tr");
    const labels = Array.from(row?.querySelectorAll("button") ?? []).map((button) => button.textContent);
    expect(labels).toEqual(["Renew", "Turn auto renew off", "Revoke", "Delete"]);
  });
});
