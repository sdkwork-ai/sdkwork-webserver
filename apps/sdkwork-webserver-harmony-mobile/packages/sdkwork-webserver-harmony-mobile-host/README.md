# SDKWork Web Server HarmonyOS Mobile Host

Typed HarmonyOS platform adapters (secure storage, network status, app lifecycle,
device info) for the HarmonyOS root.

Layer role: `frontend-host`. This is the only package allowed to touch a
HarmonyOS system API. Capability packages depend on the core-declared interfaces,
so a platform branch never leaks into a screen.
