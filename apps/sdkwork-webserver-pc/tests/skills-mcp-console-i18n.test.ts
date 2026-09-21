import { describe, expect, it } from "vitest";

import {
  formatMcpHealthLocalized,
  normalizeMcpConsoleLocale,
  translateMcpConsole,
} from "../../../../sdkwork-mcp/apps/sdkwork-mcp-pc/packages/sdkwork-mcp-pc-console-mcp/src/i18n.ts";
import {
  normalizeSkillsConsoleLocale,
  translateSkillsConsole,
} from "../../../../sdkwork-skills/apps/sdkwork-skills-pc/packages/sdkwork-skills-pc-console-skills/src/i18n.ts";
import {
  PLUGINS_MESSAGE_KEYS,
  translatePlugins,
  normalizePluginsLocale,
} from "../packages/sdkwork-webserver-pc-console-plugins/src/i18n.ts";
import { translateWebserver } from "../packages/sdkwork-webserver-pc-commons/src/i18n/index.ts";

describe("skills and mcp console i18n", () => {
  it("localizes console and admin sidebar labels", () => {
    expect(translateWebserver("zh-CN", "resource.plugins.label")).toBe("我的插件");
    expect(translateWebserver("zh-CN", "resource.plugins.admin.label")).toBe("插件管理");
    expect(translateWebserver("en-US", "resource.plugins.label")).toBe("My Plugins");
    expect(translateWebserver("zh-CN", "resource.skills.label")).toBe("我的 Skills");
    expect(translateWebserver("zh-CN", "resource.mcp.label")).toBe("我的 MCP");
    expect(translateWebserver("en-US", "resource.skills.label")).toBe("My Skills");
    expect(translateWebserver("zh-CN", "resource.skills.admin.label")).toBe("Skills 管理");
    expect(translateWebserver("zh-CN", "resource.mcp.admin.label")).toBe("MCP 管理");
  });

  it("normalizes zh* browser locales for skills and mcp without reading document.lang", () => {
    expect(normalizeSkillsConsoleLocale("zh")).toBe("zh-CN");
    expect(normalizeSkillsConsoleLocale("zh-Hans-CN")).toBe("zh-CN");
    expect(normalizeSkillsConsoleLocale(null)).toBe("en-US");
    expect(normalizeSkillsConsoleLocale("")).toBe("en-US");
    expect(normalizeMcpConsoleLocale("zh_CN")).toBe("zh-CN");
    expect(normalizeMcpConsoleLocale("en-GB")).toBe("en-US");
    expect(normalizeMcpConsoleLocale(undefined)).toBe("en-US");
  });

  it("localizes skills console page copy including create/edit field labels", () => {
    expect(translateSkillsConsole("zh-CN", "mine.title")).toBe("我的 Skills");
    expect(translateSkillsConsole("zh-CN", "mine.create")).toBe("创建 Skill 包");
    expect(translateSkillsConsole("en-US", "create.title")).toBe("Create Skill Package");
    expect(translateSkillsConsole("zh-CN", "create.created", { id: "42" })).toContain("42");
    expect(translateSkillsConsole("zh-CN", "create.field.displayName")).toBe("显示名称");
    expect(translateSkillsConsole("zh-CN", "edit.field.categories")).toBe("分类");
    expect(translateSkillsConsole("zh-CN", "mine.column.actions")).toBe("操作");
  });

  it("localizes mcp console page copy and health", () => {
    expect(translateMcpConsole("zh-CN", "mine.title")).toBe("我的 MCP");
    expect(translateMcpConsole("zh-CN", "register.submit")).toBe("注册服务器");
    expect(translateMcpConsole("zh-CN", "register.field.serverKey")).toBe("服务器标识");
    expect(translateMcpConsole("zh-CN", "edit.field.name")).toBe("显示名称");
    expect(translateMcpConsole("zh-CN", "mine.empty.action")).toBe("注册 MCP 服务器");
    expect(formatMcpHealthLocalized("en-US", "degraded")).toBe("Degraded");
  });

  it("localizes plugins console creation copy", () => {
    expect(normalizePluginsLocale("zh-Hans-CN")).toBe("zh-CN");
    expect(translatePlugins("zh-CN", "mine.title")).toBe("我的插件");
    // "登记插件" was renamed to "新增插件" across the plugin console.
    expect(translatePlugins("zh-CN", "mine.empty.action")).toBe("新增插件");
    expect(translatePlugins("zh-CN", "mine.create")).toBe("新增插件");
    expect(translatePlugins("zh-CN", "create.title")).toBe("新增插件");
    expect(translatePlugins("zh-CN", "create.submit")).toBe("新增插件");
    expect(translatePlugins("en-US", "create.source.git")).toBe("Git repository");
    expect(translatePlugins("zh-CN", "create.error.duplicateKey", { key: "plugin.a.b" })).toContain("plugin.a.b");
  });

  it("exposes the wizard and category copy the create flow depends on", () => {
    expect(translatePlugins("zh-CN", "create.wizard.step1.title")).toBe("选择 Agent 工具");
    expect(translatePlugins("zh-CN", "create.wizard.step2.title")).toBe("来源与详情");
    expect(translatePlugins("zh-CN", "create.wizard.next")).toBe("下一步");
    expect(translatePlugins("zh-CN", "create.wizard.back")).toBe("上一步");
    expect(translatePlugins("zh-CN", "create.field.category")).toBe("分类");
    expect(translatePlugins("zh-CN", "create.error.categoryRequired")).toBe("请为该插件选择分类。");
    expect(translatePlugins("en-US", "create.field.category")).toBe("Category");
    expect(translatePlugins("en-US", "categories.admin.title")).toBe("Plugin categories");
    // The category picker's empty state must tell the user who unblocks them.
    expect(translatePlugins("zh-CN", "create.category.empty.title")).toBe("暂无可用分类");
    expect(translatePlugins("zh-CN", "create.wizard.selectedTools", { count: 3 })).toContain("3");
  });

  it("keeps the plugin locale catalogs symmetric", () => {
    const keys = Object.keys(PLUGINS_MESSAGE_KEYS);
    expect(keys.length).toBeGreaterThan(0);
    for (const key of keys) {
      const zh = translatePlugins("zh-CN", key as never);
      const en = translatePlugins("en-US", key as never);
      // A missing zh key falls back to the raw key string, which is the tell.
      expect(zh).not.toBe(key);
      expect(en).not.toBe(key);
    }
  });
});
