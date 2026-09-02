import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
const root = resolve(import.meta.dirname, "..");
function files(directory: string): string[] { return readdirSync(directory).filter((name) => name !== "node_modules").flatMap((name) => { const path = resolve(directory, name); return statSync(path).isDirectory() ? files(path) : path.endsWith(".ts") || path.endsWith(".tsx") ? [path] : []; }); }
describe("surface SDK boundaries", () => {
  it("keeps backend SDK imports out of console packages", () => { const offenders = files(resolve(root, "packages")).filter((path) => path.includes("-console-") && readFileSync(path, "utf8").includes("@sdkwork/webserver-backend-sdk")); expect(offenders).toEqual([]); });
  it("keeps app SDK imports out of admin packages", () => { const offenders = files(resolve(root, "packages")).filter((path) => path.includes("-admin-") && readFileSync(path, "utf8").includes("@sdkwork/webserver-app-sdk")); expect(offenders).toEqual([]); });
  it("does not use raw HTTP in authored UI packages", () => { const offenders = files(resolve(root, "packages")).filter((path) => /\bfetch\s*\(/.test(readFileSync(path, "utf8"))); expect(offenders).toEqual([]); });
});
