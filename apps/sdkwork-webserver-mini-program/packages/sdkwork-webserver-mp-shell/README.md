# SDKWork Webserver Mini Program Shell

Navigation and auth-gate shell of the SDKWork Web Server WeChat mini program.

- `navigation/routePlacement.ts` — route contributions and their mini program
  placement metadata, plus the projection the build uses to assemble `app.json`
  pages and `subPackages` (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §5).
- `navigation/navigationModel.ts` — the ordered navigation model capability
  packages contribute to; the root page renders it and owns no route knowledge
  of its own.
- `auth/authGate.ts` — the single place a route decides whether the current
  session may render it, driven by an injected permission predicate.

The shell holds no SDK access and no feature state.
