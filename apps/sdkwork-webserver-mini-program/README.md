# SDKWork Webserver Mini Program

WeChat mini program console for SDKWork Web Server.

Authority: `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md`,
`APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §2,
`SDKWORK_WEBSERVER_SPEC.md` §17.4 (standalone-only edge).

```bash
pnpm --dir apps/sdkwork-webserver-mini-program install
pnpm --dir apps/sdkwork-webserver-mini-program run build:mini-program   # standalone.development
pnpm --dir apps/sdkwork-webserver-mini-program run build:mini-program:prod
pnpm --dir apps/sdkwork-webserver-mini-program check
```

`build:mini-program` selects one runtime profile from `config/mini-program/`,
projects the route contributions onto `src/app.json`, and emits the runtime bundle
to `src/runtime/`. Open this directory in WeChat devtools afterwards
(`miniprogramRoot = src/`); `pnpm dev` keeps the bundle rebuilt on change.

`src/runtime/` is build output and is not committed — run a build before opening
the project, or devtools will report the missing bundle.

`sdkwork-webserver` is standalone-only: every runtime profile declares an
absolute application origin, and the SDK appends `/app/v3/api` itself.
