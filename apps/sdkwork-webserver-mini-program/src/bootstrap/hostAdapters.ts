/**
 * Host adapter selection for the mini program root.
 *
 * `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §8 forbids capability packages from
 * touching platform globals; the root picks the adapter once and injects it. The
 * WeChat adapter is always constructed — it degrades to `platform: "unknown"`
 * when no bridge is present, which is what lets the Node-side contract tests
 * bootstrap the exact same composition the device runs.
 */
import { createWeixinHostAdapter, type WebserverMpHostAdapter } from "@sdkwork/webserver-mp-host";

export function createWebserverMiniProgramHostAdapter(
  adapter?: WebserverMpHostAdapter,
): WebserverMpHostAdapter {
  return adapter ?? createWeixinHostAdapter();
}
