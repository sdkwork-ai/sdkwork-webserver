// @vitest-environment jsdom

import { WebserverAdminSdkProvider, type WebserverAdminSdkClient } from "@sdkwork/webserver-pc-admin-core";
import { ServedSiteCarrierAdminSurface } from "@sdkwork/webserver-pc-admin-delivery";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

/**
 * The Site Carriers ledger is the operator's only way to author the row that
 * migration `0031` made mandatory: a `webserver_application` that names the
 * SDKWork Deploy application it carries.
 *
 * The failure this suite exists for is a 422: before this page, nothing in the
 * product sent `deployApplicationId`, so the service rejected every create with a
 * validation problem naming the field. The assertions below pin the properties
 * that keep that from coming back:
 *
 * 1. The displayed Deploy link is the wire value, rendered as the int64 string it
 *    is — never coerced through `Number`, which silently rounds ids above
 *    `2^53`.
 * 2. A missing or malformed Deploy link is refused in the form, by name, before
 *    any request is made. The service's 422 is the fallback, not the UX.
 * 3. A submit that passes validation carries `deployApplicationId` (and *no*
 *    `appKind`: the member was removed from `CreateApplicationRequest`, because a
 *    carrier row's identity is the Deploy application and its kind is a
 *    server-owned default) on `applications.create`; an edit carries only what
 *    changed, because `UpdateApplicationRequest` has no link member.
 * 4. Publishing and lifecycle are Deploy's: the page offers no publish/template
 *    entry, activation is not offered at all (it requires a successful
 *    `webserver_deployment` the retired publishing writes can no longer create),
 *    and pausing carries an idempotency key.
 *
 * Matchers are plain DOM assertions: this app's vitest setup does not load
 * jest-dom.
 */

const DEPLOY_APP_ID = "1845123456789012345";

const ACTIVE_CARRIER = {
  appKind: "STATIC_WEB",
  createdAt: "2026-11-17T02:23:21.936919Z",
  deployApplicationId: DEPLOY_APP_ID,
  id: "carrier-1",
  name: "Marketing Site",
  siteType: 1,
  slug: "marketing-site",
  status: 1,
  updatedAt: "2026-11-17T02:23:21.936919Z",
};

const PAUSED_CARRIER = {
  ...ACTIVE_CARRIER,
  deployApplicationId: "1845123456789012346",
  id: "carrier-2",
  name: "Docs Site",
  slug: "docs-site",
  status: 2,
};

function page(items: readonly unknown[]) {
  return { items, pageInfo: { hasMore: false, page: 1, pageSize: 20 } };
}

function createClient(items: readonly unknown[] = [ACTIVE_CARRIER, PAUSED_CARRIER]) {
  const list = vi.fn().mockResolvedValue(page(items));
  const create = vi.fn().mockResolvedValue(ACTIVE_CARRIER);
  const update = vi.fn().mockResolvedValue(ACTIVE_CARRIER);
  const pause = vi.fn().mockResolvedValue({ ...ACTIVE_CARRIER, status: 2 });
  const client = {
    application: { create, list, pause, update },
  } as unknown as WebserverAdminSdkClient;
  return { client, create, list, pause, update };
}

function renderSurface(
  client: WebserverAdminSdkClient,
  permissionScope: readonly string[] = ["web.applications.*"],
) {
  return render(
    <WebserverAdminSdkProvider client={client}>
      <ServedSiteCarrierAdminSurface
        locale="en-US"
        permissionScope={permissionScope}
        resource="site-carriers"
      />
    </WebserverAdminSdkProvider>,
  );
}

/** Reads a control by its wire name, failing loudly when the form lacks it. */
function field(name: string): HTMLInputElement | HTMLTextAreaElement {
  const control = document.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[name="${name}"]`);
  if (control === null) throw new Error(`the form does not render a control named ${name}`);
  return control;
}

function fill(name: string, value: string) {
  fireEvent.change(field(name), { target: { value } });
}

function submitButton(dialog: HTMLElement): HTMLButtonElement {
  const button = dialog.querySelector<HTMLButtonElement>("button[type='submit']");
  if (button === null) throw new Error("the dialog has no submit control");
  return button;
}

/** Opens the register form; the button only renders for a write-capable scope. */
async function openRegisterForm() {
  fireEvent.click(screen.getByRole("button", { name: "Register carrier" }));
  await screen.findByRole("dialog");
}

afterEach(() => {
  cleanup();
});

describe("site carrier ledger", () => {
  it("lists carriers with their Deploy link kept as the int64 string the wire carries", async () => {
    const { client } = createClient();
    renderSurface(client);

    await screen.findByText("Marketing Site");
    expect(screen.getByText("marketing-site")).toBeDefined();
    // The exact digits, not a rounded Number rendering of the same id.
    expect(screen.getByText(DEPLOY_APP_ID)).toBeDefined();
    expect(screen.getByText("Active")).toBeDefined();
    expect(screen.getByText("Paused")).toBeDefined();
    // A paused carrier says whose action re-activation is, instead of offering a
    // control that would fail its deployment precondition.
    expect(screen.getByText("Re-activation and publishing happen in Deploy")).toBeDefined();
  });

  it("states that Deploy owns publishing, so the page never reads as a second application console", async () => {
    const { client } = createClient([]);
    renderSurface(client);

    await screen.findByText(/No carrier rows yet/);
    expect(screen.getByText(/Publishing and the application lifecycle are owned by Deploy/)).toBeDefined();
    // No publish, template, or activation entry on this page.
    expect(screen.queryByRole("button", { name: /publish/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /template/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /activate/i })).toBeNull();
  });

  it("names the Deploy link as the missing field and sends nothing when it is absent", async () => {
    const { client, create } = createClient([]);
    renderSurface(client);
    await screen.findByText(/No carrier rows yet/);

    await openRegisterForm();
    fill("name", "Storefront");
    fireEvent.click(submitButton(screen.getByRole("dialog")));

    expect(await screen.findByText(/Deploy application ID is required/)).toBeDefined();
    expect(create).not.toHaveBeenCalled();
    // The sentence is pinned to the control it names.
    expect(field("deployApplicationId").getAttribute("aria-invalid")).toBe("true");
  });

  it("refuses a Deploy link that is not a positive int64 and names the rule", async () => {
    const { client, create } = createClient([]);
    renderSurface(client);
    await screen.findByText(/No carrier rows yet/);

    await openRegisterForm();
    fill("name", "Storefront");
    fill("deployApplicationId", "0");
    fireEvent.click(submitButton(screen.getByRole("dialog")));
    expect(await screen.findByText(/must be digits only and greater than zero/)).toBeDefined();
    expect(create).not.toHaveBeenCalled();

    // A non-numeric id is the same refusal: an id is never coerced or truncated.
    fill("deployApplicationId", "deploy-app-1");
    fireEvent.click(submitButton(screen.getByRole("dialog")));
    expect(await screen.findByText(/must be digits only and greater than zero/)).toBeDefined();
    expect(create).not.toHaveBeenCalled();
  });

  it("refuses a name-less create before any request", async () => {
    const { client, create } = createClient([]);
    renderSurface(client);
    await screen.findByText(/No carrier rows yet/);

    await openRegisterForm();
    fill("deployApplicationId", DEPLOY_APP_ID);
    fireEvent.click(submitButton(screen.getByRole("dialog")));

    expect(await screen.findByText("Name is required.")).toBeDefined();
    expect(create).not.toHaveBeenCalled();
  });

  it("refuses runtime config that is not a JSON object before any request", async () => {
    const { client, create } = createClient([]);
    renderSurface(client);
    await screen.findByText(/No carrier rows yet/);

    await openRegisterForm();
    fill("name", "Storefront");
    fill("deployApplicationId", DEPLOY_APP_ID);
    fill("runtimeConfig", "{not json");
    fireEvent.click(submitButton(screen.getByRole("dialog")));

    expect(await screen.findByText(/Runtime config must be a JSON object/)).toBeDefined();
    expect(create).not.toHaveBeenCalled();

    // A JSON array is valid JSON but not the wire type (`Record<string, unknown>`).
    fill("runtimeConfig", "[1, 2]");
    fireEvent.click(submitButton(screen.getByRole("dialog")));
    expect(await screen.findByText(/Runtime config must be a JSON object/)).toBeDefined();
    expect(create).not.toHaveBeenCalled();
  });

  it("sends deployApplicationId and the parsed runtime config on create, with no appKind", async () => {
    const { client, create } = createClient([]);
    renderSurface(client);
    await screen.findByText(/No carrier rows yet/);

    await openRegisterForm();
    fill("name", "Storefront");
    fill("slug", "storefront");
    fill("deployApplicationId", DEPLOY_APP_ID);
    fill("runtimeConfig", '{"sso": true}');
    fireEvent.click(submitButton(screen.getByRole("dialog")));

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1));
    // Exact object equality: an `appKind` in the body would fail this assertion.
    // The member is gone from `CreateApplicationRequest` — the carrier row's kind
    // is a server-owned default, not an identity the page declares.
    expect(create).toHaveBeenCalledWith(
      {
        deployApplicationId: DEPLOY_APP_ID,
        name: "Storefront",
        runtimeConfig: { sso: true },
        slug: "storefront",
      },
      { idempotencyKey: expect.any(String) },
    );
  });

  it("edits name and runtime config without re-sending the link or an appKind", async () => {
    const { client, update } = createClient();
    renderSurface(client);
    await screen.findByText("Marketing Site");

    fireEvent.click(screen.getByRole("button", { name: "Edit Marketing Site" }));
    const dialog = await screen.findByRole("dialog");
    // The link is read-only after create: `UpdateApplicationRequest` has no member
    // for it, and the 1:1 relation is not re-declarable.
    const link = dialog.querySelector<HTMLInputElement>('input[name="deployApplicationId"]');
    expect(link?.value).toBe(DEPLOY_APP_ID);
    expect(link?.readOnly).toBe(true);

    fireEvent.change(field("name"), { target: { value: "Marketing Site 2" } });
    fireEvent.click(submitButton(dialog));

    await waitFor(() => expect(update).toHaveBeenCalledTimes(1));
    expect(update).toHaveBeenCalledWith(
      "carrier-1",
      { name: "Marketing Site 2" },
      { idempotencyKey: expect.any(String) },
    );
  });

  it("pauses an active carrier through the confirm dialog with an idempotency key", async () => {
    const { client, pause } = createClient();
    renderSurface(client);
    await screen.findByText("Marketing Site");

    fireEvent.click(screen.getByRole("button", { name: "Pause Marketing Site" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("Pause Marketing Site?");
    fireEvent.click(screen.getByRole("button", { name: "Pause" }));

    await waitFor(() => expect(pause).toHaveBeenCalledTimes(1));
    expect(pause).toHaveBeenCalledWith("carrier-1", { idempotencyKey: expect.any(String) });
  });

  it("hides every write control from a read-only scope while keeping the ledger readable", async () => {
    const { client } = createClient();
    renderSurface(client, ["web.applications.read"]);

    await screen.findByText("Marketing Site");
    expect(screen.getByText(DEPLOY_APP_ID)).toBeDefined();
    expect(screen.queryByRole("button", { name: "Register carrier" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Edit Marketing Site" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Pause Marketing Site" })).toBeNull();
  });
});
