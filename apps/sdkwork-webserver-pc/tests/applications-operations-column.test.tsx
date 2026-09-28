// @vitest-environment jsdom

import { createTokenManager } from "@sdkwork/sdk-common";
import { DeployAppsAdminSurface } from "@sdkwork/webserver-pc-admin-apps";
import { DeployAppsManagementSurface } from "@sdkwork/webserver-pc-console-delivery";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

/**
 * The applications ledger is bridged from sdkwork-deployments (`PublishingAppsPage`),
 * so its row actions are *not* declared in this repository. That is exactly why the
 * failure mode needs a test here: the page can lose its operations column, or drop
 * one of its commands, while this repo still builds, still type-checks, and still
 * renders a perfectly valid-looking table — which is how the column went missing
 * once already (`311f7f61` deleted the host-owned action list, and the bridged page
 * replaced it without re-exposing the operations).
 *
 * The contract below is the **union** of both sides of the merge that bridged this
 * page: the retired host ledger's `update` / `update-source` / `publish` / `delete`,
 * plus the `domains` / `detail` commands the deployments page had grown while the
 * branch was in flight. Taking either side alone loses a real capability.
 *
 * `delete` is no longer a disabled slot. The app-api defines no `DELETE /apps/{id}`,
 * but retirement *is* reachable — as the `apps.update` archive transition to
 * `AppStatus.ARCHIVED` — so the ledger exposes it as a real, confirming command.
 * `pause` / `activate` likewise exist as their own operations, and the ledger shows
 * whichever direction is legal for the row's current status.
 *
 * The same "declared upstream, rendered here" hazard applies to the ownership
 * columns. `deploy_app.owner_type` is what tells a platform-operated app apart
 * from a tenant's shared app and from one person's own, and the ledger is
 * tenant-wide — so if the branch loses those two columns, every row looks alike
 * again while this repo still builds and still renders a valid-looking table.
 * The fixture therefore carries the contract's required ownership fields and the
 * assertions below cover both a shared level and a personal one.
 */
const APP_ROW = {
  id: "app-1",
  name: "Store Front",
  slug: "store-front",
  appKind: "SPA_WEB",
  appStatus: "DRAFT",
  // Required by the `AppResponse` contract. The fixture has to carry them or it
  // is not a shape the server can actually produce (`sdkwork-contract-fixture-shape-audit`).
  ownerType: "TENANT",
  tenantId: "tenant-1",
  nginxConfigOverridden: false,
  platformTargetCount: 1,
  latestReleaseTag: null,
  updatedAt: "2026-09-23T04:00:00Z",
  version: 1,
  description: "",
};

/**
 * The commands every row carries, regardless of status.
 *
 * `Source specs` joined the list with the per-app source-spec contract
 * (`apps.sourceSpecs.*`): an app can carry several sources — PC / H5 / mini program —
 * and each client class is served by the lowest-ranked one that has a source. The
 * command declares that list, so it sits next to "Modify source code" rather than
 * replacing it: declaring which sources exist is a different act from uploading code
 * into one of them.
 *
 * `Release history` joined between "Publish" and "Domains" with the release-history
 * view: it reads the three app-scoped endpoints this ledger had no UI for
 * (`packages.list` / `releases.list` / `deployments.list`) and is the only place a
 * rollback can be issued from. It sits directly after the command that produces the
 * records it lists, because "what did I just publish, and can I take it back" is the
 * next question after a publish — not a detail of the app's current configuration.
 */
const BASE_ACTIONS = [
  "Edit Store Front",
  "Source specs Store Front",
  "Modify source code Store Front",
  "Publish Store Front",
  "Release history Store Front",
  "Domains Store Front",
  "Details Store Front",
];

/**
 * Row fixture. `ownerId` is part of the AppResponse contract — the server
 * resolves the USER/ORGANIZATION subject for the owner column — but the
 * base fixture is a TENANT-scoped row and leaves it unset.
 */
type AppRowFixture = typeof APP_ROW & { ownerId?: string };

function stubAppsList(...rows: readonly AppRowFixture[]): ReturnType<typeof vi.fn> {
  const items = rows.length > 0 ? rows : [APP_ROW];
  const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response(JSON.stringify({
    code: 0,
    data: { items, pageInfo: { mode: "offset", page: 1, pageSize: 20, hasMore: false } },
    traceId: "trace-applications-1",
  }), {
    headers: { "content-type": "application/json" },
    status: 200,
  }));
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function renderAppsSurface(): void {
  const tokenManager = createTokenManager({ accessToken: "test-access-token", authToken: "test-auth-token" });
  render(
    <DeployAppsManagementSurface
      deployBaseUrl="/"
      driveBaseUrl="/"
      locale="en-US"
      tokenManager={tokenManager}
    />,
  );
}

/** The backend-admin mount. Same bridge; the admin marker is the whole difference. */
function renderAdminAppsSurface(): void {
  const tokenManager = createTokenManager({ accessToken: "test-access-token", authToken: "test-auth-token" });
  render(
    <DeployAppsAdminSurface
      deployBaseUrl="/"
      driveBaseUrl="/"
      locale="en-US"
      tokenManager={tokenManager}
    />,
  );
}

/** Accessible names of the buttons inside the one rendered application row. */
async function rowActionNames(): Promise<(string | null)[]> {
  const nameCell = await screen.findByText("Store Front");
  const row = nameCell.closest("tr");
  expect(row, "the application row is rendered").toBeTruthy();
  return within(row as HTMLElement).getAllByRole("button").map((button) => button.getAttribute("aria-label"));
}

/** The `scope` query value of every list request, in call order (`null` = absent). */
function requestedScopes(fetchMock: ReturnType<typeof vi.fn>): (string | null)[] {
  return fetchMock.mock.calls
    .map(([input]) => String(input))
    .filter((url) => url.includes("/apps"))
    .map((url) => new URL(url, "http://console.test").searchParams.get("scope"));
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("applications ledger operations column", () => {
  it("renders an operations column as the last column of the ledger", async () => {
    stubAppsList();
    renderAppsSurface();
    await screen.findByText("Store Front");

    expect(screen.getAllByRole("columnheader").map((cell) => cell.textContent)).toEqual([
      "Name",
      "Slug",
      "Kind",
      "Status",
      "Ownership",
      "Owner",
      "Domain",
      "Platform targets",
      "Version",
      "Updated",
      "Operations",
    ]);
  });

  it("exposes the eight bridged operations on every row", async () => {
    stubAppsList();
    renderAppsSurface();
    const nameCell = await screen.findByText("Store Front");

    // Asserted through the accessible name, not `textContent`: the operations
    // render as the module's icon buttons (`table-action`), exactly as
    // `DeliveryManagement.tsx` renders its ledgers, so the glyph carries no text
    // and the label lives in `aria-label` / `title`. Asserting on the accessible
    // name is also what keeps this honest across either rendering — what it guards
    // is that every command stays present and named.
    const row = nameCell.closest("tr");
    const buttons = within(row as HTMLElement).getAllByRole("button");
    expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual([
      ...BASE_ACTIONS,
      "Archive Store Front",
    ]);
    for (const button of buttons) {
      expect(button.getAttribute("title"), "every operation explains itself on hover").toBeTruthy();
      expect(button.getAttribute("aria-label")).toContain("Store Front");
    }
  });

  it("retires an application through the archive transition, and asks first", async () => {
    const fetchMock = stubAppsList();
    renderAppsSurface();

    const nameCell = await screen.findByText("Store Front");
    const row = nameCell.closest("tr") as HTMLElement;
    const archive = within(row).getByRole("button", { name: "Archive Store Front" });

    // Retirement is not a `DELETE` route — the contract has none — so the slot
    // must be a real command rather than a permanently disabled placeholder.
    expect((archive as HTMLButtonElement).disabled).toBe(false);

    // It is still irreversible from this console, so it confirms before acting.
    archive.click();
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Store Front")).toBeTruthy();

    const beforeConfirm = fetchMock.mock.calls.filter(([, init]) =>
      String((init as RequestInit | undefined)?.body ?? "").includes("ARCHIVED"));
    expect(beforeConfirm, "opening the confirmation must not retire anything yet").toHaveLength(0);

    within(dialog).getByRole("button", { name: "Archive" }).click();

    const archival = await waitFor(() => {
      const call = fetchMock.mock.calls.find(([, init]) =>
        String((init as RequestInit | undefined)?.body ?? "").includes("ARCHIVED"));
      expect(call, "confirming retires the app through apps.update").toBeTruthy();
      return call;
    });
    expect(String(archival?.[0])).toContain("/apps/app-1");
  });

  it("offers the lifecycle direction that is legal for the row's status", async () => {
    // DRAFT has no legal ACTIVE/PAUSED transition, so neither direction is offered.
    stubAppsList();
    renderAppsSurface();
    expect(await rowActionNames()).toEqual([...BASE_ACTIONS, "Archive Store Front"]);

    cleanup();

    // An ACTIVE application can only be disabled, a PAUSED one only enabled.
    stubAppsList({ ...APP_ROW, appStatus: "ACTIVE" });
    renderAppsSurface();
    expect(await rowActionNames()).toEqual([...BASE_ACTIONS, "Disable Store Front", "Archive Store Front"]);

    cleanup();

    stubAppsList({ ...APP_ROW, appStatus: "PAUSED" });
    renderAppsSurface();
    expect(await rowActionNames()).toEqual([...BASE_ACTIONS, "Enable Store Front", "Archive Store Front"]);
  });
});

describe("applications ledger ownership columns", () => {
  /**
   * The whole point of the two columns is that the three levels stop looking
   * identical, so the test has to cover at least one shared level *and* the
   * personal one — a single row would pass even if the owner column printed the
   * level name twice.
   */
  it("names the level and the owner subject for a shared level and for a person", async () => {
    // Platform-wide: the owner *is* the level, so no single subject exists and the
    // owner column says so instead of repeating the level badge.
    stubAppsList({ ...APP_ROW, ownerType: "PLATFORM" });
    renderAppsSurface();
    const platformRow = (await screen.findByText("Store Front")).closest("tr") as HTMLElement;
    expect(within(platformRow).getByText("Platform app")).toBeTruthy();
    expect(within(platformRow).getByText("Whole platform")).toBeTruthy();

    cleanup();

    // Personal: the owner column carries the user id the server resolved.
    stubAppsList({ ...APP_ROW, ownerType: "USER", ownerId: "user-42" });
    renderAppsSurface();
    const personalRow = (await screen.findByText("Store Front")).closest("tr") as HTMLElement;
    expect(within(personalRow).getByText("Personal app")).toBeTruthy();
    expect(within(personalRow).getByText("user-42")).toBeTruthy();
  });

  /**
   * Ownership is a **column, not a control**: this console reaches one ownership
   * level, so a selector for that axis would be a selector with a single answer.
   * Two things have to survive its removal, and they are the two the rail used to
   * be blamed for — the *information* (every row names its level) and the request
   * shape (the ledger decides nothing about reach; the first load stays
   * unfiltered, so the server's ownership gate is the only thing answering "which
   * apps exist for me").
   *
   * The control was a tablist in the ledger's left rail before this; it used to be
   * a header `<select>`. Asserting its *absence* is what keeps a third form from
   * being added back without anyone noticing that the axis has one answer here.
   */
  it("labels each row's ownership level without a control for it", async () => {
    const fetchMock = stubAppsList();
    renderAppsSurface();
    const nameCell = await screen.findByText("Store Front");

    expect(
      requestedScopes(fetchMock).every((scope) => scope === null),
      "reach is the server's decision, never the browser's",
    ).toBe(true);
    expect(document.querySelector('[role="tablist"]'), "no ownership tabs come back").toBeNull();

    // The fixture row is TENANT-scoped, so the row has to say so.
    const row = nameCell.closest("tr") as HTMLElement;
    expect(within(row).getByText("Shared app"), "the row still names its level").toBeTruthy();
  });

  /**
   * The application-type facet is the *local* one, and the two must not be
   * confused: selecting a type narrows the rendered rows and sends no request at
   * all. If it ever started sending `scope`, the toolbar would be filtering
   * ownership by accident.
   */
  it("narrows by application type without asking the server again", async () => {
    // Two kinds in the fixture, because a chip only exists for a kind that has
    // rows — one row would leave the toolbar with nothing to select.
    const fetchMock = stubAppsList(
      APP_ROW,
      { ...APP_ROW, id: "app-2", name: "Pocket Shop", slug: "pocket-shop", appKind: "ANDROID_APP" },
    );
    renderAppsSurface();
    await screen.findByText("Store Front");
    expect(screen.getByText("Pocket Shop")).toBeTruthy();
    const requestsAfterMount = fetchMock.mock.calls.length;

    // The chip's accessible name is its localized label, so this stays a
    // role-and-name assertion rather than a class lookup.
    fireEvent.click(screen.getByRole("button", { name: /^Android app/ }));

    await waitFor(() => {
      expect(screen.queryByText("Store Front"), "the SPA row is filtered out").toBeNull();
    });
    expect(screen.getByText("Pocket Shop"), "the Android row stays").toBeTruthy();
    expect(fetchMock.mock.calls.length, "the type facet is a local filter").toBe(requestsAfterMount);
    expect(requestedScopes(fetchMock).every((scope) => scope === null)).toBe(true);
  });
});

describe("applications ledger admin surface", () => {
  /**
   * The two surfaces differ by exactly one control, and that control is the whole
   * reason the admin face is not a bare re-export of the console's page. The admin
   * reaches **every** ownership level, so the page gives it a tab row for that
   * axis, applied by the server (a tab is pushed down as `apps.list`'s `scope`);
   * the console reaches one level, so it gets no control for the axis at all —
   * asserted in "labels each row's ownership level without a control for it" above.
   * Both halves have to be here, because either one alone passes if the marker
   * stops being forwarded.
   *
   * It is also the one place a `scope` may appear in a request: the page's console
   * branch structurally cannot set the state the request reads. What the *server*
   * does with it is unchanged — `scope` narrows the reachable set, it never widens
   * it.
   */
  it("gives the admin mount the ownership tabs, and pushes the level down as `scope`", async () => {
    const fetchMock = stubAppsList();
    renderAdminAppsSurface();
    await screen.findByText("Store Front");

    // All levels first, then the contract's four levels: the label map is the
    // source, so a level the contract adds shows up without another edit.
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "All ownership levels",
      "Platform app",
      "Shared app",
      "Organization app",
      "Personal app",
    ]);
    // First paint is unfiltered — the page still decides nothing about reach on
    // its own, it only offers a way to narrow it.
    expect(requestedScopes(fetchMock).every((scope) => scope === null)).toBe(true);

    fireEvent.click(screen.getByRole("tab", { name: "Personal app" }));

    await waitFor(() => {
      expect(requestedScopes(fetchMock), "the tab reaches the server").toContain("USER");
    });
  });
});
