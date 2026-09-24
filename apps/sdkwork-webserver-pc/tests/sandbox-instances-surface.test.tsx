// @vitest-environment jsdom

import { SandboxInstancesConsoleSurface } from "@sdkwork/webserver-pc-console-sandbox";
import { toSandboxExpiresAtLocal } from "@sdkwork/webserver-pc-console-sandbox";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * The VM Instances page, mounted with a stub transport.
 *
 * The stub is deliberate and it is stubbed at the *seam*: the page receives an
 * app-api base URL and creates its client through `createSandboxAppClient`, which
 * console-core owns. Replacing that one factory leaves the page, the form, the
 * model, the i18n, and the DataTable all real — so what is asserted below is the
 * page's own behaviour, and the assertions that matter are the ones about what
 * this page is *allowed to ask for*:
 *
 *  - it must never send an owner or a tenant (both come from the principal);
 *  - it must ask the server to filter and page rather than doing either locally;
 *  - it must not offer a transition or a delete the service would refuse.
 */

const stub = vi.hoisted(() => {
  const list = vi.fn();
  const create = vi.fn();
  const update = vi.fn();
  const remove = vi.fn();
  const attach = vi.fn((clients: readonly { http?: unknown }[]) => clients);
  const holder: { client: unknown } = { client: null };
  return { attach, create, holder, list, remove, update };
});

vi.mock("@sdkwork/webserver-pc-console-core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@sdkwork/webserver-pc-console-core")>();
  return { ...actual, createSandboxAppClient: () => stub.holder.client };
});

const PAGE_INFO = {
  mode: "offset" as const,
  page: 1,
  pageSize: 20,
  totalItems: "1",
  totalPages: 1,
  hasMore: false,
};

const REQUESTED = {
  sandboxInstanceId: "sbi-1",
  sandboxInstanceOwnerId: "usr-1",
  sandboxInstanceName: "agent-vm-01",
  sandboxInstanceState: "requested" as const,
  sandboxInstanceProfile: "standard" as const,
  sandboxInstanceBaseImage: "sdkwork/sandbox-runtime:latest",
  sandboxInstanceVcpuCount: 2,
  sandboxInstanceMemoryMb: 4_096,
  sandboxInstanceDiskMb: 20_480,
  sandboxInstanceRequiredCapabilities: ["terminal", "git"] as const,
  sandboxInstanceMinimumAssurance: "container" as const,
  sandboxInstanceAutoStart: false,
  sandboxVersion: "1",
};

function pageOf(items: readonly unknown[]): unknown {
  return { items, pageInfo: { ...PAGE_INFO, totalItems: String(items.length) } };
}

function field<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`field #${id} is not rendered`);
  return element as unknown as T;
}

function renderSurface(locale = "zh-CN") {
  return render(
    <SandboxInstancesConsoleSurface
      appApiBaseUrl="/"
      attachSdkClientBoundaries={stub.attach}
      locale={locale}
      resource="sandbox-instances"
      tokenManager={{} as never}
    />,
  );
}

/** The body of the nth call to a stubbed client method. */
function bodyOf(mock: typeof stub.list, index = 0): Record<string, unknown> {
  return mock.mock.calls[index][0] as unknown as Record<string, unknown>;
}

/** An `update` call is `(sandboxInstanceId, body)`; this reads the body. */
function updateBodyOf(index = 0): Record<string, unknown> {
  return stub.update.mock.calls[index][1] as unknown as Record<string, unknown>;
}

beforeEach(() => {
  vi.clearAllMocks();
  stub.list.mockResolvedValue(pageOf([REQUESTED]));
  stub.create.mockResolvedValue(REQUESTED);
  stub.update.mockResolvedValue(REQUESTED);
  stub.remove.mockResolvedValue({ sandboxInstanceId: "sbi-1", deleted: true });
  stub.holder.client = {
    create: stub.create,
    http: {},
    list: stub.list,
    remove: stub.remove,
    update: stub.update,
  };
});

afterEach(cleanup);

describe("sandbox instances console surface", () => {
  it("lists the caller's instances and never projects an owner or tenant", async () => {
    renderSurface();
    expect(await screen.findByText("agent-vm-01")).toBeTruthy();
    // The row carries the service's own vocabulary, localized. Scoped to the
    // table because the state filter offers those same state names as options,
    // so an unscoped query would match the filter rather than the row.
    const table = within(screen.getByRole("table"));
    expect(table.getByText("已申请")).toBeTruthy();
    expect(table.getByText("标准型")).toBeTruthy();

    const query = bodyOf(stub.list);
    expect(query).toEqual({ page: 1, pageSize: 20 });
    // Owner and tenant are derived from the verified principal server-side
    // (API_SPEC §10.2). Projecting either would let the page address another
    // account's rows, and the whole per-user contract rests on it not doing so.
    expect(query).not.toHaveProperty("sandboxInstanceOwnerId");
    expect(query).not.toHaveProperty("tenantId");

    // The transport is registered with the IAM session boundary, so a 401 from
    // this plane signs the operator out instead of painting failed reads.
    expect(stub.attach).toHaveBeenCalledTimes(1);
    expect(stub.attach.mock.calls[0][0]).toEqual([stub.holder.client]);
  });

  it("renders the page in English for an en-US locale", async () => {
    renderSurface("en-US");
    expect(await screen.findByText("VM Instances")).toBeTruthy();
    expect(await screen.findByText("agent-vm-01")).toBeTruthy();
    // Scoped to the table for the same reason as above: the filter offers it too.
    expect(within(screen.getByRole("table")).getByText("Requested")).toBeTruthy();
  });

  it("asks the server to filter by state instead of filtering the page it holds", async () => {
    renderSurface();
    await screen.findByText("agent-vm-01");
    stub.list.mockClear();

    fireEvent.change(screen.getByLabelText("按状态筛选"), { target: { value: "active" } });

    await waitFor(() => expect(stub.list).toHaveBeenCalled());
    // `fail` rather than a client-side filter: the listing is unbounded, so a
    // local filter would narrow one page and claim it was the whole collection.
    expect(bodyOf(stub.list)).toMatchObject({ page: 1, sandboxInstanceState: "active" });
  });

  it("tells an empty collection apart from an empty filter result", async () => {
    stub.list.mockResolvedValue(pageOf([]));
    renderSurface();
    expect(await screen.findByText("还没有虚拟机实例")).toBeTruthy();
    // The primary action is offered where there is nothing to show yet.
    expect(screen.getAllByText("开通虚拟机").length).toBeGreaterThan(0);

    fireEvent.change(screen.getByLabelText("按状态筛选"), { target: { value: "failed" } });

    // A different message, because "you have none" and "none is failed" are
    // different facts and only one of them is fixed by clearing a filter.
    expect(await screen.findByText("没有处于该状态的虚拟机实例")).toBeTruthy();
  });

  it("refuses an unnamed instance before spending a request", async () => {
    renderSurface();
    await screen.findByText("agent-vm-01");
    fireEvent.click(screen.getByRole("button", { name: "开通虚拟机" }));
    const dialog = await screen.findByRole("dialog");

    fireEvent.click(within(dialog).getByRole("button", { name: "开通" }));

    expect(await within(dialog).findByText("请填写名称。")).toBeTruthy();
    expect(stub.create).not.toHaveBeenCalled();
  });

  it("provisions what the form holds, with no owner in the body", async () => {
    renderSurface();
    await screen.findByText("agent-vm-01");
    fireEvent.click(screen.getByRole("button", { name: "开通虚拟机" }));
    const dialog = await screen.findByRole("dialog");

    fireEvent.change(field<HTMLInputElement>("sandbox-instance-name"), {
      target: { value: "agent-vm-02" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "开通" }));

    await waitFor(() => expect(stub.create).toHaveBeenCalledTimes(1));
    expect(bodyOf(stub.create)).toMatchObject({
      sandboxInstanceBaseImage: "sdkwork/sandbox-runtime:latest",
      sandboxInstanceDiskMb: 20_480,
      sandboxInstanceMemoryMb: 4_096,
      sandboxInstanceMinimumAssurance: "container",
      sandboxInstanceName: "agent-vm-02",
      sandboxInstanceProfile: "standard",
      sandboxInstanceVcpuCount: 2,
    });
    expect(bodyOf(stub.create)).not.toHaveProperty("sandboxInstanceOwnerId");
  });

  it("offers only the transitions the current state accepts, and PATCHes the choice", async () => {
    renderSurface();
    await screen.findByText("agent-vm-01");
    fireEvent.click(screen.getByRole("button", { name: "编辑" }));
    const dialog = await screen.findByRole("dialog");

    const nextState = field<HTMLSelectElement>("sandbox-instance-next-state");
    const offered = [...nextState.options].map((option) => option.value);
    // `requested` admits active / suspended / failed. Offering `terminated` here
    // would be a control that always answers 409.
    expect(offered).toEqual(["", "active", "suspended", "failed"]);

    fireEvent.change(nextState, { target: { value: "active" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存更改" }));

    await waitFor(() => expect(stub.update).toHaveBeenCalledTimes(1));
    expect(stub.update.mock.calls[0][0]).toBe("sbi-1");
    expect(stub.update.mock.calls[0][1]).toMatchObject({ sandboxInstanceState: "active" });
  });

  it("offers no transition at all on a terminal instance", async () => {
    stub.list.mockResolvedValue(pageOf([{ ...REQUESTED, sandboxInstanceState: "terminated" }]));
    renderSurface();
    await screen.findByText("agent-vm-01");
    fireEvent.click(screen.getByRole("button", { name: "编辑" }));

    const nextState = field<HTMLSelectElement>("sandbox-instance-next-state");
    // Only the "keep the current state" placeholder survives.
    expect(nextState.options.length).toBe(1);
  });

  it("freezes the base image where the wire has no field for it", async () => {
    renderSurface();
    await screen.findByText("agent-vm-01");
    fireEvent.click(screen.getByRole("button", { name: "编辑" }));
    await screen.findByRole("dialog");

    // `PATCH` carries no `sandboxInstanceBaseImage`, so an editable box would
    // offer a change the server silently drops.
    expect(field<HTMLInputElement>("sandbox-instance-base-image").readOnly).toBe(true);

    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "保存更改" }));
    await waitFor(() => expect(stub.update).toHaveBeenCalledTimes(1));
    expect(updateBodyOf()).not.toHaveProperty("sandboxInstanceBaseImage");
  });

  it("sends an expiry the operator typed and clears one they emptied", async () => {
    stub.list.mockResolvedValue(
      pageOf([{ ...REQUESTED, sandboxInstanceExpiresAt: "2026-10-01T01:30:00.000Z" }]),
    );
    renderSurface();
    await screen.findByText("agent-vm-01");
    fireEvent.click(screen.getByRole("button", { name: "编辑" }));
    const dialog = await screen.findByRole("dialog");

    const expiry = field<HTMLInputElement>("sandbox-instance-expires-at");
    // The stored UTC instant is shown as the wall clock the operator booked.
    expect(expiry.value).toBe(toSandboxExpiresAtLocal("2026-10-01T01:30:00.000Z"));

    fireEvent.change(expiry, { target: { value: "" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存更改" }));

    await waitFor(() => expect(stub.update).toHaveBeenCalledTimes(1));
    // Three-way on the wire: an emptied box must arrive as an explicit `null`
    // (clear), not as an omitted key (leave alone). Collapsing the two would make
    // a stored expiry impossible to remove from the console.
    const body = updateBodyOf();
    expect(Object.hasOwn(body, "sandboxInstanceExpiresAt")).toBe(true);
    expect(body.sandboxInstanceExpiresAt).toBeNull();
  });

  it("refuses to delete a live instance, and deletes a suspended one", async () => {
    stub.list.mockResolvedValue(pageOf([{ ...REQUESTED, sandboxInstanceState: "active" }]));
    renderSurface();
    await screen.findByText("agent-vm-01");

    const liveDelete = screen.getByRole("button", { name: "删除" }) as HTMLButtonElement;
    expect(liveDelete.disabled).toBe(true);
    expect(liveDelete.getAttribute("title")).toBe("运行中的实例不可删除，请先挂起或终止。");
    expect(stub.remove).not.toHaveBeenCalled();

    cleanup();
    stub.list.mockResolvedValue(pageOf([{ ...REQUESTED, sandboxInstanceState: "suspended" }]));
    renderSurface();
    await screen.findByText("agent-vm-01");

    const suspendedDelete = screen.getByRole("button", { name: "删除" }) as HTMLButtonElement;
    expect(suspendedDelete.disabled).toBe(false);
    fireEvent.click(suspendedDelete);

    const dialog = await screen.findByRole("alertdialog");
    // The confirmation names the instance, so a mis-click on a populated table is
    // not a blind delete.
    expect(within(dialog).getByText(/agent-vm-01/)).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "删除" }));

    await waitFor(() => expect(stub.remove).toHaveBeenCalledWith("sbi-1"));
  });

  it("keeps a failed command out of the listing frame", async () => {
    // An empty collection whose provisioning just failed: both facts are true, so
    // both surfaces render. This is why the page models the two errors separately
    // — a single channel would let a refused create repaint the listing as
    // unreadable, or a refused read repaint it as empty.
    stub.list.mockResolvedValue(pageOf([]));
    stub.create.mockRejectedValue(new Error("42201 refused"));
    renderSurface();
    await screen.findByText("还没有虚拟机实例");

    // Index 0 is the header action; the empty frame offers the same label.
    fireEvent.click(screen.getAllByRole("button", { name: "开通虚拟机" })[0]);
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(field<HTMLInputElement>("sandbox-instance-name"), {
      target: { value: "agent-vm-09" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "开通" }));

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("虚拟机实例开通失败");
    expect(alert.textContent).toContain("42201 refused");
    // The listing is still the empty one the page successfully read.
    expect(screen.getByText("还没有虚拟机实例")).toBeTruthy();
  });

  it("surfaces a refused read instead of an empty table", async () => {
    stub.list.mockRejectedValue(new Error("40301 forbidden"));
    renderSurface();
    // Exactly one alert: the frame below *is* the alert, so there is no second
    // banner repeating the same failure.
    const alert = await screen.findByRole("alert");
    // "unavailable" and "empty" must not look the same. Handing the failure to
    // the empty frame would tell the operator they own nothing and offer to
    // provision a first instance — neither of which is true or useful here.
    expect(alert.textContent).toContain("虚拟机实例列表不可用");
    expect(screen.queryByText("还没有虚拟机实例")).toBeNull();
    // The cause survives, so an expired session is distinguishable from an outage.
    expect(alert.textContent).toContain("40301 forbidden");

    // Retrying is the fix, and it re-issues the read rather than re-rendering.
    stub.list.mockResolvedValue(pageOf([REQUESTED]));
    fireEvent.click(within(alert).getByRole("button", { name: "刷新" }));
    expect(await screen.findByText("agent-vm-01")).toBeTruthy();
    expect(screen.queryByText("虚拟机实例列表不可用")).toBeNull();
  });
});
