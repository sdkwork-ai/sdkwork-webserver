# ADR-20260731 Web And Deploy Domain-Certificate Authority Boundary

Status: proposed
Requirement: REQ-2026-0067, REQ-2026-0068
Owner: sdkwork-webserver, sdkwork-deployments, sdkwork-iam
Date: 2026-07-31
Specs: ARCHITECTURE_DECISION_SPEC.md, DATABASE_SPEC.md, SUBJECT_ID_SPEC.md, API_SPEC.md, SDK_SPEC.md, EVENT_SPEC.md, DEPLOYMENT_SPEC.md, SECURITY_SPEC.md

## Context

Web and Deploy currently contain overlapping mutable models for sites, Zones, hostnames,
certificates, listener bindings, and deployments. IAM additionally stores primary-domain and
domain-configuration fields. Independent writes create split-brain route and TLS state, while
cross-database foreign keys would couple deployment availability to the Web control-plane store.

## Decision

- `sdkwork-web` is the sole mutable authority for root domains, hostnames, ownership verification,
  sites, site bindings, certificate aggregates and versions, listener certificate bindings, and
  Web application deployment intent.
- `sdkwork-deploy` owns immutable rollout plans, target assignments, certificate distribution,
  TLS snapshots, and target observations. It stores the immutable Web public ids, versions, and
  hashes used by each rollout.
- `sdkwork-iam` owns application registration and authorization only. It does not own primary
  domains or domain configuration.
- Integration uses generated internal SDKs and versioned events. Cross-service identifiers are
  public UUIDs or explicit operation ids; database-local snowflake ids never become cross-database
  foreign keys.
- A Deploy snapshot is evidence of what was rolled out. It is immutable and cannot be edited as an
  alternate domain or certificate authority.
- Rollout success requires exact target observation quorum. Desired Web state alone is not serving
  evidence.

## Alternatives

- Keep duplicate mutable tables and dual write. Rejected because partial failure cannot preserve a
  single serializable truth across independent stores.
- Move all Web configuration into Deploy. Rejected because domain verification, application
  routing, Backend API/SDK, and Web product ownership already belong to Web.
- Share one database schema with cross-module foreign keys. Rejected because service lifecycle,
  failure isolation, release sequencing, and ownership become inseparable.

## Consequences

Web product behavior has one authority. Deploy can scale rollout and retain audit-grade evidence
without accepting configuration mutations. IAM remains focused on identity and authorization.
The migration is cross-repository and requires reconciliation, compatibility reads, session/token
repair, staged removal, backup evidence, and human approval.

## Verification

- Cross-repository contract tests prove Deploy consumes generated Web internal contracts.
- Static checks reject new mutable domain/certificate operations outside Web.
- Reconciliation tests compare normalized hostname, fingerprint, version, and rollout hashes.
- Rollout tests prove assignment and observation quorum without cross-database foreign keys.
- IAM migration tests prove positive numeric subjects and removal of domain configuration.

## Supersedes / Superseded By

Proposed for human review. On acceptance, it supersedes any local documentation that treats
Deploy or IAM as an independent mutable domain/certificate authority.
