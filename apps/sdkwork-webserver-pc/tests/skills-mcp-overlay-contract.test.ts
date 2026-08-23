import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const workspaceRoot = resolve(fileURLToPath(new URL("../../../../", import.meta.url)));

function source(relativePath: string): string {
  return readFileSync(resolve(workspaceRoot, relativePath), "utf8");
}

describe("skills and mcp list CRUD overlays", () => {
  it("opens console create/edit in drawers and delete in a confirm modal", () => {
    const skills = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-console-skills/src/pages/MySkillsPage.tsx",
    );
    const mcp = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-console-mcp/src/pages/MyMcpServersPage.tsx",
    );
    expect(skills).toContain("SurfaceDrawer");
    expect(skills).toContain("table-frame");
    expect(skills).toContain("empty-state");
    expect(skills).toContain("ConfirmModal");
    expect(mcp).toContain("SurfaceDrawer");
    expect(mcp).toContain("empty-state");
    expect(mcp).toContain("skills-console-primary");
    expect(mcp).toContain("ConfirmModal");
  });

  it("opens skills admin create/edit in drawers instead of in-page forms", () => {
    const packages = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-admin/src/index.tsx",
    );
    const artifacts = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-admin-skill/src/pages/PackageArtifactsPage.tsx",
    );
    const capabilities = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-admin-skill/src/pages/SkillCapabilitiesPage.tsx",
    );
    const updatePage = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-admin-skill/src/pages/UpdateSkillPackagePage.tsx",
    );
    expect(packages).toContain("SurfaceDrawer");
    expect(packages).toContain("ConfirmModal");
    expect(packages).toContain("Create package");
    expect(artifacts).toContain("SurfaceDrawer");
    expect(artifacts).toContain("Attach artifact");
    expect(capabilities).toContain("SurfaceDrawer");
    expect(capabilities).toContain("Create capability");
    expect(updatePage).toContain("Navigate");
    expect(updatePage).toContain("?edit=");
  });

  it("opens mcp admin create/add in drawers instead of in-page forms", () => {
    const servers = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-admin/src/pages/AdminServersPage.tsx",
    );
    const categories = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-admin/src/pages/AdminCategoriesPage.tsx",
    );
    const detail = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-admin/src/pages/AdminServerDetailPage.tsx",
    );
    const capabilities = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-admin/src/components/AdminCapabilityPanel.tsx",
    );
    expect(servers).toContain("SurfaceDrawer");
    expect(servers).toContain("ConfirmModal");
    expect(categories).toContain("SurfaceDrawer");
    expect(detail).toContain("SurfaceDrawer");
    expect(capabilities).toContain("SurfaceDrawer");
  });

  it("opens plugins console create/edit in drawers with empty primary action", () => {
    const plugins = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-console-plugins/src/MyPluginsPage.tsx",
    );
    const createForm = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-console-plugins/src/CreatePluginForm.tsx",
    );
    const host = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/src/surfaces/WebserverAuthorizedWorkspace.tsx",
    );
    expect(plugins).toContain("SurfaceDrawer");
    expect(plugins).toContain("ConfirmModal");
    expect(plugins).toContain("empty-state");
    expect(plugins).toContain("skills-console-primary");
    expect(createForm).toContain("plugin-source-toggle");
    expect(createForm).toContain("existingKeys");
    expect(host).toContain("pluginsModule");
    expect(host).toMatch(/pluginsModule,\s*skillsModule,\s*mcpModule/);
  });
});
