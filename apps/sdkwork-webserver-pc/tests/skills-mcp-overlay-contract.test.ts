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

  it("creates a plugin through a two-step wizard that picks agent tools first", () => {
    const createForm = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-console-plugins/src/CreatePluginForm.tsx",
    );
    // Step 1 is the agent-tool picker and it must gate step 2.
    expect(createForm).toContain("plugin-wizard-steps");
    expect(createForm).toContain("PluginHostToolMultiSelect");
    expect(createForm).toMatch(/supportedHostTools\.length === 0[\s\S]{0,220}setStep\(1\)/);
    // Category is required on the record the wizard persists.
    expect(createForm).toContain("PluginCategorySelect");
    expect(createForm).toContain("categoryId");
    expect(createForm).not.toContain("登记插件");
  });

  it("curates plugin categories from a dedicated admin page", () => {
    const categoriesPage = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-console-plugins/src/PluginCategoriesAdminPage.tsx",
    );
    const adminModule = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-plugins/src/module.ts",
    );
    const adminSurface = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-plugins/src/PluginsAdminSurface.tsx",
    );
    const workspace = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/src/surfaces/WebserverAuthorizedWorkspace.tsx",
    );
    expect(categoriesPage).toContain("SurfaceDrawer");
    expect(categoriesPage).toContain("ConfirmModal");
    expect(categoriesPage).toContain("skills-console-primary");
    expect(adminModule).toContain("plugin-categories");
    expect(adminSurface).toContain("PluginCategoriesAdminPage");
    expect(workspace).toContain("plugin-categories");
  });

  it("scopes the plugin catalog to the IAM subject on every surface", () => {
    const consoleSurface = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-console-plugins/src/PluginsConsoleSurface.tsx",
    );
    const plugins = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-console-plugins/src/MyPluginsPage.tsx",
    );
    const workspace = source(
      "sdkwork-webserver/apps/sdkwork-webserver-pc/src/surfaces/WebserverAuthorizedWorkspace.tsx",
    );
    expect(consoleSurface).toContain("ownerKey");
    expect(plugins).toContain("filterPluginRecordsByOwner");
    expect(plugins).toMatch(/savePluginCatalog\(owner/);
    expect(workspace).toMatch(/ownerKey=\{operatorId\}/);
  });

  it("makes a managed category mandatory in the skills console create/edit forms", () => {
    const createForm = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-console-skills/src/components/CreateSkillForm.tsx",
    );
    const editForm = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-console-skills/src/components/EditSkillForm.tsx",
    );
    // Both forms consume the managed picker rather than a free-text input.
    expect(createForm).toContain("SkillCategorySelect");
    expect(editForm).toContain("SkillCategorySelect");
    expect(createForm).toContain("useSkillCategories");
    expect(editForm).toContain("useSkillCategories");
    // Required-category gate on submit AND on the submit button.
    expect(createForm).toContain("create.error.categoryRequired");
    expect(editForm).toContain("edit.error.categoryRequired");
    // Both forms must gate the primary action on the category being chosen.
    expect(createForm).toMatch(/disabled=\{[\s\S]{0,80}isBlank\(trim\(form\.categoryCode\)\)/);
    expect(editForm).toMatch(/disabled=\{[\s\S]{0,80}isBlank\(trim\(form\.categoryCode\)\)/);
    // The old comma-separated free-text category field must be gone.
    expect(createForm).not.toContain("form.categories");
    expect(editForm).not.toContain("form.categories.split");
  });

  it("makes a managed category mandatory in the mcp console register/edit forms", () => {
    const registerForm = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-console-mcp/src/components/RegisterMcpServerForm.tsx",
    );
    const editForm = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-console-mcp/src/components/EditMcpServerForm.tsx",
    );
    expect(registerForm).toContain("McpCategorySelect");
    expect(editForm).toContain("McpCategorySelect");
    expect(registerForm).toContain("useMcpCategories");
    expect(editForm).toContain("useMcpCategories");
    expect(registerForm).toContain("register.error.categoryRequired");
    expect(editForm).toContain("edit.error.categoryRequired");
    // category_code is now sent unconditionally (mandatory), not guarded by trim().
    expect(registerForm).toContain("category_code: trim(form.categoryCode)");
    expect(editForm).toContain("category_code: trim(form.categoryCode)");
    expect(registerForm).not.toMatch(/trim\(form\.categoryCode\) \? \{ category_code/);
    expect(editForm).not.toMatch(/trim\(form\.categoryCode\) \? \{ category_code/);
  });

  it("reads the category dictionary from each module's own core package", () => {
    // High cohesion: the console consumes its own module's app-api binding; the
    // host never re-declares the wire contract.
    const skillsHook = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-console-skills/src/hooks/useSkillCategories.ts",
    );
    const mcpCore = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-core/src/services/marketplaceService.ts",
    );
    const mcpHook = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-console-mcp/src/hooks/useMcpCategories.ts",
    );
    expect(skillsHook).toContain("listSkillCategories");
    expect(skillsHook).toContain("@sdkwork/skills-pc-core");
    expect(mcpCore).toContain("listPublishedMcpCategories");
    expect(mcpCore).toContain("clients.app.mcp.categories.list");
    expect(mcpHook).toContain("@sdkwork/mcp-pc-core");
  });

  it("keeps skills admin category maintenance able to update existing categories", () => {
    const adminPage = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-admin-skill/src/pages/SkillCategoriesPage.tsx",
    );
    const adminCore = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-admin-core/src/services/skillsBackendService.ts",
    );
    expect(adminCore).toContain("updateSkillCategory");
    // The update contract carries no `code`: category codes stay immutable.
    expect(adminCore).toMatch(
      /UpdateSkillCategoryCommand[\s\S]{0,200}skillCategories\.update/,
    );
    expect(adminPage).toContain("updateSkillCategory");
    expect(adminPage).toContain("editTarget");
    // `version` is required for optimistic concurrency and must come from the
    // record, never from form state.
    expect(adminPage).toMatch(/version:\s*target\.version/);
    // The code is immutable server side, so the form must render it read-only
    // rather than accepting an edit the server would silently drop.
    expect(adminPage).toMatch(/value=\{editTarget\.code\}[\s\S]{0,40}readOnly/);
  });

  it("curates both skills taxonomy branches rather than one flat list", () => {
    const adminPage = source(
      "sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-admin-skill/src/pages/SkillCategoriesPage.tsx",
    );
    // `categoryType` is a two-value enum on the write contract; a single flat
    // list would let an operator file a collection where a market category
    // belongs (or vice versa). Asserting the strings merely *appear* is not
    // enough — the active tab must actually drive the filter, so pin the
    // filter expression itself.
    expect(adminPage).toMatch(
      /\.filter\(\(item\) => \(item\.categoryType \?\? 'skill_market'\) === activeType\)/,
    );
    expect(adminPage).toContain("const CATEGORY_TYPES = ['skill_market', 'skills_collection'] as const");
    // Switching tab must re-point the active taxonomy.
    expect(adminPage).toMatch(/onClick=\{\(\) => setActiveType\(type\)\}/);
    // A new category defaults to the taxonomy the operator is looking at, so
    // creating from the Collection tab cannot silently produce a Market entry.
    expect(adminPage).toMatch(/setCreateForm\(\{ \.\.\.EMPTY_CREATE, categoryType: activeType \}\)/);
    // The page reuses the admin-core derivation instead of re-spelling the
    // permission string, so what it creates is what the console resolves.
    expect(adminPage).toContain("packageManagePermissionForCategory");
    expect(adminPage).not.toMatch(/`skills\.packages\.manage\.\$\{/);
  });

  it("models mcp category identity as the code, matching the upsert contract", () => {
    const adminPage = source(
      "sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-admin/src/pages/AdminCategoriesPage.tsx",
    );
    // The write command carries no id — identity is the code, so the page must
    // key its save semantics off the code and say so.
    expect(adminPage).toContain("category_code");
    expect(adminPage).toContain("upsertAdminCategory");
    expect(adminPage).toMatch(/isOverwrite/);
    // `lifecycle_status` is absent from the write command: server-managed, so
    // it is displayed but never submitted. A windowed "not.toMatch" over the
    // call site is toothless here — the payload literal is longer than any sane
    // window, so an injected field survives. Bind the payload instead: extract
    // the object literal handed to upsertAdminCategory and assert its exact key
    // set, which fails the moment any extra field is smuggled in.
    expect(adminPage).toContain("lifecycle_status");
    const payload = adminPage.match(
      /upsertAdminCategory\(clients,\s*\{([\s\S]*?)\n\s*\}\);/,
    );
    expect(payload, "upsert payload literal must be extractable").not.toBeNull();
    const payloadKeys = [...payload![1].matchAll(/^\s*([a-z_]+):/gm)].map((m) => m[1]);
    expect(payloadKeys).toEqual([
      "category_code",
      "name",
      "description",
      "sort_order",
      "parent_id",
      "icon_ref",
    ]);
    // Hierarchy is part of the contract on both sides of the tree.
    expect(adminPage).toContain("parent_id");
  });
});
