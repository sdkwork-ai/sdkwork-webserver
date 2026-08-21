import { webserverWorkspaceEnUs } from "./en-US/webserver/workspace/workspace.ts";
import { webserverWorkspaceZhCn } from "./zh-CN/webserver/workspace/workspace.ts";

export type WebserverLocale = "en-US" | "zh-CN";
export type WebserverMessageKey = keyof typeof webserverWorkspaceEnUs;

const catalogs: Record<WebserverLocale, Record<WebserverMessageKey, string>> = {
  "en-US": webserverWorkspaceEnUs,
  "zh-CN": webserverWorkspaceZhCn,
};

export function translateWebserver(locale: WebserverLocale, key: WebserverMessageKey, values: Record<string, string | number> = {}): string {
  const template = catalogs[locale][key] ?? catalogs["en-US"][key] ?? String(key);
  return Object.entries(values).reduce((message, [name, value]) => message.replaceAll(`{${name}}`, String(value)), template);
}
