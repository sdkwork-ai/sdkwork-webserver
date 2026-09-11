#!/usr/bin/env bash
# apps-static-deploy.sh — build, package and publish the Adaptive Web static
# dist (PC / H5) of any independent sibling module to a target host (local WSL
# or remote ssh), with extraction, verification and quick rollback
# (MODULE_BIN_SPEC.md §2.2: apps-<object>-<action>, object "static").
# Implementation: bin/lib/apps-static.sh (build) + bin/lib/apps-static-deploy.sh.
SDKWORK_ENTRY="apps-static-deploy"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/apps-static.sh"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/apps-static-deploy.sh"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/bootstrap.sh"
