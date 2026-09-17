# SDKWork Web Server HarmonyOS Mobile Shell

Page stack, ordered navigation model, and the auth gate for the HarmonyOS root.

Layer role: `frontend-shell`. Capability packages declare auth mode and
permission hints; the shell decides whether a route may render, so no screen
re-implements the guard and no screen reimplements tab-bar ordering.
