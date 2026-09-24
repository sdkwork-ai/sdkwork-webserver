// @vitest-environment jsdom

import {
  MyPluginsPage,
  PluginsLocaleProvider,
  resolvePluginCatalogStorageKey,
  serializePluginCatalog,
} from "@sdkwork/webserver-pc-console-plugins";
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

/**
 * The record shape the page's own catalog writer accepts. The package exports
 * `serializePluginCatalog` rather than the record type, so the fixture derives
 * the type from the writer instead of reaching into the package's internals —
 * a fixture that drifted from the real record would fail here, not in the page.
 */
type PluginRecord = Parameters<typeof serializePluginCatalog>[0][number];

/**
 * Where the plugin page's filter card sits relative to the table's frame.
 *
 * Not a geometry test: jsdom has no layout, so it cannot see the 1px doubled
 * outline this guards against. What it *can* see is the composition that produced
 * it — the card used to be the first **child** of `.data-surface`, a frame that
 * carries no padding, so its border landed on the frame's own border; and being a
 * flex/grid child of the frame it also spent its own 573px height out of the
 * table's pane, leaving a twenty-row table 102px.
 *
 * The real-browser numbers for both symptoms live on the page's own comment and
 * in `theme-style-contract.test.ts`; this file is the part a build can fail on,
 * and the mutation it has to catch is "put the filter bar back inside the frame".
 */
const OWNER = "plugins-frame-test";

const pluginRow = (index: number): PluginRecord => ({
  id: `plugin-frame-${index}`,
  ownerKey: OWNER,
  pluginKey: `frame-plugin-${index}`,
  displayName: `Frame Plugin ${index}`,
  summary: "Fixture plugin for the list-frame contract",
  version: "1.0.0",
  categoryId: "cat-development",
  supportedHostTools: ["codex"],
  contributedCapabilities: ["tools"],
  sourceKind: "git",
  gitRepository: "https://github.com/sdkwork-ai/frame-plugin",
  gitRef: "main",
  artifactRef: null,
  checksumSha256: null,
  archiveFileName: null,
  status: "active",
  createdAt: "2026-09-01T00:00:00Z",
  updatedAt: "2026-09-24T00:00:00Z",
});

function renderPage() {
  return render(
    <PluginsLocaleProvider locale="zh-CN">
      {/* `drive` is only read by the upload handlers, which these cases never
          reach; the catalog itself comes from browser storage. */}
      <MyPluginsPage drive={{} as never} ownerKey={OWNER} variant="console" />
    </PluginsLocaleProvider>,
  );
}

const frame = () => document.body.querySelector<HTMLElement>(".data-surface");
const filterCard = () => document.body.querySelector<HTMLElement>(".plugin-filter-bar");

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  cleanup();
});

describe("plugin page list frame", () => {
  it("keeps the filter card out of the table's frame", async () => {
    localStorage.setItem(
      resolvePluginCatalogStorageKey(OWNER),
      serializePluginCatalog([pluginRow(1), pluginRow(2), pluginRow(3)]),
    );
    renderPage();

    await waitFor(() => expect(filterCard()).not.toBeNull());
    const surface = frame();
    expect(surface).not.toBeNull();
    // The whole defect, in one assertion: the card is a sibling above the frame,
    // not a child inside it.
    expect(surface?.contains(filterCard() as Node)).toBe(false);
    expect(surface?.querySelector('[data-slot="data-table"]')).not.toBeNull();
  });

  it("offers BirdCoder among the agent-tool filter options", async () => {
    localStorage.setItem(
      resolvePluginCatalogStorageKey(OWNER),
      serializePluginCatalog([pluginRow(1)]),
    );
    renderPage();

    await waitFor(() => expect(filterCard()).not.toBeNull());
    const labels = [
      ...(filterCard() as HTMLElement).querySelectorAll(".plugin-tool-option-label"),
    ].map((node) => node.textContent);
    // The picker is catalog-driven, so a host tool only shows up here when it is
    // registered in PLUGIN_HOST_TOOL_IDS *and* resolved by the zh-CN catalog.
    expect(labels).toContain("BirdCoder");
    expect(new Set(labels).size).toBe(labels.length);
  });

  it("still frames the table when the catalog is empty", async () => {
    renderPage();

    await waitFor(() => expect(frame()?.querySelector('[data-slot="data-table"]')).not.toBeNull());
    // An empty catalog renders no filter card at all, which is what kept every
    // empty-list measurement of this page looking correct while the populated one
    // was not — the card only appears once there are rows to filter.
    expect(filterCard()).toBeNull();
  });
});
