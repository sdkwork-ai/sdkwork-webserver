# SDKWork Webserver H5 Shell

Navigation shell of the SDKWork Web Server mobile console.

- `navigation/routeRegistry.ts` — the ordered navigation model feature packages
  contribute to; the root application renders it and owns no route knowledge of
  its own.
- `components/AppShell.tsx` — the chrome (header + bottom tab bar) every screen
  renders inside.

The shell holds no SDK access and no feature state.
