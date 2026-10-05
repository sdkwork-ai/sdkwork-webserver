// @vitest-environment jsdom

import {
  groupMenuEntries,
  hasWebserverPermission,
  MENU_SECTIONS,
  translateWebserver,
  type WebserverLocale,
} from "@sdkwork/webserver-pc-commons";
import { describe, expect, it } from "vitest";
import {
  adminModules,
  appTemplatesAdminResourceRenderers,
  consoleModules,
  marketplaceConsoleResourceRenderers,
} from "../src/surfaces/WebserverAuthorizedWorkspace.tsx";

/**
 * The app-template marketplace is the second bridged sdkwork-deployments
 * family: the console browses and acquires (`marketplace`, `my-templates`)
 * while the operations surface moderates the catalog (`template-categories`,
 * `app-templates`, `app-template-versions`). Like the IAM module's contract,
 * these assertions pin the wiring an operator sees immediately — mounted
 * modules, a renderer behind every menu entry, one sidebar section, and the
 * permission codes that keep the operations entries reachable.
 *
 * `consoleModules`, `adminModules`, and the two renderer groups are imported
 * from the host surface rather than restated: the failure this guards against
 * is exactly a module that was implemented and tested but never mounted.
 */

const LOCALES: readonly WebserverLocale[] = ["zh-CN", "en-US"];

const CONSOLE_RESOURCES = ["marketplace", "my-templates"] as const;
const ADMIN_RESOURCES = ["template-categories", "app-templates", "app-template-versions"] as const;

const marketplaceHostModule = consoleModules.find((module) => module.id === "marketplace");
const appTemplatesHostModule = adminModules.find((module) => module.id === "app-templates");

describe("app-template marketplace module registration", () => {
  it("mounts the console marketplace module with both entries behind renderers", () => {
    expect(marketplaceHostModule).toBeDefined();
    expect(marketplaceHostModule?.surface).toBe("app-console");
    expect(marketplaceHostModule?.entries.map((entry) => entry.resource).sort()).toEqual(
      [...CONSOLE_RESOURCES].sort(),
    );

    const renderers = marketplaceConsoleResourceRenderers({
      deployBaseUrl: "https://edge.example.test",
      driveBaseUrl: "https://edge.example.test",
      locale: "zh-CN",
      tokenManager: {} as Parameters<typeof marketplaceConsoleResourceRenderers>[0]["tokenManager"],
    });
    expect(Object.keys(renderers).sort()).toEqual([...CONSOLE_RESOURCES].sort());
  });

  it("mounts the operations catalog module with all three entries behind renderers", () => {
    expect(appTemplatesHostModule).toBeDefined();
    expect(appTemplatesHostModule?.surface).toBe("backend-admin");
    expect(appTemplatesHostModule?.entries.map((entry) => entry.resource).sort()).toEqual(
      [...ADMIN_RESOURCES].sort(),
    );

    const renderers = appTemplatesAdminResourceRenderers({
      backendApiBaseUrl: "https://edge.example.test",
      locale: "zh-CN",
      tokenManager: {} as Parameters<typeof appTemplatesAdminResourceRenderers>[0]["tokenManager"],
    });
    expect(Object.keys(renderers).sort()).toEqual([...ADMIN_RESOURCES].sort());
  });

  it("carries the deployments permission codes the catalog moderates under", () => {
    expect(marketplaceHostModule?.entries.map((entry) => entry.permission)).toEqual([
      "deploy.marketplaceTemplates.read",
      "deploy.appTemplates.read",
    ]);
    expect(appTemplatesHostModule?.entries.map((entry) => entry.permission)).toEqual([
      "deploy.templateCategories.read",
      "deploy.appTemplates.read",
      "deploy.appTemplateVersions.read",
    ]);
    // An operations entry whose permission is missing from the admin entry-gate
    // list can never open `/admin` at all — the module would be permanently
    // filtered out of the operator's menu.
    for (const permission of appTemplatesHostModule?.entries.map((entry) => entry.permission) ?? []) {
      expect(hasWebserverPermission([permission], permission)).toBe(true);
      expect(hasWebserverPermission([], permission)).toBe(false);
    }
  });

  it("labels every resource on both locales through the shared catalog", () => {
    for (const locale of LOCALES) {
      for (const resource of [...CONSOLE_RESOURCES, ...ADMIN_RESOURCES]) {
        const label = translateWebserver(locale, `resource.${resource}.label` as Parameters<typeof translateWebserver>[1]);
        expect(label, `${locale} ${resource}.label`).not.toMatch(/^resource\./);
      }
      // Only the operations trio renders on the backend-admin surface, so only
      // its entries carry the admin descriptive copy.
      for (const resource of ADMIN_RESOURCES) {
        const adminLabel = translateWebserver(locale, `resource.${resource}.admin.label` as Parameters<typeof translateWebserver>[1]);
        expect(adminLabel, `${locale} ${resource}.admin.label`).not.toMatch(/^resource\./);
      }
    }
  });

  it("groups the whole family under the template-market sidebar section", () => {
    const section = MENU_SECTIONS.find((candidate) => candidate.id === "templateMarket");
    expect(section?.labelKey).toBe("menuSection.templateMarket");
    expect(section?.resources).toEqual([
      "marketplace",
      "my-templates",
      "template-categories",
      "app-templates",
      "app-template-versions",
    ]);

    // The section is a grouping, not a route: on a console-shaped menu it
    // claims exactly the two console entries, and the heading differs from
    // every label it holds (the sidebar draws the heading and the links).
    const consoleEntries = (marketplaceHostModule?.entries ?? []).map((entry) => ({
      resource: entry.resource,
    }));
    const consoleGroups = groupMenuEntries(consoleEntries);
    const consoleSection = consoleGroups.find((group) => group.id === "templateMarket");
    expect(consoleSection?.entries.map((entry) => entry.resource).sort()).toEqual(
      [...CONSOLE_RESOURCES].sort(),
    );
  });
});
