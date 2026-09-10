#!/usr/bin/env node
/**
 * Edge exposure governance check (PRD-FR-021).
 *
 * The standalone gateway serves the application public ingress and the
 * unauthenticated health/diagnostics surfaces on one router. The public nginx
 * edge must therefore explicitly deny /metrics and /livez in every
 * gateway-locations snippet, keeping those surfaces loopback-governed
 * (scraped via SDKWORK_WEBSERVER_DATA_PLANE_OPERATIONS_BIND).
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const root = process.argv.includes("--root")
  ? process.argv[process.argv.indexOf("--root") + 1]
  : process.cwd();
const snippetDir = join(root, "deployments", "webserver", "snippets");

const failures = [];
const files = readdirSync(snippetDir).filter((name) =>
  name.startsWith("gateway-locations.") && name.endsWith(".conf"),
);
if (files.length === 0) {
  failures.push(`no gateway-locations.*.conf snippets found under ${snippetDir}`);
}
for (const name of files) {
  const body = readFileSync(join(snippetDir, name), "utf8");
  for (const path of ["/metrics", "/livez"]) {
    const pattern = `location = ${path} {`;
    if (!body.includes(pattern)) {
      failures.push(`${name}: missing exact-match deny block \`${pattern}\``);
      continue;
    }
    const blockStart = body.indexOf(pattern);
    const blockEnd = body.indexOf("}", blockStart);
    const block = body.slice(blockStart, blockEnd);
    if (!block.includes("return 404")) {
      failures.push(`${name}: \`${pattern}\` must \`return 404\`, got: ${block.trim()}`);
    }
  }
}

if (failures.length > 0) {
  console.error("edge exposure governance check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`edge exposure governance check passed (${files.length} snippet(s))`);
