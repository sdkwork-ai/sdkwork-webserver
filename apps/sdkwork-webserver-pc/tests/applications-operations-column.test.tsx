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
 * The six operations are the union of both sides of the merge that bridged this page:
 * the retired host ledger's `update` / `update-source` / `publish` / `delete`, **plus**
 * the deployments page's own `domains` / `detail` commands, which it had grown while the
 * other branch was in flight. Taking either side alone loses a real capability, so the
 * contract here is the union. `delete` is rendered but permanently disabled because the
 * deploy app-api contract defines no `apps.delete` — the `apps` resource exposes only
 * list / create / retrieve / update / activate / pause / domains.list / composition.update
 * / envVariables.* / healthChecks.*.
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

    const row = nameCell.closest("tr");
    expect(row, "the application row is rendered").toBeTruthy();

    // Asserted through the accessible name, not `textContent`: the operations
    // render as the module's icon buttons (`table-action`), exactly as
    // `DeliveryManagement.tsx` renders its ledgers, so the glyph carries no text
    // and the label lives in `aria-label` / `title`. Asserting on the accessible
    // name is also what keeps this test honest across either rendering — what it
    // guards is that all six operations stay present and named.
    const buttons = within(row as HTMLElement).getAllByRole("button");
    expect(buttons).toHaveLength(6);
    expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual([
      "Edit Store Front",
      "Modify source code Store Front",
      "Publish Store Front",
      "Domains Store Front",
      "Details Store Front",
      "Delete Store Front",
    ]);
    for (const button of buttons) {
      expect(button.getAttribute("title"), "every operation explains itself on hover").toBeTruthy();
      expect(button.getAttribute("aria-label")).toContain("Store Front");
    }
  });

  it("keeps delete disabled with the reason, while the other five stay actionable", async () => {
    stubAppsList();
    renderAppsSurface();
    const nameCell = await screen.findByText("Store Front");

    const row = nameCell.closest("tr") as HTMLElement;
    const [edit, source, publish, domains, detail, remove] = within(row).getAllByRole("button");

    // The contract has no `apps.delete`, so the slot must not promise a call it
    // cannot make — it says so in its title instead.
    expect((remove as HTMLButtonElement).disabled).toBe(true);
    expect(remove?.getAttribute("title")).toContain("no delete operation");

    for (const button of [edit, source, publish, domains, detail]) {
      expect((button as HTMLButtonElement).disabled, `${button?.getAttribute("aria-label")} is actionable`).toBe(false);
    }
  });
});
