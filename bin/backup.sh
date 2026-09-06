#!/usr/bin/env bash
# backup.sh — backup, verify, and restore an environment (OPERATIONS_SPEC §5).
SDKWORK_ENTRY="backup"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/bootstrap.sh"
