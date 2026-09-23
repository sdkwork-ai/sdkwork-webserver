// @vitest-environment jsdom

import { createTokenManager } from "@sdkwork/sdk-common";
import { DeployAppsManagementSurface } from "@sdkwork/webserver-pc-console-delivery";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
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
 */
const APP_ROW = {
  id: "app-1",
  name: "Store Front",
  slug: "store-front",
  appKind: "SPA_WEB",
  appStatus: "DRAFT",
  platformTargetCount: 1,
  latestReleaseTag: null,
  updatedAt: "2026-09-23T04:00:00Z",
  version: 1,
  description: "",
};

/** The five commands every row carries, regardless of status. */
const BASE_ACTIONS = [
  "Edit Store Front",
  "Modify source code Store Front",
  "Publish Store Front",
  "Domains Store Front",
  "Details Store Front",
];

function stubAppsList(row: typeof APP_ROW = APP_ROW): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response(JSON.stringify({
    code: 0,
    data: { items: [row], pageInfo: { mode: "offset", page: 1, pageSize: 20, hasMore: false } },
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

/** Accessible names of the buttons inside the one rendered application row. */
async function rowActionNames(): Promise<(string | null)[]> {
  const nameCell = await screen.findByText("Store Front");
  const row = nameCell.closest("tr");
  expect(row, "the application row is rendered").toBeTruthy();
  return within(row as HTMLElement).getAllByRole("button").map((button) => button.getAttribute("aria-label"));
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
      "Domain",
      "Platform targets",
      "Version",
      "Updated",
      "Operations",
    ]);
  });

  it("exposes the six bridged operations on every row", async () => {
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
