# sdkwork-webserver-mp-applications

The applications capability of the SDKWork Web Server WeChat mini program.

It renders the `deploy_app` list on a mobile viewport against the deployments
app API — the same authority the PC console mounts through
`@sdkwork/deployments-pc-console-publishing`. That package is React-PC only, so
this package composes the same generated client instead of re-using that
component tree.

Boundaries:

- The generated deployments app client arrives **by prop**, narrowed to the
  `ApplicationsListReader` port this screen actually uses. Capability code never
  calls `createClient`, never imports a generated transport module, and never
  touches `wx.*` (`APP_SDK_INTEGRATION_SPEC.md` §2,
  `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §6/§8).
- Copy lives in `src/i18n/<locale>/webserver/applications/list.ts` and is
  resolved through a `resolveMessage` callback the runtime supplies, so the
  package carries no locale strategy of its own.
- Lists request one page at a time and page through the SDK's `pageInfo`
  (`PAGINATION_SPEC.md`); the package never downloads a full set to slice it.
- `state/` registers its slice with the core's sensitive-state clearing
  registry, so a logout or account switch drops tenant-scoped rows
  (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §7).
- `pages/applicationListPageModel.ts` owns every state transition. The native
  `src/pages/applications/index.js` projection is a thin adapter that forwards
  `setData` into it, which is what makes the behaviour testable off-device.
