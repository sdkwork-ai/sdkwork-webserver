# SDKWork Webserver PC Contract

The application is a browser composition root with two isolated surfaces:

- `app-console`: standalone tenant operations through `@sdkwork/webserver-app-sdk`.
- `backend-admin`: internal Web Server operations through `@sdkwork/webserver-backend-sdk`.

The root owns runtime configuration, IAM bootstrap, the shared TokenManager, route composition, and lazy loading. Capability packages own navigation metadata; surface core packages own generated SDK adaptation. Drive uploads and cloud publishing do not belong to this application.

Standalone browser profiles use `browserOriginMode = same-origin`. Development keeps the Vite renderer and Rust ingress as separate internal processes, but Vite proxies canonical API paths so the browser sees only the renderer origin. Standalone production assets and APIs are delivered by the application ingress, so the same public runtime contract resolves SDK clients from the page origin without exposing an internal listener URL.
