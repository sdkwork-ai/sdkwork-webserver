# ADR-20260730 Drive-Backed Application Source Versions

## Status

Accepted.

## Context

Application creation currently combines source intake and release creation. ZIP and directory sources are uploaded to Drive, while Git repository URLs bypass Drive and are stored directly on deployment rows. This prevents a consistent version catalog, bounded retention, configuration discovery, and reliable release provenance.

## Decision

An application owns two distinct resources:

1. A source version is an immutable Drive-backed artifact with a business version tag, source type, original source reference, optional commit hash, digest, size, configuration discovery snapshot, lifecycle status, and retention state.
2. A release record references one ready source version and records the environment, release version, execution status, timestamps, and immutable artifact snapshot.

Browser-selected ZIP and directory content continues to use the generated Drive App SDK. Public HTTPS Git repositories are imported server-side using a bounded non-interactive shallow clone, deterministic packaging, and `DriveUploaderService`. Repository URLs with credentials, query strings, fragments, or non-HTTPS schemes are rejected. Private, loopback, link-local, and otherwise disallowed targets fail closed.

The per-application deployment configuration is stored in `webserver_site.runtime_config` and exposed through typed API fields. The default ready-source retention window is five, with a valid range of 1 through 50. Versions outside that window are marked pruned and cannot be published or used for rollback, while release records and immutable artifact snapshots remain auditable.

This release-retention transition does not synchronously delete Drive objects. Physical reclamation remains owned by Drive lifecycle processing and requires an explicit cross-owner cleanup contract; the Web Server repository must not bypass that boundary by mutating Drive tables directly.

`webserver_deployment.source_version_id` is nullable for existing records. New release flows provide `sourceVersionId`; the service resolves that source version in the same tenant and application, requires it to be ready and retained, then copies its stable artifact facts into the deployment record.

## Consequences

- ZIP, directory, and Git sources have one storage and permission model.
- One source version can be released more than once without re-uploading bytes.
- At most five source versions are ready and releasable by default.
- Release history remains auditable after a source version leaves the retained release window.
- Rollback is unavailable when the referenced source version is no longer retained.
- Drive object reclamation is intentionally separate from the synchronous source-version transaction.
- Existing artifact-based deployment callers remain compatible during migration, but new SDK consumers use `sourceVersionId`.

## Rejected Alternatives

- Keeping Git URLs only on deployment rows was rejected because it is mutable and bypasses Drive lifecycle and access controls.
- Treating deployment records as source versions was rejected because source storage and release execution have different lifecycles.
- Deleting old deployment rows to enforce retention was rejected because it destroys release history.
