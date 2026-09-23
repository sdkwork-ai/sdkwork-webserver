// @vitest-environment jsdom

import { webserverModule as clusterModule } from "@sdkwork/webserver-pc-admin-cluster";
import {
  WebserverWorkspace,
  type WebserverResourceAction,
  type WebserverResourceDataSource,
  type WebserverResourceRegistry,
} from "@sdkwork/webserver-pc-commons";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";

/**
 * Column plans for the cluster instance and host lists.
 *
 * Both resources used to fall through to field-order inference, which renders
 * the first eight keys of the JSON payload in payload order: the instance list
 * began `id, clusterId, hostId, hostName, name, role, environment, processPid`
 * - every identifier and not one operational fact, so liveness, health, routing
 * and sync state never reached the page.
 *
 * The plan that replaced it had twelve default columns plus a tail of columns
 * one opt-in away in the menu. That is still a table nobody can read: it answered
 * "what does this record contain" when the page is opened to ask "which record
 * is wrong". So the plan now splits in two, and these assertions pin the split:
 *
 * - the row carries the summary an operator compares **across** records -
 *   identity, placement, and the state they triage on;
 * - everything describing **one** record opens with that record, on a click,
 *   and is deliberately not offered back as a column.
 */

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

/** One instance that exercises every composite column in the plan. */
const INSTANCE = {
  id: "11111111-1111-1111-1111-111111111111",
  clusterId: "22222222-2222-2222-2222-222222222222",
  hostId: "33333333-3333-3333-3333-333333333333",
  hostName: "edge-a",
  name: "edge-a:3800",
  role: "GATEWAY",
  environment: "production",
  processPid: 4242,
  bindHost: "0.0.0.0",
  bindPort: 3800,
  status: 1,
  healthState: "HEALTHY",
  routingEnabled: false,
  draining: true,
  ejected: false,
  joinMode: "TUNNEL",
  syncStatus: "IN_SYNC",
  qualityScore: 97,
  restartCount: 3,
  routingWeight: 200,
  buildVersion: "1.1.0",
  uptimeSeconds: 3_600,
  lastHeartbeatAt: "2026-09-21T10:00:00Z",
  processStartedAt: "2026-09-20T22:00:00Z",
  lastOnlineAt: "2026-09-21T10:00:00Z",
  publicEndpoint: "edge-a.example.test:3800",
  createdAt: "2026-09-20T10:00:00Z",
  updatedAt: "2026-09-21T10:00:00Z",
};

/** One host that exercises every composite column in the plan. */
const HOST = {
  id: "44444444-4444-4444-4444-444444444444",
  clusterId: "22222222-2222-2222-2222-222222222222",
  hostname: "edge-a",
  osName: "Ubuntu",
  osVersion: "24.04",
  arch: "x86_64",
  cpuCores: 8,
  memoryTotalMb: 16_384,
  remoteIp: "10.0.0.11",
  localIps: ["10.0.0.11", "10.0.0.12"],
  daemonVersion: "0.4.2",
  joinMode: "TUNNEL",
  status: 1,
  instanceCount: 3,
  lastHeartbeatAt: "2026-09-21T10:00:00Z",
  machineCode: "mc-9f2c1a",
  cpuModel: "AMD EPYC 7B13",
  macAddresses: ["52:54:00:aa:bb:cc"],
  kernelVersion: "6.8.0-45-generic",
  tunnelRouteDomain: "edge-a.tunnel.example.test",
  createdAt: "2026-09-19T08:00:00Z",
  updatedAt: "2026-09-21T10:00:00Z",
};

function sourceOf(
  items: Record<string, unknown>[],
  actions: readonly WebserverResourceAction[] = [],
): WebserverResourceDataSource {
  return {
    actions,
    load: async () => ({
      items,
      pageInfo: { hasMore: false, page: 1, pageSize: 20, total: items.length },
    }),
  };
}

/**
 * The instance lifecycle action set, carrying the registry's own availability
 * rules.
 *
 * Mirrored instead of imported on purpose: the assertion is that the detail
 * strip re-evaluates these predicates against **the expanded record**. A copy
 * taken from the registry would keep agreeing with a strip that quietly stopped
 * filtering, because both sides would move together.
 */
const INSTANCE_ACTIONS: WebserverResourceAction[] = [
  {
    availableWhen: ({ selectedItem }) => selectedItem?.routingEnabled === true && selectedItem.draining !== true,
    bodyTemplate: {},
    execute: async () => ({}),
    id: "cordon",
    label: "Cordon",
    requiresSelection: true,
  },
  {
    availableWhen: ({ selectedItem }) => selectedItem?.routingEnabled === false && selectedItem.draining !== true,
    bodyTemplate: {},
    execute: async () => ({}),
    id: "uncordon",
    label: "Restore routing",
    requiresSelection: true,
  },
  {
    availableWhen: ({ selectedItem }) => selectedItem?.draining !== true,
    bodyTemplate: {},
    dangerous: true,
    execute: async () => ({}),
    id: "drain",
    label: "Drain",
    requiresSelection: true,
  },
  {
    availableWhen: ({ selectedItem }) => selectedItem?.draining === true || selectedItem?.routingEnabled === false,
    bodyTemplate: {},
    execute: async () => ({}),
    id: "undrain",
    label: "Return to rotation",
    requiresSelection: true,
  },
  { bodyTemplate: {}, execute: async () => ({}), id: "probe", label: "Probe now", requiresSelection: true },
  { bodyTemplate: {}, execute: async () => ({}), id: "maintain", label: "Mark maintenance", requiresSelection: true },
  {
    availableWhen: ({ selectedItem }) => selectedItem?.status === 5,
    bodyTemplate: {},
    execute: async () => ({}),
    id: "endMaintenance",
    label: "End maintenance",
    requiresSelection: true,
  },
  { bodyTemplate: {}, dangerous: true, execute: async () => ({}), id: "delete", label: "Unregister", requiresSelection: true },
];

/** A record whose routing is cordoned but not yet draining. */
const CORDONED_INSTANCE = {
  ...INSTANCE,
  draining: false,
  id: "inst-cordoned",
  name: "edge-b:3800",
  processPid: 5150,
};

/** The host's own action set: removal is the only operation it has. */
function hostActions(onExecute?: (item: Record<string, unknown> | undefined) => void): WebserverResourceAction[] {
  return [{
    bodyTemplate: {},
    dangerous: true,
    execute: async (context) => {
      onExecute?.(context.selectedItem);
      return {};
    },
    id: "delete",
    label: "Remove host",
    requiresSelection: true,
  }];
}

function mountCluster(
  path: string,
  registry: WebserverResourceRegistry,
): { container: HTMLElement } {
  const { container } = render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route
          path="/console/*"
          element={(
            <WebserverWorkspace
              locale="en-US"
              modules={[clusterModule]}
              onSignOut={() => {}}
              permissionScope={["web.cluster.read", "web.cluster.write"]}
              portalHref="/"
              registry={registry}
              surface="app-console"
              userLabel="operator@example.test"
            />
          )}
        />
      </Routes>
    </MemoryRouter>,
  );
  return { container };
}

function headerLabels(): string[] {
  return screen.queryAllByRole("columnheader").map((cell) => cell.textContent?.trim() ?? "");
}

function row(rowId: string): HTMLElement {
  const element = document.querySelector(`[data-sdk-row-id="${rowId}"]`);
  if (!element) throw new Error(`row ${rowId} not rendered`);
  return element as HTMLElement;
}

/** The expanded detail row of the one record currently open. */
function detail(): HTMLElement {
  const element = document.querySelector('[data-slot="data-table-expanded-row"]');
  if (!element) throw new Error("row detail not rendered");
  return element as HTMLElement;
}

function detailLabels(): string[] {
  return Array.from(detail().querySelectorAll("dt")).map((term) => term.textContent ?? "");
}

/** Names of the operations the open detail offers for its own record. */
function detailActionLabels(): string[] {
  const strip = detail().querySelector(".resource-detail-actions");
  if (!strip) return [];
  return Array.from(strip.querySelectorAll("button")).map((button) => button.textContent?.trim() ?? "");
}

/** The toolbar's copy of an action, which still requires an explicit selection. */
function commandbarButton(name: string): HTMLButtonElement {
  const bar = document.querySelector(".resource-commandbar");
  if (!bar) throw new Error("resource command bar not rendered");
  return Array.from(bar.querySelectorAll("button")).find((button) => button.textContent?.trim() === name)
    ?? (() => { throw new Error(`no command bar button named ${name}`); })();
}

/** Labels the column menu offers, i.e. what the operator can put back on screen. */
function columnMenuLabels(): string[] {
  const menu = document.querySelector(".column-menu-list");
  if (!menu) throw new Error("column menu not rendered");
  return Array.from(menu.querySelectorAll("label span")).map((span) => span.textContent ?? "");
}

describe("cluster instance column plan", () => {
  const mount = () => mountCluster(
    "/console/cluster/instances",
    { "cluster-instances": sourceOf([INSTANCE]) },
  );

  it("keeps the row to the summary of the fleet", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("edge-a:3800")).toBeTruthy();
    });

    // Two control columns (select-all, disclosure) then the summary: identity,
    // placement, and the four state codes an operator triages on.
    expect(headerLabels()).toEqual([
      "",
      "Details",
      "Instance",
      "Host",
      "Listen address",
      "Role",
      "Status",
      "Health",
      "Routing",
      "Config sync",
    ]);
    // The shared field dictionary calls `name` "Application name": correct on the
    // applications table, a misdescribed column on the instance list.
    expect(headerLabels()).not.toContain("Application name");
  });

  it("composes the row's state cells instead of one column per wire field", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("edge-a:3800")).toBeTruthy();
    });

    // `bindHost` + `bindPort` are one fact.
    expect(screen.getByText("0.0.0.0:3800")).toBeTruthy();
    // `routingEnabled: false` + `draining: true` is ONE routing posture: drain
    // supersedes cordon, so a draining instance must not read as merely cordoned.
    expect(screen.getByText("Draining")).toBeTruthy();
    // Code spaces are labelled, not printed as the wire enum.
    expect(screen.getByText("In sync")).toBeTruthy();
    expect(screen.getByText("Healthy")).toBeTruthy();
    expect(screen.queryByText("IN_SYNC")).toBeNull();
  });

  it("opens the record's remaining fields when its row is clicked", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("edge-a:3800")).toBeTruthy();
    });

    expect(document.querySelector('[data-slot="data-table-expanded-row"]')).toBeNull();

    fireEvent.click(row(INSTANCE.id));

    expect(detailLabels()).toEqual([
      "Join mode",
      "Build version",
      "Environment",
      "Uptime",
      "Last heartbeat",
      "Process started",
      "Last online",
      "Quality score",
      "Restarts",
      "Routing weight",
      "PID",
      "Public endpoint",
      "Cluster",
      "Host",
      "ID",
      "Created at",
      "Updated at",
    ]);
    // Values the row never showed, rendered through the same formatters.
    expect(within(detail()).getByText("Tunnel")).toBeTruthy();
    expect(within(detail()).getByText("1.1.0")).toBeTruthy();
    expect(within(detail()).getByText("1h 0m")).toBeTruthy();
    expect(within(detail()).getByText("4242")).toBeTruthy();
    expect(within(detail()).getByText("edge-a.example.test:3800")).toBeTruthy();
    // Toggling again closes it.
    fireEvent.click(row(INSTANCE.id));
    expect(document.querySelector('[data-slot="data-table-expanded-row"]')).toBeNull();
  });

  it("does not offer the detail fields back as columns", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("edge-a:3800")).toBeTruthy();
    });

    const offered = columnMenuLabels();
    expect(offered).toContain("Config sync");
    // Re-adding a detail field as a column is the "widen the table until it
    // curls" shape the detail row exists to replace.
    for (const label of ["Build version", "Restarts", "PID", "Uptime", "Join mode"]) {
      expect(offered, `detail field offered as a column: ${label}`).not.toContain(label);
    }
  });

  it("expands without selecting, and raises no selection strip", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("edge-a:3800")).toBeTruthy();
    });

    fireEvent.click(row(INSTANCE.id));

    expect(detail()).toBeTruthy();
    expect(row(INSTANCE.id).getAttribute("data-state")).toBe("unselected");
    expect(document.querySelector('[data-sdk-ui="bulk-action-bar"]')).toBeNull();
    expect(screen.queryByText("Selected rows")).toBeNull();
  });
});

describe("cluster host column plan", () => {
  const mount = () => mountCluster(
    "/console/cluster/hosts",
    { "cluster-hosts": sourceOf([HOST]) },
  );

  it("keeps the row to the machine's own summary", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("edge-a")).toBeTruthy();
    });

    expect(headerLabels()).toEqual([
      "",
      "Details",
      "Hostname",
      "Cluster",
      "Platform",
      "Architecture",
      "Capacity",
      "Status",
      "Instances",
    ]);
  });

  it("composes platform and capacity from their wire fields", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("edge-a")).toBeTruthy();
    });

    // `osName` + `osVersion` are one fact, and so are `cpuCores` +
    // `memoryTotalMb`: one column each, not four.
    expect(screen.getByText("Ubuntu 24.04")).toBeTruthy();
    expect(screen.getByText("8 vCPU · 16 GiB")).toBeTruthy();
    expect(headerLabels()).not.toContain("OS name");
    expect(headerLabels()).not.toContain("Memory total mb");
  });

  it("opens the machine's details when its row is clicked", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("edge-a")).toBeTruthy();
    });

    fireEvent.click(row(HOST.id));

    expect(detailLabels()).toEqual([
      "Remote address",
      "Local addresses",
      "Agent version",
      "Join mode",
      "Last heartbeat",
      "Machine code",
      "CPU model",
      "MAC addresses",
      "Kernel",
      "Tunnel domain",
      "ID",
      "Created at",
      "Updated at",
    ]);
    // Address and MAC lists are spelled out in the detail; the row has no room
    // for them, which is why they are not columns.
    expect(within(detail()).getByText("10.0.0.11, 10.0.0.12")).toBeTruthy();
    expect(within(detail()).getByText("52:54:00:aa:bb:cc")).toBeTruthy();
    expect(within(detail()).getByText("mc-9f2c1a")).toBeTruthy();
  });

  it("expands without selecting, and raises no selection strip", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("edge-a")).toBeTruthy();
    });

    fireEvent.click(row(HOST.id));

    expect(detail()).toBeTruthy();
    expect(row(HOST.id).getAttribute("data-state")).toBe("unselected");
    expect(document.querySelector('[data-sdk-ui="bulk-action-bar"]')).toBeNull();
    expect(screen.queryByText("Selected rows")).toBeNull();
  });

  it("offers the machine's own removal inside the detail, without a prior selection", async () => {
    const removed: (Record<string, unknown> | undefined)[] = [];
    mountCluster(
      "/console/cluster/hosts",
      { "cluster-hosts": sourceOf([HOST], hostActions((item) => removed.push(item))) },
    );
    await waitFor(() => {
      expect(screen.getByText("edge-a")).toBeTruthy();
    });

    // Removal is row-scoped, so the toolbar's copy stays disabled until a row is
    // checked — opening the record is not checking it.
    expect(commandbarButton("Remove host").disabled).toBe(true);

    fireEvent.click(row(HOST.id));
    expect(detailActionLabels()).toEqual(["Remove host"]);

    fireEvent.click(within(detail()).getByRole("button", { name: "Remove host" }));

    // The dialog is the record's own: confirming it runs the operation against
    // the record that was opened, not against the list or a stale selection.
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("checkbox"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    await waitFor(() => {
      expect(removed).toHaveLength(1);
    });
    expect(removed[0]?.id).toBe(HOST.id);
  });
});

describe("cluster instance detail operations", () => {
  it("reads each record's posture, not the list's, when deciding what to offer", async () => {
    mountCluster(
      "/console/cluster/instances",
      { "cluster-instances": sourceOf([CORDONED_INSTANCE, INSTANCE], INSTANCE_ACTIONS) },
    );
    await waitFor(() => {
      expect(screen.getByText("edge-b:3800")).toBeTruthy();
    });

    fireEvent.click(row(CORDONED_INSTANCE.id));
    // Routing is off and no drain is underway: the way back to routing and the
    // way out of the pool are both meaningful, and so is the reverse of each.
    expect(detailActionLabels()).toEqual([
      "Restore routing",
      "Drain",
      "Return to rotation",
      "Probe now",
      "Mark maintenance",
      "Unregister",
    ]);
    fireEvent.click(row(CORDONED_INSTANCE.id));
    expect(document.querySelector('[data-slot="data-table-expanded-row"]')).toBeNull();

    fireEvent.click(row(INSTANCE.id));
    // This record is already draining: cordon and a second drain are gone, and
    // what is left is the way back. Same action list, different row, different
    // buttons - which is the whole point of filtering per record.
    expect(detailActionLabels()).toEqual([
      "Return to rotation",
      "Probe now",
      "Mark maintenance",
      "Unregister",
    ]);
  });
});

describe("cluster event column plan", () => {
  /**
   * One event of each scope.
   *
   * `ClusterEventResponse` carries `hostId` and `instanceId` as optional, so a
   * cluster-wide event has two empty identifiers. That is the shape the plan has
   * to survive: inline, those are two columns of `-` per row.
   */
  const CLUSTER_EVENT = {
    id: "evt-1",
    clusterId: "clu-1",
    severity: "WARNING",
    eventType: "CLUSTER_OFFLINE_THRESHOLD_CHANGED",
    message: "离线判定阈值由 60 秒调整为 120 秒",
    detail: { from: 60, to: 120, operatorId: "ops@example.test" },
    occurredAt: "2026-09-21T09:59:12Z",
    createdAt: "2026-09-21T09:59:12Z",
  };

  const INSTANCE_EVENT = {
    ...CLUSTER_EVENT,
    detail: { drainDeadlineSeconds: 30, remainingConnections: 12, attempts: [{ state: "retry" }] },
    eventType: "ROUTING_DRAIN_TIMEOUT",
    hostId: "host-1",
    id: "evt-2",
    instanceId: "inst-2",
    severity: "ERROR",
  };

  const mount = () => mountCluster(
    "/console/cluster/events",
    { "cluster-events": sourceOf([CLUSTER_EVENT, INSTANCE_EVENT]) },
  );

  it("reads as a timeline, not as a row of identifiers", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("ROUTING_DRAIN_TIMEOUT")).toBeTruthy();
    });

    // When / how bad / what kind / what it says. The three identifiers an event
    // points at are not part of the scan.
    expect(headerLabels()).toEqual([
      "",
      "Details",
      "Occurred at",
      "Severity",
      "Event type",
      "Message",
    ]);
    // `occurredAt` had no label anywhere, so the header was the humanized field
    // name - `occurred At`, lowercase and untranslated, on every locale.
    expect(headerLabels()).not.toContain("occurred At");
    expect(headerLabels()).not.toContain("Cluster");
    expect(headerLabels()).not.toContain("Instance");
  });

  it("opens the identifiers and the payload with the event they belong to", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("ROUTING_DRAIN_TIMEOUT")).toBeTruthy();
    });

    fireEvent.click(row(INSTANCE_EVENT.id));

    expect(detailLabels()).toEqual([
      "Cluster",
      "Host",
      "Instance",
      "Event payload",
      "ID",
      "Created at",
    ]);
    // The identifiers are the point of expanding: the row has no room, and an
    // operator copies them into another page.
    expect(within(detail()).getByText("inst-2")).toBeTruthy();
    expect(within(detail()).getByText("host-1")).toBeTruthy();
    // `detail` is a generic field name, so its wording has to come from the
    // resource: "Detail" would say nothing about what is in it.
    expect(detailLabels()).toContain("Event payload");
  });

  it("renders the nested payload as a block rather than one line of JSON", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("ROUTING_DRAIN_TIMEOUT")).toBeTruthy();
    });

    fireEvent.click(row(INSTANCE_EVENT.id));

    const payload = detail().querySelector(".resource-detail-json");
    expect(payload).toBeTruthy();
    // Indented and broken across lines: the one-line `JSON.stringify` the shared
    // formatter produces is a wall of delimiters for anything that nests.
    expect(payload?.textContent).toContain("\n");
    expect(payload?.textContent).toContain('"drainDeadlineSeconds": 30');
    expect(payload?.textContent).toContain('"state": "retry"');
    // A block takes the whole row: a ~220px grid track would shred it.
    expect(payload?.closest(".resource-detail-block")).toBeTruthy();
  });

  it("offers no operation, and no empty strip pretending otherwise", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("ROUTING_DRAIN_TIMEOUT")).toBeTruthy();
    });

    fireEvent.click(row(CLUSTER_EVENT.id));

    // Events are evidence, not a target: no action, so no action strip at all.
    expect(detail().querySelector(".resource-detail-actions")).toBeNull();
  });

  it("names the row by what happened, not by its record key", async () => {
    mount();
    await waitFor(() => {
      expect(screen.getByText("ROUTING_DRAIN_TIMEOUT")).toBeTruthy();
    });

    // An event has no name, so the identifier was the only handle left - and
    // "Show details of evt-2" tells a screen-reader user nothing they can match
    // against the list. The event type is what the row is about.
    expect(screen.getByRole("button", { name: "Show details of ROUTING_DRAIN_TIMEOUT" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: `Show details of ${INSTANCE_EVENT.id}` })).toBeNull();
  });
});
