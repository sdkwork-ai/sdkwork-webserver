// @vitest-environment jsdom

import { webserverModule as clusterModule } from "@sdkwork/webserver-pc-admin-cluster";
import {
  WebserverWorkspace,
  type WebserverLocale,
  type WebserverResourceDataSource,
  type WebserverResourceRegistry,
} from "@sdkwork/webserver-pc-commons";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(cleanup);

/**
 * The cluster list is a summary table with one expandable row per cluster.
 *
 * Two design decisions are pinned here, because both invert an earlier shape of
 * the page and are easy to regress without noticing:
 *
 * 1. A cluster's remaining fields are read by expanding its row, not by adding
 *    columns to the table. The row therefore carries the fleet summary only —
 *    thresholds, balancing strategy, served domains and timestamps are in the
 *    detail.
 * 2. Clicking a row expands it. It does not select it, and selecting one does
 *    not raise a "Selected rows" strip above the table: with the record's own
 *    operations inside its detail row, a bar that only restates the count
 *    offers the operator nothing.
 *
 * Matchers are written as plain DOM assertions: this app's vitest setup does not
 * load jest-dom.
 */

const CLUSTERS = [
  {
    id: "clu-1",
    name: "生产集群",
    code: "prod",
    description: "primary cluster",
    status: 1,
    heartbeatIntervalSeconds: 15,
    offlineThresholdSeconds: 60,
    hostCount: "2",
    instanceCount: "3",
    onlineInstanceCount: "2",
    lbStrategy: "round_robin",
    servedDomains: ["a.example.com", "b.example.com", "c.example.com", "d.example.com"],
    createdAt: "2026-09-01T02:00:00Z",
    updatedAt: "2026-09-02T03:30:00Z",
  },
  {
    id: "clu-2",
    name: "灰度集群",
    code: "canary",
    status: 0,
    heartbeatIntervalSeconds: 30,
    offlineThresholdSeconds: 120,
    hostCount: "1",
    instanceCount: "1",
    onlineInstanceCount: "0",
    lbStrategy: "least_connections",
    servedDomains: [],
    createdAt: "2026-09-03T02:00:00Z",
    updatedAt: "2026-09-03T02:00:00Z",
  },
];

/** Cluster registry source: the real action set, a stub transport. */
function clusterSource(options: { onDelete?: () => void } = {}): WebserverResourceDataSource {
  const executed = vi.fn(async () => ({}));
  return {
    actions: [
      { bodyTemplate: { code: "", name: "" }, execute: executed, id: "create", label: "Create cluster" },
      { bodyTemplate: { description: "" }, execute: executed, id: "update", label: "Update", requiresSelection: true },
      {
        bodyTemplate: {},
        dangerous: true,
        execute: async () => {
          options.onDelete?.();
          return {};
        },
        id: "delete",
        label: "Delete cluster",
        requiresSelection: true,
      },
    ],
    load: async () => ({
      items: CLUSTERS,
      pageInfo: { hasMore: false, mode: "offset", page: 1, pageSize: 20, total: CLUSTERS.length },
    }),
  };
}

function renderClusterList(locale: WebserverLocale, registry?: WebserverResourceRegistry) {
  return render(
    <MemoryRouter initialEntries={["/admin/cluster/clusters"]}>
      <Routes>
        <Route
          path="/admin/*"
          element={(
            <WebserverWorkspace
              locale={locale}
              modules={[clusterModule]}
              permissionScope={["web.cluster.read", "web.cluster.write"]}
              registry={registry ?? { "cluster-clusters": clusterSource() }}
              surface="backend-admin"
              userLabel="ops@example.test"
            />
          )}
        />
      </Routes>
    </MemoryRouter>,
  );
}

function tableHeaderLabels(container: HTMLElement): string[] {
  const table = container.querySelector("table");
  if (!table) throw new Error("cluster table not rendered");
  return within(table)
    .getAllByRole("columnheader")
    .map((cell) => cell.textContent?.trim() ?? "");
}

function commandbarButton(container: HTMLElement, name: string): HTMLButtonElement {
  const bar = container.querySelector(".resource-commandbar");
  if (!bar) throw new Error("resource command bar not rendered");
  return within(bar as HTMLElement).getByRole("button", { name }) as HTMLButtonElement;
}

function expandedDetail(container: HTMLElement): HTMLElement {
  const detail = container.querySelector('[data-slot="data-table-expanded-row"]');
  if (!detail) throw new Error("row detail not rendered");
  return detail as HTMLElement;
}

function row(container: HTMLElement, rowId: string): HTMLElement {
  const element = container.querySelector(`[data-sdk-row-id="${rowId}"]`);
  if (!element) throw new Error(`row ${rowId} not rendered`);
  return element as HTMLElement;
}

describe("cluster list row detail", () => {
  it("keeps the collapsed row a summary and leaves the rest of the record out of the columns", async () => {
    const { container } = renderClusterList("zh-CN");
    await screen.findByText("生产集群");

    // Selection checkbox, disclosure control, then the fleet summary. The
    // disclosure column carries a screen-reader-only header, exactly like the
    // select-all column beside it.
    expect(tableHeaderLabels(container)).toEqual([
      "",
      "详情",
      "集群",
      "集群编码",
      "状态",
      "实例数",
      "在线实例",
      "主机数",
    ]);
    // Moved into the detail row, so they are columns no longer.
    expect(tableHeaderLabels(container)).not.toContain("心跳间隔（秒）");
    expect(tableHeaderLabels(container)).not.toContain("负载均衡策略");
  });

  it("reveals the record's remaining fields when its row is clicked", async () => {
    const { container } = renderClusterList("zh-CN");
    await screen.findByText("生产集群");

    expect(container.querySelector('[data-slot="data-table-expanded-row"]')).toBeNull();

    fireEvent.click(row(container, "clu-1"));

    const detail = expandedDetail(container);
    // The whole field set the row leaves out, in plan order.
    expect(Array.from(detail.querySelectorAll("dt")).map((term) => term.textContent)).toEqual([
      "负载均衡策略",
      "心跳间隔（秒）",
      "离线判定阈值（秒）",
      "承载域名",
      "描述",
      "ID",
      "创建时间",
      "更新时间",
    ]);
    // Codes are read as words, and a list is spelled out in full here: the row
    // truncates it, the detail is opened to read the whole record.
    expect(within(detail).getByText("轮询")).not.toBeNull();
    expect(within(detail).getByText("a.example.com, b.example.com, c.example.com, d.example.com")).not.toBeNull();
    expect(within(detail).getByText("15")).not.toBeNull();
    // Only the clicked record opens.
    expect(screen.queryByText("最少连接")).toBeNull();

    fireEvent.click(row(container, "clu-1"));

    expect(container.querySelector('[data-slot="data-table-expanded-row"]')).toBeNull();
  });

  it("offers the record's own operations inside the detail, without a prior selection", async () => {
    const onDelete = vi.fn();
    const registry = {
      "cluster-clusters": clusterSource({ onDelete }),
    } as WebserverResourceRegistry;
    const { container } = renderClusterList("zh-CN", registry);
    await screen.findByText("生产集群");

    // Nothing is selected, so the toolbar cannot delete anything yet.
    expect(commandbarButton(container, "删除集群").disabled).toBe(true);

    fireEvent.click(row(container, "clu-1"));

    const detail = expandedDetail(container);
    expect(within(detail).getByRole("button", { name: "更新" })).not.toBeNull();
    fireEvent.click(within(detail).getByRole("button", { name: "删除集群" }));

    // The operation opens against the expanded row, not against a stale selection.
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("删除集群")).not.toBeNull();
    expect(onDelete).not.toHaveBeenCalled();
  });

  it("does not select the row it expands", async () => {
    const { container } = renderClusterList("zh-CN");
    await screen.findByText("生产集群");

    fireEvent.click(row(container, "clu-1"));

    expect(expandedDetail(container)).not.toBeNull();
    expect(screen.queryByText("Selected rows")).toBeNull();
    // The row is expanded, not selected: the delete action still needs a row.
    expect(commandbarButton(container, "删除集群").disabled).toBe(true);
  });

  it("shows no selected-rows strip when a row is selected", async () => {
    const { container } = renderClusterList("zh-CN");
    await screen.findByText("生产集群");

    const checkbox = screen.getByRole("checkbox", { name: "Select row clu-1" });
    fireEvent.click(checkbox);

    // The checkbox is what carries the selection, and the toolbar reacts to it.
    expect(checkbox.getAttribute("aria-checked")).toBe("true");
    expect(commandbarButton(container, "删除集群").disabled).toBe(false);
    expect(container.querySelector('[data-sdk-ui="bulk-action-bar"]')).toBeNull();
    expect(screen.queryByText("Selected rows")).toBeNull();
  });

  it("keeps the expandable-row affordance localised on both locales", async () => {
    const zh = renderClusterList("zh-CN");
    await screen.findByText("生产集群");
    expect(screen.getByRole("button", { name: "展开 生产集群 的详情" }).getAttribute("aria-expanded")).toBe("false");
    zh.unmount();

    renderClusterList("en-US");
    await screen.findByText("生产集群");
    expect(screen.getByRole("button", { name: "Show details of 生产集群" }).getAttribute("aria-expanded"))
      .toBe("false");
  });
});
