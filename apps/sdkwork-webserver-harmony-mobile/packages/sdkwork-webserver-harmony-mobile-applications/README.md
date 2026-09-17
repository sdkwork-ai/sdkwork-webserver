# SDKWork Web Server HarmonyOS Mobile Applications

The tenant application catalog capability: route contribution, generated-SDK
record projection, pagination state machine, ArkUI page, and locale fragments.

Layer role: `frontend-feature`. The generated app SDK port arrives **injected**
from root bootstrap; this package never constructs a transport, never reads a
runtime environment value, and never touches a HarmonyOS system API.

The screen is the same one the PC console and the mini program root render:
`app.webserver.applications.list`, gated on `deploy.apps.read`, reading the
`deploy_app` list page by page.
