# SDKWork Webserver H5

Mobile browser Adaptive Web surface for SDKWork Web Server.

Authority: `APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §2.1,
`SDKWORK_DEPLOY_SPEC.md` §8 / §8.1.

Desktop browsers receive `apps/sdkwork-webserver-pc`; mobile browsers receive
this H5 root. Plan folding collapses when one surface is missing; neither uses
`deployments/webserver/static` (`static-fallback`).

```bash
pnpm --dir apps/sdkwork-webserver-h5 install
pnpm --dir apps/sdkwork-webserver-h5 run build:standalone
pnpm --dir apps/sdkwork-webserver-h5 check
```
