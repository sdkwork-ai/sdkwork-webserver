// @vitest-environment jsdom

import { WebserverAdminSdkProvider, type WebserverAdminSdkClient } from "@sdkwork/webserver-pc-admin-core";
import { ServedCertificateAdminSurface, ServedDomainAdminSurface } from "@sdkwork/webserver-pc-admin-delivery";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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

/**
 * The wildcard form of `sdkwork.com`, as the Domains page declares it.
 *
 * The record name is `*` and the stored hostname is `*.sdkwork.com`: the
 * asterisk is a *record* name that the backend composes with the root domain, and
 * the composed hostname is what the picker offers and what the issuance path
 * reads `hostname_type` off. A fixture that carried only the hostname would not
 * exercise the declaration half of the flow.
 */
const WILDCARD_SUBDOMAIN = {
  certificateCount: "0",
  createdAt: "2026-09-23T02:23:21.936919Z",
  hostname: "*.sdkwork.com",
  id: "d-wildcard",
  isPrimary: false,
  isVerified: true,
  recordName: "*",
  rootDomainId: ROOT_ID,
  sslEnabled: true,
  sslProvider: "letsencrypt",
  status: 1,
};

/** One offset page, in the shape the paged domain reads answer with. */
function hostnamePage(items: readonly unknown[]) {
  return { items, pageInfo: { hasMore: false, mode: "offset", page: 1, pageSize: 50 } };
}

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

/**
 * The edge's configured DNS accounts, in the shape `/dns_accounts` answers with.
 *
 * Two rows rather than one, and the two are deliberately *different* zones under
 * different providers. A single-row fixture cannot tell "the picker offered the
 * configured account" apart from "the picker rendered, and the automatic answer
 * happened to be the only thing there"; a second row is what makes choosing one
 * an assertion rather than a coincidence.
 */
const DNS_ACCOUNTS = [
  { accountId: "aliyun-prod", provider: "ALIYUN_DNS", zoneApex: "sdkwork.com" },
  { accountId: "cloudflare-mirror", provider: "CLOUDFLARE", zoneApex: "zowalk.com" },
];

interface Stubs {
  client: WebserverAdminSdkClient;
  createRootDomain: ReturnType<typeof vi.fn>;
  createSubdomain: ReturnType<typeof vi.fn>;
  deleteCertificate: ReturnType<typeof vi.fn>;
  deleteRootDomain: ReturnType<typeof vi.fn>;
  deleteDomain: ReturnType<typeof vi.fn>;
  dnsAccounts: ReturnType<typeof vi.fn>;
  issue: ReturnType<typeof vi.fn>;
  listCertificates: ReturnType<typeof vi.fn>;
  listSubdomains: ReturnType<typeof vi.fn>;
  renewCertificate: ReturnType<typeof vi.fn>;
  retrieveRootDomain: ReturnType<typeof vi.fn>;
  revokeCertificate: ReturnType<typeof vi.fn>;
  updateCertificate: ReturnType<typeof vi.fn>;
  updateRootDomain: ReturnType<typeof vi.fn>;
  verifyDomain: ReturnType<typeof vi.fn>;
}

function stubClient(): Stubs {
  const page = hostnamePage;
  const listSubdomains = vi.fn().mockResolvedValue(page(SUBDOMAINS));
  const createRootDomain = vi.fn().mockResolvedValue(ROOTS[0]);
  const createSubdomain = vi.fn().mockResolvedValue(SUBDOMAINS[0]);
  const deleteRootDomain = vi.fn().mockResolvedValue(undefined);
  const deleteDomain = vi.fn().mockResolvedValue(undefined);
  const dnsAccounts = vi.fn().mockResolvedValue(page(DNS_ACCOUNTS));
  const issue = vi.fn().mockResolvedValue(undefined);
  const listCertificates = vi.fn().mockResolvedValue(page(CERTIFICATES));
  const retrieveRootDomain = vi.fn().mockResolvedValue(ROOTS[0]);
  const updateRootDomain = vi.fn().mockResolvedValue(ROOTS[0]);
  const deleteCertificate = vi.fn().mockResolvedValue(undefined);
  const renewCertificate = vi.fn().mockResolvedValue(undefined);
  const revokeCertificate = vi.fn().mockResolvedValue(undefined);
  const updateCertificate = vi.fn().mockResolvedValue(CERTIFICATES[0]);
  const verifyDomain = vi.fn().mockResolvedValue({
    attemptCount: 1,
    expiresAt: "2026-09-23T03:23:21.936919Z",
    method: "DNS_TXT",
    recordName: "_sdkwork-verification.sdkwork.com",
    recordValue: "sdkwork-domain-verification=challenge-1",
    status: "PENDING",
    verified: false,
  });
  const client = {
    certificate: {
      // The issue form reads the edge's configured DNS accounts on open, so a
      // stub without this port throws on mount and takes the whole drawer with it
      // rather than failing one assertion.
      delete: deleteCertificate,
      dnsAccounts: { list: dnsAccounts },
      issue,
      list: listCertificates,
      renew: renewCertificate,
      revoke: revokeCertificate,
      update: updateCertificate,
    },
    domain: {
      delete: deleteDomain,
      list: vi.fn().mockResolvedValue(page(SUBDOMAINS)),
      // The ownership challenge: the Domains ledger's own verify action, which is
      // the only step that turns a declared hostname into a coverable one.
      verify: verifyDomain,
      rootDomains: {
        create: createRootDomain,
        delete: deleteRootDomain,
        list: vi.fn().mockResolvedValue(page(ROOTS)),
        retrieve: retrieveRootDomain,
        subdomains: { create: createSubdomain, list: listSubdomains },
        update: updateRootDomain,
      },
    },
  } as unknown as WebserverAdminSdkClient;
  return {
    client,
    createRootDomain,
    createSubdomain,
    deleteCertificate,
    deleteDomain,
    deleteRootDomain,
    dnsAccounts,
    issue,
    listCertificates,
    listSubdomains,
    renewCertificate,
    retrieveRootDomain,
    revokeCertificate,
    updateCertificate,
    updateRootDomain,
    verifyDomain,
  };
}

/**
 * Mounts a surface the way `WebserverWorkspace` does: at `<surface>/<resource>/*`.
 *
 * `entry` is the full location, so a case that arrives through a link — the
 * Domains ledger's request-certificate action — can carry its query string the
 * way the browser does.
 */
function renderInProvider(
  ui: ReactNode,
  client: WebserverAdminSdkClient,
  entry: string,
  basePath = entry,
) {
  return render(
    <WebserverAdminSdkProvider client={client}>
      <MemoryRouter initialEntries={[entry]}>
        <Routes>
          <Route element={ui} path={`${basePath}/*`} />
        </Routes>
      </MemoryRouter>
    </WebserverAdminSdkProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/**
 * The coverage picker: how a test opens it, finds a row in it, and takes one.
 *
 * The coverage list lives in its own dialog, so a test that clicks a hostname
 * label straight out of the drawer is aiming at something the drawer no longer
 * renders — the failure it produces is "unable to find a label", which reads
 * like a missing hostname rather than a stale interaction.
 *
 * The pane is a plain `<table>` with a radio per row, so a row is found by its
 * `.hostname-cell` text and taken through its own control. That is deliberately
 * not `getByLabelText`: the radio's accessible name is the hostname with
 * "Choose " in front of it, and asserting on that would pin the wording instead
 * of the hostname being chosen. `[data-sdk-row-id]` is gone with the framework
 * table it belonged to.
 */
async function openHostnamePicker(): Promise<HTMLElement> {
  fireEvent.click(screen.getByRole("button", { name: "Choose hostnames" }));
  return await screen.findByRole("dialog", { name: "Choose hostnames" });
}

function pickerRowFor(picker: HTMLElement, hostname: string): HTMLElement | undefined {
  return [...picker.querySelectorAll("tbody tr")].find((row) =>
    (row.querySelector(".hostname-cell")?.textContent ?? "").startsWith(hostname),
  ) as HTMLElement | undefined;
}

/**
 * Waits for the read behind the dialog, then takes the row's own control.
 *
 * The rows are a read, so the dialog exists before they do. Waiting for the
 * dialog alone races the fetch and reports "no picker row" — a message that
 * reads like the hostname is missing rather than like the test was early.
 */
async function pickHostname(picker: HTMLElement, hostname: string): Promise<void> {
  await waitFor(() => expect(pickerRowFor(picker, hostname), `no picker row for ${hostname}`).toBeTruthy());
  fireEvent.click(pickerRowFor(picker, hostname)?.querySelector('input[type="radio"]') as HTMLElement);
}

async function chooseHostname(hostname: string): Promise<void> {
  const picker = await openHostnamePicker();
  await pickHostname(picker, hostname);
  fireEvent.click(within(picker).getByRole("button", { name: "Confirm" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Choose hostnames" })).toBeNull());
}

/** The hostnames the pane is holding, read off the radios themselves. */
function chosenHostnames(picker: HTMLElement): string[] {
  return [...picker.querySelectorAll<HTMLInputElement>('tbody input[type="radio"]')]
    .filter((radio) => radio.checked)
    .map((radio) => radio.closest("tr")?.querySelector(".hostname-cell strong")?.textContent ?? "");
}

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

    // `zowalk.com` is the root that owns no subdomain, so it is the one whose
    // delete is offered at all — see the availability case below.
    const deleteButton = await screen.findByRole("button", { name: "Delete zowalk.com" });
    fireEvent.click(deleteButton);

    // The action opens the page's own dialog rather than window.confirm, so the
    // destructive step is a second, explicit click.
    expect(deleteRootDomain).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    expect(deleteRootDomain).toHaveBeenCalledWith(
      "360974393242841088",
      expect.objectContaining({ idempotencyKey: expect.any(String) }),
    );
    // ...and the delete did not navigate into the root that was just removed.
    expect(listSubdomains).not.toHaveBeenCalled();
  });

  /**
   * Every dialog on this plane shares one overlay contract (focus on open,
   * Escape to dismiss, scroll lock, focus restore). Escape is pinned here so
   * the shared backdrop keeps honouring the keyboard: a confirmation that
   * could only be left by clicking would trap keyboard operators.
   */
  it("dismisses the delete confirmation on Escape without deleting", async () => {
    const { client, deleteRootDomain } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    fireEvent.click(await screen.findByRole("button", { name: "Delete zowalk.com" }));
    expect(screen.getByRole("dialog")).toBeTruthy();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(deleteRootDomain).not.toHaveBeenCalled();
  });

  /**
   * The console's zone ledger carries five actions, and this ledger has to read
   * as the same ledger: two named navigations (entering the hostname list,
   * requesting a certificate — neither is guessable from a glyph), then the
   * lifecycle pair, then the destructive one. The order is part of the design,
   * not an accident of how the cell was written, so it is pinned here.
   */
  it("offers the console's five zone actions, in the console's order", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    const row = (await screen.findByText("sdkwork.com")).closest("tr");
    const actions = Array.from(row?.querySelectorAll(".row-actions > *") ?? []);
    expect(actions.map((node) => node.getAttribute("aria-label"))).toEqual([
      "Hostnames · sdkwork.com",
      "Certificates · sdkwork.com",
      "Edit sdkwork.com",
      "Pause sdkwork.com",
      "Delete sdkwork.com",
    ]);
    // The first two carry words; the console's rules size the column for four
    // 32px glyphs plus gaps, and a bare globe/certificate glyph there is exactly
    // the defect the console's own comment warns about.
    expect(actions[0]?.className).toContain("table-action-text");
    expect(actions[1]?.className).toContain("table-action-text");
    expect(actions[2]?.className).not.toContain("table-action-text");
  });

  it("links the request-certificate action at the certificate ledger, scoped to that root", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    const link = await screen.findByRole("link", { name: "Certificates · sdkwork.com" });
    expect(link.getAttribute("href")).toBe(
      `/admin/certificates?rootDomainId=${encodeURIComponent(ROOT_ID)}&apex=sdkwork.com`,
    );
  });

  /**
   * The console blocks the zone delete while the zone still owns anything, and
   * the operations plane enforces the same thing server-side (`subdomain_count >
   * 0` → 409). Offering an enabled button that can only come back refused is the
   * failure this pins down.
   */
  it("offers the delete only for a root that owns no subdomain", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    const blocked = await screen.findByRole("button", { name: "Delete sdkwork.com" });
    expect((blocked as HTMLButtonElement).disabled).toBe(true);
    const offered = screen.getByRole("button", { name: "Delete zowalk.com" });
    expect((offered as HTMLButtonElement).disabled).toBe(false);
  });

  it("pauses an active root through the one partial-edit call", async () => {
    const { client, updateRootDomain } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    fireEvent.click(await screen.findByRole("button", { name: "Pause sdkwork.com" }));
    fireEvent.click(screen.getByRole("button", { name: "Pause" }));

    expect(updateRootDomain).toHaveBeenCalledWith(
      ROOT_ID,
      { status: 2 },
      expect.objectContaining({ idempotencyKey: expect.any(String) }),
    );
  });

  it("sends only the fields the edit form changed", async () => {
    const { client, updateRootDomain } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    fireEvent.click(await screen.findByRole("button", { name: "Edit sdkwork.com" }));
    // Nothing changed yet, so there is nothing to send.
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.change(screen.getByLabelText("Display name"), { target: { value: "SDKWork" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(updateRootDomain).toHaveBeenCalledWith(
      ROOT_ID,
      { displayName: "SDKWork" },
      expect.objectContaining({ idempotencyKey: expect.any(String) }),
    );
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

  /**
   * The wildcard hostname is the one shape a `certificateScope WILDCARD` request
   * can cover, and this dialog is the only place it can be declared: `*` is the
   * record name that composes to `*.example.com`, which is what the backend
   * stores as a `WILDCARD` hostname. Without this the certificate form's wildcard
   * scope has nothing to offer and the flow ends at its own picker.
   *
   * The assertion is on the wire payload rather than on the box being ticked,
   * because the box is only interesting for what it puts in `recordName`.
   */
  it("declares the wildcard form of the root domain when the wildcard box is ticked", async () => {
    const { client, createSubdomain } = stubClient();
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    fireEvent.click(await screen.findByText("sdkwork.com"));
    await screen.findByText("server-dev.sdkwork.com");

    fireEvent.click(screen.getByRole("button", { name: "Add subdomain" }));
    fireEvent.click(screen.getByLabelText("Wildcard subdomain"));
    // The field shows what the request will carry, rather than leaving the star
    // to be inferred from the box beside it.
    expect((screen.getByLabelText("Record name") as HTMLInputElement).value).toBe("*");
    expect(screen.getByText(/Declares \*\.sdkwork\.com/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(createSubdomain).toHaveBeenCalledWith(
      ROOT_ID,
      { recordName: "*", sslEnabled: true },
      { idempotencyKey: expect.any(String) },
    );
  });

  /**
   * A hostname is stored pending until its ownership challenge is checked, and the
   * issuance path covers `VERIFIED` rows only — so this control is the step between
   * declaring a hostname and being able to cover it. Without it a hostname
   * declared here never becomes coverable, and the wildcard form has no other
   * route at all: `*.example.com` is not reconciled from the edge's own
   * configuration unless that configuration already names it.
   *
   * The challenge is asserted rather than the call: which TXT record to publish is
   * the only thing the operator can act on, and a dialog that reported "pending"
   * without the record name and value would be a loop with no exit.
   */
  it("verifies a declared hostname and shows the challenge to publish", async () => {
    const { client, listSubdomains, verifyDomain } = stubClient();
    listSubdomains.mockResolvedValue(
      hostnamePage([{ ...SUBDOMAINS[0], isVerified: false }, SUBDOMAINS[1]]),
    );
    renderInProvider(<ServedDomainAdminSurface locale="en-US" resource="domains" />, client, "/admin/domains");

    fireEvent.click(await screen.findByText("sdkwork.com"));
    await screen.findByText("server-dev.sdkwork.com");
    // A verified row has nothing to verify, so the control is not offered there.
    expect(screen.queryByRole("button", { name: "Verify server-admin-dev.sdkwork.com" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Verify server-dev.sdkwork.com" }));

    expect(verifyDomain).toHaveBeenCalledWith("d-server-dev", { idempotencyKey: expect.any(String) });
    const dialog = await screen.findByRole("dialog", { name: "Verify server-dev.sdkwork.com" });
    await waitFor(() =>
      expect((within(dialog).getByLabelText("TXT record name") as HTMLInputElement).value).toBe(
        "_sdkwork-verification.sdkwork.com",
      ),
    );
    expect((within(dialog).getByLabelText("TXT record value") as HTMLInputElement).value).toBe(
      "sdkwork-domain-verification=challenge-1",
    );
    // Still pending, so the loop has to stay available rather than closing on a
    // verdict the operator cannot change.
    expect(within(dialog).getByRole("button", { name: "Check again" })).toBeTruthy();
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

  /**
   * The ledger effect depends on the locale-bound translator. A fresh
   * translator closure on every render used to re-run the effect forever:
   * fetch completes -> setState -> new `t` identity -> refetch. The
   * translator is memoized per locale, so the request count must settle at
   * one per pagination build instead of growing without bound.
   */
  it("fetches the ledger once per build, not once per render", async () => {
    const { client, listCertificates } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    // Any unbounded refetch loop shows up within a few macrotask flushes:
    // every completed fetch used to schedule the next one immediately.
    for (let flush = 0; flush < 5; flush += 1) {
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    expect(listCertificates).toHaveBeenCalledTimes(1);
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

  /**
   * The Domains ledger's request-certificate action is a navigation, so the
   * landing page has to honour what it carries: the form opens, and its
   * identifier picker is scoped to the root domain that sent the operator here
   * rather than to the whole tenant inventory. Both halves are the console's
   * behaviour for `?zoneId=…`, mirrored on this plane.
   */
  it("opens the request form scoped to the root domain it was sent with", async () => {
    const { client, listSubdomains } = stubClient();
    renderInProvider(
      <ServedCertificateAdminSurface locale="en-US" resource="certificates" />,
      client,
      `/admin/certificates?rootDomainId=${encodeURIComponent(ROOT_ID)}&apex=sdkwork.com`,
      "/admin/certificates",
    );

    // The scoped read is the evidence: the toolbar's own entry point reads the
    // whole inventory instead.
    expect(listSubdomains).toHaveBeenCalledWith(ROOT_ID, { page: 1, pageSize: 200 });
    expect(await screen.findByText("Served hostnames to cover")).toBeTruthy();
    // Scoped to the dialog: the ledger behind it renders the issued
    // certificate's identifiers, which is the same hostname seen twice.
    const dialog = within(screen.getByRole("dialog"));
    // The form opened on the root domain it was sent with. The drawer states
    // that on its own line and offers the hostnames behind the picker, so the
    // scoping is asserted through both: the label says which root, the picker
    // says that root's names are the ones on offer.
    expect(dialog.getByText("Root domain: sdkwork.com")).toBeTruthy();
    fireEvent.click(dialog.getByRole("button", { name: "Choose hostnames" }));
    const picker = await screen.findByRole("dialog", { name: "Choose hostnames" });
    await waitFor(() => expect(within(picker).getByText("server-dev.sdkwork.com")).toBeTruthy());
    expect(within(picker).getByText("server-admin-dev.sdkwork.com")).toBeTruthy();
  });

  it("reads the whole inventory when the toolbar opens the request form", async () => {
    const { client, listSubdomains } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    expect(listSubdomains).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    expect(await screen.findByText("Served hostnames to cover")).toBeTruthy();
    expect(listSubdomains).not.toHaveBeenCalled();
  });

  /**
   * The coverage pane is a row list, not a grid of cards: one hostname per row,
   * with the facts an operator compares before choosing in columns beside it.
   *
   * The assertion is on the columns' headers and on the number of rows, because
   * the failure this guards against — a return to two cards per row — keeps every
   * hostname on screen and only changes how many of them share a line and how much
   * width each one gets. Counting rows is what tells the two apart.
   */
  it("lists the root domain's hostnames as rows with their facts beside them", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose hostnames" }));
    const picker = await screen.findByRole("dialog", { name: "Choose hostnames" });

    // One row per hostname the root domain has — two in the fixture — so a layout
    // that put two names on one line would come out as one row rather than two.
    await waitFor(() => expect(picker.querySelectorAll("tbody tr").length).toBe(2));

    // One choice per row, and the choice is a radio. The field holds a single
    // hostname, so a checkbox would be offering something the request cannot
    // carry, and a select-all in the header would be offering all of them.
    expect(picker.querySelectorAll('tbody input[type="radio"]').length).toBe(2);
    expect(picker.querySelectorAll('tbody input[type="checkbox"]').length).toBe(0);
    expect(picker.querySelectorAll("thead input").length).toBe(0);

    // The choice column has no header of its own, so the empty cell is dropped
    // rather than asserted as a blank.
    const headers = [...picker.querySelectorAll("thead th")]
      .map((th) => th.textContent?.trim() ?? "")
      .filter((label) => label !== "");
    expect(headers).toEqual(["Record name", "Hostname", "Verification", "Application", "Certificates"]);

    // The facts follow the name into the row: the record name that goes into DNS,
    // and the verdict on whether control of the name is proven.
    const row = pickerRowFor(picker, "server-dev.sdkwork.com") as HTMLElement;
    expect(row.textContent).toContain("server-dev");
    expect(row.textContent).toContain("Verified");
    // Opening the pane is not choosing: it holds nothing until a row is taken.
    expect(chosenHostnames(picker)).toEqual([]);
  });

  /**
   * The empty answer is a row of the table, not a block beside it.
   *
   * It has to land under the headers it is answering about, and it has to carry
   * the class `deploy-surface.css` pads and centres — so both are asserted here
   * rather than left to the stylesheet. An empty `<tbody>` is the failure this
   * guards against: a header bar over nothing reads as a read that failed rather
   * than as a root domain with no hostnames yet, and the two want different
   * reactions from an operator.
   */
  it("says so inside the table when a root domain has no hostnames yet", async () => {
    const { client, listSubdomains } = stubClient();
    listSubdomains.mockResolvedValue({
      items: [],
      pageInfo: { hasMore: false, mode: "offset", page: 1, pageSize: 50 },
    });
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    const picker = await openHostnamePicker();

    const empty = await waitFor(() => {
      const cell = picker.querySelector(".hostname-picker-empty");
      expect(cell).toBeTruthy();
      return cell as HTMLElement;
    });
    expect(empty.textContent).toBe("This root domain has no hostnames yet");
    expect(empty.getAttribute("colspan")).toBe("6");
    // Still six columns above it: the empty answer shares the row's grid rather
    // than replacing it, the unnamed choice column included.
    expect(picker.querySelectorAll("thead th").length).toBe(6);
    expect(chosenHostnames(picker)).toEqual([]);
  });

  /**
   * The drawer is a form whose default matters. The key algorithm control is
   * rendered with RSA pressed — the platform default, shown as a choice rather
   * than assumed — and submitting without touching it asks for RSA on the wire,
   * with the idempotency key the server dedupes retries on.
   *
   * The payload is pinned whole rather than partially. `providerAccountId` and
   * `certName` are *absent* here and that absence is the assertion: "automatic"
   * and "no name given" have to reach the server as omissions, because an empty
   * string is a different request — one that names an account nobody configured,
   * or a certificate with a blank name.
   */
  it("issues through the drawer with the RSA default and an idempotency key", async () => {
    const { client, issue } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    // The fixture's *second* row, not its first: the payload has to be the name
    // the test aimed at rather than whatever an untouched pane would default to.
    await chooseHostname("server-admin-dev.sdkwork.com");
    fireEvent.click(screen.getByRole("button", { name: "Issue" }));

    expect(issue).toHaveBeenCalledWith(
      {
        domainIds: ["d-server-admin-dev"],
        certType: 1,
        keyAlgorithm: "RSA",
        autoRenew: true,
        certificateScope: "SINGLE_DOMAIN",
        renewBeforeDays: 30,
        validationMethod: "AUTO",
        caProfile: "LETS_ENCRYPT_PRODUCTION",
      },
      expect.objectContaining({ idempotencyKey: expect.any(String) }),
    );
  });

  /**
   * The wildcard flow, read from the certificate form's side: a root domain that
   * has declared its wildcard hostname offers it under the wildcard scope, and
   * the request that leaves the drawer names that hostname and that scope.
   *
   * This is the half the Domains-page test hands off to. Before the declaration
   * path existed, no `WILDCARD` hostname could be created from the product, so
   * this pane had nothing to offer and `certificateScope WILDCARD` was refused by
   * the server with "requires at least one wildcard identifier" no matter what the
   * operator did here.
   */
  it("issues a wildcard certificate from the wildcard hostname the root domain declared", async () => {
    const { client, issue, listSubdomains } = stubClient();
    listSubdomains.mockResolvedValue(hostnamePage([WILDCARD_SUBDOMAIN, ...SUBDOMAINS]));
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    fireEvent.click(screen.getByRole("button", { name: "Wildcard" }));

    // The declared wildcard is the one selectable row, which is what makes this
    // the *wildcard* flow rather than "whatever happened to be on offer": the
    // exact rows beside it are refused, and the declare-it-first hint that stands
    // in when a root domain has no wildcard is absent here.
    const picker = await openHostnamePicker();
    await waitFor(() => expect(picker.querySelectorAll("tbody tr").length).toBe(3));
    const wildcardRow = pickerRowFor(picker, "*.sdkwork.com") as HTMLElement;
    const exactRow = pickerRowFor(picker, "server-dev.sdkwork.com") as HTMLElement;
    expect(wildcardRow.querySelector<HTMLInputElement>('input[type="radio"]')?.disabled).toBe(false);
    expect(exactRow.querySelector<HTMLInputElement>('input[type="radio"]')?.disabled).toBe(true);
    expect(within(picker).queryByText(/No wildcard hostname is declared/)).toBeNull();

    await pickHostname(picker, "*.sdkwork.com");
    fireEvent.click(within(picker).getByRole("button", { name: "Confirm" }));
    fireEvent.click(screen.getByRole("button", { name: "Issue" }));

    expect(issue).toHaveBeenCalledWith(
      expect.objectContaining({
        domainIds: ["d-wildcard"],
        certificateScope: "WILDCARD",
        // A wildcard can only be authorized over DNS-01, so the automatic
        // resolution is the only method the drawer may send here; naming HTTP-01
        // would be a request the server refuses on its face.
        validationMethod: "AUTO",
      }),
      expect.objectContaining({ idempotencyKey: expect.any(String) }),
    );
  });

  /**
   * A wildcard scope with no wildcard hostname declared is the one state where
   * every row is refused. The pane has to say what is missing *and* where it is
   * declared: a column of dead radios with no explanation is exactly how the scope
   * came to read as a dead end.
   */
  it("refuses exact hostnames under the wildcard scope and says what to declare", async () => {
    const { client } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    fireEvent.click(screen.getByRole("button", { name: "Wildcard" }));
    const picker = await openHostnamePicker();
    await waitFor(() => expect(picker.querySelectorAll("tbody tr").length).toBe(2));

    // Both fixture hostnames are exact, and the server requires at least one
    // wildcard identifier for this scope, so neither can satisfy it.
    const radios = [...picker.querySelectorAll<HTMLInputElement>('tbody input[type="radio"]')];
    expect(radios.length).toBe(2);
    expect(radios.every((radio) => radio.disabled)).toBe(true);
    // Showed, not hidden, and each refused row carries its own reason.
    expect(
      within(picker).getAllByText("A wildcard certificate can only be built from a wildcard hostname.").length,
    ).toBe(2);
    // The hint names the concrete hostname to declare, so the refusal is
    // actionable rather than a statement that the scope is unusable.
    expect(
      within(picker).getByText(
        /No wildcard hostname is declared under this root domain\. Add \*\.sdkwork\.com on the Domains page/,
      ),
    ).toBeTruthy();
  });

  /**
   * Ownership evidence is the issuance path's own precondition: it resolves
   * `domainIds` against rows whose verification status is `VERIFIED` only. A
   * picker that offered an unverified name would move that refusal to the submit,
   * where it reads as a server fault rather than as the step that was skipped.
   */
  it("refuses a hostname whose ownership is not verified yet", async () => {
    const { client, listSubdomains } = stubClient();
    listSubdomains.mockResolvedValue(hostnamePage([{ ...SUBDOMAINS[0], isVerified: false }]));
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    const picker = await openHostnamePicker();
    const row = await waitFor(() => {
      const found = pickerRowFor(picker, "server-dev.sdkwork.com");
      expect(found).toBeTruthy();
      return found as HTMLElement;
    });

    expect(row.querySelector<HTMLInputElement>('input[type="radio"]')?.disabled).toBe(true);
    expect(row.textContent).toContain("Not verified yet");
  });

  it("sends ECDSA only after the operator switches the algorithm control", async () => {
    const { client, issue } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    await chooseHostname("server-dev.sdkwork.com");
    fireEvent.click(screen.getByRole("button", { name: "ECDSA" }));
    fireEvent.click(screen.getByRole("button", { name: "Issue" }));

    expect(issue).toHaveBeenCalledWith(
      expect.objectContaining({ domainIds: ["d-server-dev"], certType: 1, keyAlgorithm: "ECDSA", autoRenew: true }),
      expect.anything(),
    );
  });

  /**
   * The cloud account is the one setting whose "I chose it" and "I left it
   * automatic" states are both invisible in the request unless the field really
   * reaches the wire, so the test drives the picker and then reads the payload.
   *
   * It picks the *second* account. Picking the first would pass even if the
   * summary were the only thing that changed, because the first row is also what
   * an unfiltered default would land on.
   */
  it("carries the account chosen in the picker and the name typed beside it", async () => {
    const { client, issue, dnsAccounts } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    // Addressed through the trigger rather than through the word "Automatic",
    // which the summary and the picker's first row both carry.
    expect(await screen.findByRole("button", { name: "Choose account" })).toBeTruthy();
    expect(dnsAccounts).toHaveBeenCalledWith({ page: 1, pageSize: 200 });

    await chooseHostname("server-dev.sdkwork.com");
    // "Automatic" is both the summary's own text and the picker's first row, so
    // the trigger is addressed by its label rather than by that word.
    fireEvent.click(screen.getByRole("button", { name: "Choose account" }));
    fireEvent.click(await screen.findByText("cloudflare-mirror"));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

    fireEvent.change(screen.getByLabelText("Certificate name"), { target: { value: "edge-front-door" } });
    fireEvent.click(screen.getByRole("button", { name: "Issue" }));

    expect(issue).toHaveBeenCalledWith(
      expect.objectContaining({ providerAccountId: "cloudflare-mirror", certName: "edge-front-door" }),
      expect.anything(),
    );
  });

  /** Escape and the header's own close button are both exits; neither submits. */
  it("closes the drawer on Escape and on its close button, without issuing", async () => {
    const { client, issue } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    expect(await screen.findByRole("dialog")).toBeTruthy();

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    fireEvent.click(await screen.findByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(issue).not.toHaveBeenCalled();
  });

  /** The picker's search appears only once the list is long enough to need it. */
  it("filters the hostname picker once the inventory is long enough to need it", async () => {
    const { client, listSubdomains } = stubClient();
    const many = Array.from({ length: 10 }, (_, index) => ({
      hostname: `svc-${String(index).padStart(2, "0")}.sdkwork.com`,
      id: `d-svc-${index}`,
      rootDomainId: ROOT_ID,
      status: 1,
    }));
    // The picker reads a root domain's own hostnames, not the flat inventory, so
    // the stub has to answer on the port the picker actually calls.
    listSubdomains.mockResolvedValue({
      items: many,
      pageInfo: { hasMore: false, mode: "offset", page: 1, pageSize: 50 },
    });
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    fireEvent.click(await screen.findByRole("button", { name: "Choose hostnames" }));
    const picker = await screen.findByRole("dialog", { name: "Choose hostnames" });

    const search = await within(picker).findByPlaceholderText("Search hostnames");
    fireEvent.change(search, { target: { value: "svc-07" } });
    expect(within(picker).getByText("svc-07.sdkwork.com")).toBeTruthy();
    expect(within(picker).queryByText("svc-00.sdkwork.com")).toBeNull();
  });

  /**
   * Choosing a second name replaces the first.
   *
   * A request names one hostname, so the pane is a radio group rather than a set
   * of ticks, and the mark that says so sits on exactly one row. The search box is
   * moved in between on purpose: it is where a "keep what was already chosen" rule
   * would put the first name back, and where rebuilding the draft from the
   * *filtered* rows would silently drop it instead.
   *
   * The assertion the request turns on is the drawer's own summary, because that
   * is the draft as the form actually holds it — the pane could look right while
   * the form carried two.
   */
  it("replaces the chosen hostname when a second one is picked", async () => {
    const { client, listSubdomains } = stubClient();
    const many = Array.from({ length: 10 }, (_, index) => ({
      hostname: `svc-${String(index).padStart(2, "0")}.sdkwork.com`,
      id: `d-svc-${index}`,
      rootDomainId: ROOT_ID,
      status: 1,
    }));
    listSubdomains.mockResolvedValue({
      items: many,
      pageInfo: { hasMore: false, mode: "offset", page: 1, pageSize: 50 },
    });
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    await screen.findByText("sdkwork-served");
    fireEvent.click(screen.getByRole("button", { name: "Issue certificate" }));
    const picker = await openHostnamePicker();

    await pickHostname(picker, "svc-00.sdkwork.com");
    expect(chosenHostnames(picker)).toEqual(["svc-00.sdkwork.com"]);

    const search = await within(picker).findByPlaceholderText("Search hostnames");
    fireEvent.change(search, { target: { value: "svc-07" } });
    expect(within(picker).queryByText("svc-00.sdkwork.com")).toBeNull();
    await pickHostname(picker, "svc-07.sdkwork.com");

    // One row carries the mark, and it is the one chosen last.
    expect(chosenHostnames(picker)).toEqual(["svc-07.sdkwork.com"]);

    fireEvent.click(within(picker).getByRole("button", { name: "Confirm" }));
    const chips = [...document.querySelectorAll(".selected-hostnames button")].map(
      (chip) => chip.getAttribute("aria-label") ?? "",
    );
    expect(chips).toHaveLength(1);
    expect(chips[0]).toContain("svc-07.sdkwork.com");
  });

  it("renews only after the row action is confirmed, with a fresh idempotency key", async () => {
    const { client, renewCertificate } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    const row = (await screen.findByText("sdkwork-served")).closest("tr");
    fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "Renew sdkwork-served" }));
    expect(renewCertificate).not.toHaveBeenCalled();

    const confirm = screen.getByRole("dialog", { name: "Renew" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Renew" }));
    expect(renewCertificate).toHaveBeenCalledWith("cert-1", {
      idempotencyKey: expect.any(String),
    });
  });

  it("toggles auto renew through the update port with the flipped value", async () => {
    const { client, updateCertificate } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    const row = (await screen.findByText("sdkwork-served")).closest("tr");
    // The fixture ships `autoRenew: true`, so the row's action is "turn it off".
    fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "Turn auto renew off sdkwork-served" }));

    expect(updateCertificate).toHaveBeenCalledWith(
      "cert-1",
      { autoRenew: false },
      expect.objectContaining({ idempotencyKey: expect.any(String) }),
    );
  });

  it("revokes with the reason the operator chose in the dialog", async () => {
    const { client, revokeCertificate } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    const row = (await screen.findByText("sdkwork-served")).closest("tr");
    fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "Revoke sdkwork-served" }));

    const confirm = screen.getByRole("dialog", { name: "Revoke" });
    fireEvent.change(within(confirm).getByLabelText("Revocation reason"), {
      target: { value: "keyCompromise" },
    });
    fireEvent.click(within(confirm).getByRole("button", { name: "Revoke" }));

    expect(revokeCertificate).toHaveBeenCalledWith(
      "cert-1",
      { reason: "keyCompromise" },
      expect.objectContaining({ idempotencyKey: expect.any(String) }),
    );
  });

  it("deletes the record only after its own confirmation", async () => {
    const { client, deleteCertificate } = stubClient();
    renderInProvider(<ServedCertificateAdminSurface locale="en-US" resource="certificates" />, client, "/admin/certificates");

    const row = (await screen.findByText("sdkwork-served")).closest("tr");
    fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "Delete sdkwork-served" }));
    expect(deleteCertificate).not.toHaveBeenCalled();

    const confirm = screen.getByRole("dialog", { name: "Delete" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Delete" }));
    expect(deleteCertificate).toHaveBeenCalledWith(
      "cert-1",
      expect.objectContaining({ idempotencyKey: expect.any(String) }),
    );
  });
});
