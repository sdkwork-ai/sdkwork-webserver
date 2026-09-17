# SDKWork Webserver Mini Program Host

WeChat platform adapter for the SDKWork Web Server mini program.

`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §8: capability packages must not call
`wx.*` (or any equivalent platform global). This package is the only place a
platform API is bound, and it binds it behind a typed contract:

- `contracts/hostAdapter.ts` — the adapter contract and the stable host error
  vocabulary every implementation normalizes into.
- `weixin/` — the real WeChat implementation (`globalThis.wx`, guarded).
- `testing/` — an in-memory adapter so the Node-side contract tests exercise the
  same contract the device runs, without a device.

Only the categories the current console needs are implemented; the remaining
standard categories (`qrScanner`, `mediaPicker`, `share`, …) are added when a
capability actually consumes them, not speculatively.
