#!/usr/bin/env node
// Runs the ignored Pebble ACME lifecycle test with the local CA binaries.
//
// The test spawns `pebble` and `pebble-challtestsrv` itself; it is `#[ignore]`d
// only because those binaries are not part of the repository toolchain. This
// runner fails with an actionable message when they are absent instead of the
// silent skip a plain `cargo test` produces.
//
// Usage:
//   pnpm test:acme:pebble
//   PEBBLE=/opt/pebble/pebble CHALLTESTSRV=/opt/pebble/pebble-challtestsrv \
//     pnpm test:acme:pebble

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";

const binaries = [
  { env: "PEBBLE", name: "pebble" },
  { env: "CHALLTESTSRV", name: "pebble-challtestsrv" },
];

const resolveBinary = ({ env, name }) => process.env[env] ?? name;

const isAvailable = (binary) => {
  if (binary.includes("/") || binary.includes("\\")) {
    return existsSync(binary);
  }
  const locator = process.platform === "win32" ? "where" : "which";
  const probe = spawnSync(locator, [binary], { stdio: "ignore" });
  return probe.status === 0;
};

const missing = binaries.filter((entry) => !isAvailable(resolveBinary(entry)));
if (missing.length > 0) {
  console.error(
    `[acme-pebble] missing ${missing
      .map((entry) => `${entry.name} (${entry.env})`)
      .join(", ")}; download from https://github.com/letsencrypt/pebble/releases ` +
      "and set PEBBLE/CHALLTESTSRV or put both on PATH",
  );
  process.exit(69);
}

const result = spawnSync(
  "cargo",
  [
    "test",
    "-p",
    "sdkwork-webserver-acme-service",
    "--test",
    "pebble_lifecycle",
    "--",
    "--ignored",
    "--nocapture",
  ],
  { stdio: "inherit" },
);

process.exit(result.status ?? 1);
