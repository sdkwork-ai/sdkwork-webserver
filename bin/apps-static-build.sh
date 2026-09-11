#!/usr/bin/env bash
# apps-static-build.sh — build the Adaptive Web static dist (PC / H5) of any
# independent sibling module in the sdkwork workspace via the canonical browser
# runner (MODULE_BIN_SPEC.md §2.2: apps-<object>-<action>, object "static").
# Implementation: bin/lib/apps-static.sh.
SDKWORK_ENTRY="apps-static-build"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/apps-static.sh"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/bootstrap.sh"
