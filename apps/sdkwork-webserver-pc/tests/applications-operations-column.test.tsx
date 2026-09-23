// @vitest-environment jsdom

import { createTokenManager } from "@sdkwork/sdk-common";
import { DeployAppsManagementSurface } from "@sdkwork/webserver-pc-console-delivery";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

/**
 * The applications ledger is bridged from sdkwork-deployments (`PublishingAppsPage`),
 * so its row actions are *not* declared in this repository. That is exactly why the
 * failure mode needs a test here: the page can lose its operations column, or drop
 * one of the four operations, while this repo still builds, still type-checks, and
 * still renders a perfectly valid-looking table — which is how the column went
 * missing once already (`311f7f61` deleted the host-owned action list, and the
 * bridged page replaced it without re-exposing the operations).
 *
 * The four operations mirror the retired host ledger's `update` / `update-source` /
 * `publish` / `delete`. `delete` is rendered but permanently disabled because the
 * deploy app-api contract defines no `apps.delete`.
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

function stubAppsList(): void {
  vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({
    code: 0,
    data: { items: [APP_ROW], pageInfo: { mode: "offset", page: 1, pageSize: 20, hasMore: false } },
    traceId: "trace-applications-1",
  }), {
    headers: { "content-type": "application/json" },
    status: 200,
  })));
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
      "Platform targets",
      "Version",
      "Updated",
      "Operations",
    ]);
  });

  it("exposes the four host-ledger operations on every row", async () => {
    stubAppsList();
    renderAppsSurface();
    const nameCell = await screen.findByText("Store Front");

    const row = nameCell.closest("tr");
    expect(row, "the application row is rendered").toBeTruthy();

    // Asserted through the accessible name, not `textContent`: the operations
    // render as the module's icon buttons (`table-action`), exactly as
    // `DeliveryManagement.tsx` renders its ledgers, so the glyph carries no text
    // and the label lives in `aria-label` / `title`. Keeping the assertion on
    // the accessible name means the test survives either rendering — what it
    // guards is that the four operations stay present and named.
    const buttons = within(row as HTMLElement).getAllByRole("button");
    expect(buttons).toHaveLength(4);
    expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual([
      "Edit Store Front",
      "Modify source code Store Front",
      "Publish Store Front",
      "Delete Store Front",
    ]);
    for (const button of buttons) {
      expect(button.getAttribute("title"), "every operation explains itself on hover").toBeTruthy();
      expect(button.getAttribute("aria-label")).toContain("Store Front");
    }
  });

  it("keeps delete disabled with the reason, while the other three stay actionable", async () => {
    stubAppsList();
    renderAppsSurface();
    const nameCell = await screen.findByText("Store Front");

    const row = nameCell.closest("tr") as HTMLElement;
    const [edit, source, publish, remove] = within(row).getAllByRole("button");

    // The contract has no `apps.delete`, so the slot must not promise a call it
    // cannot make — it says so in its title instead.
    expect((remove as HTMLButtonElement).disabled).toBe(true);
    expect(remove?.getAttribute("title")).toContain("no delete operation");

    for (const button of [edit, source, publish]) {
      expect((button as HTMLButtonElement).disabled, `${button?.getAttribute("aria-label")} is actionable`).toBe(false);
    }
  });
});
