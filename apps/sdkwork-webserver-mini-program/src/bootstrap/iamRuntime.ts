/**
 * IAM runtime projection for the mini program root.
 *
 * The console authenticates through the SDKWork app-api dual-token session
 * (`@sdkwork/webserver-mp-core/session`); a native mini program has no
 * independent IAM runtime of its own. Platform login facts (a WeChat login code,
 * a phone-number grant) are inputs that must be exchanged through approved
 * app-api/appbase flows, never through a feature-local transport
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §7).
 *
 * This module exists so the root has one named place to wire that exchange once
 * the appbase login flow lands; until then it reports "not composed" explicitly
 * rather than pretending a session exists.
 */
export interface WebserverMiniProgramIamRuntime {
  readonly composed: false;
  readonly reason: "appbase-login-flow-not-composed";
}

export function describeWebserverMiniProgramIamRuntime(): WebserverMiniProgramIamRuntime {
  return { composed: false, reason: "appbase-login-flow-not-composed" };
}
