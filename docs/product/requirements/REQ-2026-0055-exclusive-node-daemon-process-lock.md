# REQ-2026-0055 Exclusive Web Node Daemon Process Lock

```yaml
id: REQ-2026-0055
title: Prevent concurrent Node Daemons from mutating one node state directory
owner: sdkwork-webserver
status: accepted
source: node-activation-single-writer-safety
problem: Multiple sdkwork-web-agent compatibility processes can start with the same durable state directory and concurrently update desired/observed generations, certificate bundles, Nginx site files, and reload operations. Atomic file replacement protects one write but does not establish one process as the node activation writer.
goals:
  - Acquire one cross-platform exclusive operating-system file lock before reading durable state or contacting the control plane.
  - Fail startup immediately when another Node Daemon owns the same state directory.
  - Release ownership automatically when the process exits or crashes.
  - Keep the lock path confined to the validated durable state directory and reject symlinks/non-files.
non_goals:
  - Distributed leader election, Kubernetes lease ownership, cluster quorum, or cross-node coordination.
  - Coordinating unrelated Nginx/package-manager/operator processes that edit the same filesystem outside the Node Daemon contract.
  - Claiming advisory locks on network/distributed filesystems provide the same semantics as a local host filesystem.
acceptance_criteria:
  - The lock file is the fixed sdkwork-web-node-daemon.lock sibling in the state directory and contains no token, path, PID, manifest, or other retained payload.
  - State paths and ancestors remain absolute and symlink-rejected; the lock target must be a regular non-symlink file before and after open.
  - Unix lock-file permissions are 0600 and the containing directory is synchronized after acquisition.
  - Standard-library exclusive try_lock is acquired before AgentLocalState::load; a contending process fails without waiting or entering the sync loop.
  - Dropping or crashing the owner releases the operating-system lock without stale-PID parsing or lock-file deletion.
  - Tests prove exclusion, empty lock content, Unix permissions, symlink rejection, release/reacquire, and startup ordering.
non_functional_requirements:
  concurrency: One state directory has exactly one Node Daemon writer and no waiter queue.
  security: Lock acquisition does not follow an observed symlink and stores no sensitive data.
  reliability: A retained empty lock file is not treated as ownership; only the live operating-system lock is authoritative.
trace:
  specs:
    - RUST_CODE_SPEC.md
    - RUNTIME_DIRECTORY_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-web-agent
verification:
  - cargo test -p sdkwork-web-agent
  - node --test tests/contract/agent-sync-state.contract.test.mjs
  - pnpm verify
```

## Runtime Boundary

The Node Daemon creates or opens `sdkwork-web-node-daemon.lock` in the parent directory of the
resolved state file, secures the file, and requests a non-blocking exclusive lock. The process keeps
the file handle alive for the complete async main loop. A second daemon configured with another
state filename but the same state directory still contends on the same fixed lock.

The empty file may remain after normal or abnormal termination. This is intentional: deleting lock
files creates inode replacement races. Ownership is represented only by the live kernel lock, so a
new daemon can acquire the retained file immediately after the old handle is closed by process exit.

Production deployment must place the state directory on a node-local filesystem with permissions
that prevent untrusted replacement and must schedule one Node Daemon replica per host. Shared NFS,
SMB, object mounts, or distributed filesystems require separately proven lock semantics and are not
approved by this requirement.

## Remaining Coordination Gate

This requirement closes same-directory Node Daemon competition only. Versioned certificate/current
references, stale inventory reconciliation, served-state probes, control-plane acknowledgement,
Kubernetes lease/fencing, and multi-node rollout remain separate commercial convergence gates.
