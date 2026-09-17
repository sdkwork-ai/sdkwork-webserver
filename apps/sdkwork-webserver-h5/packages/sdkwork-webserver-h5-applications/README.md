# sdkwork-webserver-h5-applications

The applications feature face of the Web Server H5 console.

It renders the `deploy_app` list on a mobile viewport against the deployments
app API — the same authority the PC console mounts through
`@sdkwork/deployments-pc-console-publishing`. The deployments console package is
React-PC only, so this package composes the same generated client instead of
re-using that component tree.

Boundaries:

- The generated deployments app client arrives **by prop**, narrowed to the
  `ApplicationsListReader` port this screen actually uses. Feature code never
  calls `createClient` and never imports a generated transport module
  (`APP_SDK_INTEGRATION_SPEC.md` §2).
- Copy lives in `src/i18n/<locale>/webserver/applications/list.ts` and is
  resolved through a `resolveMessage` callback the application root supplies, so
  the package carries no locale strategy of its own.
- Lists request one page at a time and page through the SDK's `pageInfo`
  (`PAGINATION_SPEC.md`); the package never downloads a full set to slice it.
