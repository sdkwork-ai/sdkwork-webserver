import { webserverModule as deploymentsModule } from "@sdkwork/webserver-pc-console-deployments";
import { webserverModule as configurationModule } from "@sdkwork/webserver-pc-console-site-configuration";
import { webserverModule as sitesModule } from "@sdkwork/webserver-pc-console-sites";
import { WebserverConsoleShell } from "@sdkwork/webserver-pc-console-shell";
import type { WebserverResourceDataSource, WebserverResourceRegistry } from "@sdkwork/webserver-pc-commons";
import { SdkworkThemeProvider } from "@sdkwork/ui-pc-react/theme";
import { createRoot } from "react-dom/client";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import "../../src/index.css";

const applications: readonly Record<string, unknown>[] = [
  {
    id: "app-customer-portal",
    name: "Customer portal",
    environment: "Production",
    status: "Healthy",
    primaryDomain: "portal.example.com",
    updatedAt: "2026-07-28 15:42",
  },
  {
    id: "app-partner-api",
    name: "Partner API",
    environment: "Staging",
    status: "Deploying",
    primaryDomain: "api-staging.example.com",
    updatedAt: "2026-07-28 15:38",
  },
  {
    id: "app-docs",
    name: "Developer documentation",
    environment: "Production",
    status: "Healthy",
    primaryDomain: "docs.example.com",
    updatedAt: "2026-07-28 14:56",
  },
];

const emptySource: WebserverResourceDataSource = {
  actions: [],
  async load(query) {
    return { items: [], pageInfo: { page: query.page, pageSize: query.pageSize, hasMore: false, total: 0 } };
  },
};

const registry: WebserverResourceRegistry = {
  applications: {
    actions: [{
      id: "create",
      label: "Create application",
      bodyTemplate: {
        name: "",
        description: "",
        applicationType: "WEB",
        siteType: 1,
        environment: "production",
        versionTag: "v1.0.0",
        shortDescription: "",
        fullDescription: "",
        releaseNotes: "",
        category: "",
        keywords: "",
        supportUrl: "",
        privacyPolicyUrl: "",
        officialWebsiteUrl: "",
      },
      applicationSubmission: "create",
      execute: async () => ({}),
      fieldOptions: {
        applicationType: ["WEB", "API"],
        siteType: [1, 2, 3, 4, 5, 6],
        environment: ["production", "staging", "test", "development"],
      },
      permission: "web.sites.write",
      requiredFields: ["name", "versionTag"],
      sourceInput: "archive-directory-or-git",
    }, {
      id: "update",
      label: "Edit",
      bodyTemplate: {
        name: "",
        description: "",
        shortDescription: "",
        fullDescription: "",
        releaseNotes: "",
        category: "",
        keywords: "",
        supportUrl: "",
        privacyPolicyUrl: "",
        officialWebsiteUrl: "",
      },
      applicationSubmission: "update",
      execute: async () => ({}),
      permission: "web.sites.write",
      requiresSelection: true,
    }, {
      id: "update-source",
      label: "Update code",
      bodyTemplate: { versionTag: "" },
      execute: async () => ({}),
      loadSourceInputDefaults: async () => ({
        mode: "git",
        repository: "https://github.com/sdkwork/customer-portal.git",
      }),
      permission: "web.sites.write",
      requiredFields: ["versionTag"],
      requiresSelection: true,
      sourceInput: "archive-directory-or-git",
    }, {
      id: "publish",
      label: "Publish",
      bodyTemplate: {
        deployType: 1,
        sourceVersionId: "",
        environment: "production",
        versionTag: "",
      },
      execute: async () => ({}),
      fieldOptions: {
        deployType: [1],
        sourceVersionId: [],
        environment: ["production", "staging", "test", "development"],
      },
      loadFieldOptions: async () => ({
        sourceVersionId: [{
          label: "v1.4.0 · ARCHIVE",
          relatedValues: { versionTag: "v1.4.0" },
          value: "source-version-v1-4-0",
        }],
      }),
      permission: "web.sites.write",
      readOnlyFields: ["versionTag"],
      requiredFields: ["sourceVersionId", "versionTag"],
      requiresConfirmation: true,
      requiresSelection: true,
    }, {
      id: "delete",
      label: "Delete",
      bodyTemplate: {},
      dangerous: true,
      execute: async () => ({}),
      permission: "web.sites.write",
      requiresSelection: true,
    }],
    async load(query) {
      return {
        items: applications,
        pageInfo: { page: query.page, pageSize: query.pageSize, hasMore: false, total: applications.length },
      };
    },
  },
  configuration: emptySource,
  deployments: {
    actions: [{
      id: "deploy",
      label: "Deploy",
      bodyTemplate: {
        deployType: 1,
        environment: "production",
        versionTag: "v1.4.0",
      },
      execute: async () => ({}),
      fieldOptions: {
        deployType: [1],
        environment: ["production", "staging", "test", "development"],
      },
      permission: "web.sites.write",
      requiredFields: ["versionTag"],
      requiresConfirmation: true,
      requiresScope: true,
      sourceInput: "archive-directory-or-git",
    }, {
      id: "rollback",
      label: "Restore this version",
      bodyTemplate: {},
      execute: async () => ({}),
      availableWhen: ({ selectedItem }) => Number(selectedItem?.status) === 2,
      permission: "web.sites.write",
      requiresConfirmation: true,
      requiresScope: true,
      requiresSelection: true,
    }],
    async load(query) {
      return {
        items: [
          {
            id: "deployment-v1-3-0",
            versionTag: "v1.3.0",
            environment: "production",
            status: 2,
            artifactHash: "71ecbe6a3898cc64b5ad6f952f1251b5f5dc91bb51438e61e3f0d2fc45f8d9e2",
            createdAt: "2026-07-28T15:42:00Z",
            completedAt: "2026-07-28T15:42:18Z",
            durationMs: "18000",
          },
          {
            id: "restore-v1-2-1",
            versionTag: "v1.2.1",
            environment: "production",
            status: 0,
            rollbackFromDeploymentId: "deployment-v1-2-1",
            artifactHash: "945d5df8a102c18a57f132f4f8b30cd1feab44e6023a6cebbc3a0dc8bfe5fefd",
            createdAt: "2026-07-28T15:39:00Z",
          },
        ],
        pageInfo: { page: query.page, pageSize: query.pageSize, hasMore: false, total: 2 },
      };
    },
    requiresScope: true,
    scopeKind: "application",
  },
};

const root = document.getElementById("root");
if (!root) throw new Error("visual fixture root is required");
const defaultTheme = new URLSearchParams(window.location.search).get("theme") === "dark" ? "dark" : "light";
const initialPath = new URLSearchParams(window.location.search).get("view") === "deployments"
  ? "/console/deployments"
  : "/console/sites";

createRoot(root).render(
  <SdkworkThemeProvider className="webserver-pc-theme" defaultTheme={defaultTheme} locale="en-US" themeColor="green-tech">
    <MemoryRouter initialEntries={[initialPath]}>
      <Routes>
        <Route
          path="/console/*"
          element={(
            <WebserverConsoleShell
              locale="en-US"
              modules={[sitesModule, configurationModule, deploymentsModule]}
              notificationsHref="/notifications"
              onSignOut={() => undefined}
              permissionScope={["web.sites.*", "web.certificates.*"]}
              portalHref="/"
              registry={registry}
              userLabel="Alex Morgan"
            />
          )}
        />
      </Routes>
    </MemoryRouter>
  </SdkworkThemeProvider>,
);
