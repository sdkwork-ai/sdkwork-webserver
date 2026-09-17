# Component Deployment

This application surface shares the enclosing application deployment unit.
Deployment profiles are owned by `../../../etc/sdkwork.deployment.config.json`; runtime process topology is owned by `../../../specs/topology.spec.json`.
Surface-local runtime templates live in `../config/mini-program/` and are materialized into the mini program runtime bundle by `../scripts/build-runtime.mjs`.
Surface-local build and test commands stay in this application root.
