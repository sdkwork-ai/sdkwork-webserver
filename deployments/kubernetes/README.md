# SDKWork Web Server Kubernetes Deployment

The authored manifests are production templates. They deliberately contain
`__SDKWORK_IMAGE_DIGEST__`; apply only rendered output bound to an attested registry digest. The
application manifest currently defers release builds, so these templates are not an assertion that
a registry image or production environment exists. Rendering also produces an immutable,
Node-specific `config-map.yaml` containing the compiled website listener policy; it contains no
credentials or certificate material. Its immutable name includes the rendered config SHA-256
prefix so a policy change creates a new object instead of mutating an active revision.

## Required Secrets

Create `sdkwork-webserver-runtime` through the approved secret manager integration with these keys:

| Key | Purpose |
| --- | --- |
| `database-url` | PostgreSQL authority for Web Server data |
| `iam-database-url` | IAM request-context database authority |
| `secret-encryption-key` | Production environment-value encryption root |
| `certificate-encryption-key` | Certificate private-key encryption root |
| `acme-contact-email` | ACME account contact |

This Secret is consumed by the migration Job. Cloud management routes are hosted through the
platform cloud gateway and the application standalone gateway is not deployed by this workload.

Create one distinct Node Secret for every rendered website data-plane StatefulSet. Every Node in a
tenant fleet must carry the same authorized tenant scope but a distinct Node UUID, Node token, and
provider credentials. The Secret name is passed through `--website-node-secret-name` and must
contain:

| Key | Purpose |
| --- | --- |
| `node-uuid` | Stable UUID registered by Deployments for this Web Node |
| `node-token` | Web Internal API ingress token bound to this Node |
| `tenant-scope-hash` | Exact lowercase SHA-256 tenant scope assigned to this process |
| `drive-ingress-token` | Drive Internal SDK service credential for the same tenant scope |
| `knowledgebase-ingress-token` | Knowledgebase Internal SDK service credential for the same tenant scope |
| `website-provider-events.json` | Loopback provider-event subscription config for this tenant scope |
| referenced signing-secret keys | The Node's Drive derivation secret and Knowledgebase outbox secrets referenced by the event config |

The event config uses `/run/secrets/sdkwork-webserver-node/<key>` paths for its signing secrets and
`/var/lib/sdkwork/webserver/website-provider-events` for checkpoints. Do not reuse a Node Secret across
StatefulSets. Do not commit a Secret manifest or plaintext values. Encryption roots and Node/provider
credentials must be independent, randomly generated, least-privilege, rotation-governed values.
The Drive entry must use subscription ID `drive-website-events`; its derivation secret must contain
the same bytes exposed to the Deploy runtime-assignment worker under the hashed filename contract.
Per-WebsiteRoot verification tokens are derived in memory and must not be stored in this Secret.

## Release And Apply

1. Build and validate the cloud release archive and container image.
2. Verify the image checksum, signature, provenance/attestation, and SBOM.
3. Allocate a stable opaque tenant fleet name matching `^tf-[a-z2-7]{15}$` from a cryptographically
   secure random source. The 15 Base32 symbols provide approximately 75 bits of non-secret,
   non-identifying orchestration entropy. Do not derive it from or set it to a tenant ID, tenant
   scope hash, customer name, email address, or domain. Render one Node workload with the verified
   registry digest and non-secret deployment names:

   ```powershell
   node scripts/render-kubernetes-manifests.mjs `
     --image-digest <64-hex-sha256> `
     --website-tenant-fleet-name <tf-plus-15-lowercase-base32-symbols> `
     --website-node-name <unique-dns-label> `
     --website-node-secret-name <unique-secret-dns-label> `
     --website-trusted-proxy-cidr <direct-ingress-cidr>
   ```

   Repeat `--website-trusted-proxy-cidr` for every reviewed network from which the Pod can directly
   receive public-ingress connections. The value is the observed TCP peer network after CNI or
   load-balancer source NAT, not a browser/client network. The renderer rejects missing, duplicate,
   universal, IPv4 broader than `/8`, IPv6 broader than `/16`, malformed, or compiler-invalid
   policies. It invokes the edge runtime's real config compiler before publishing any manifests.

4. Run one rendered `migration-job.yaml` once for the application environment and wait for
   successful completion; it is not a per-tenant migration. For each tenant fleet, apply
   `network-policy.yaml` from one of its renders. Apply every Node's `service.yaml`,
   `config-map.yaml`, and matching `deployment.yaml` into the fleet's dedicated namespace. The
   Website and headless Services are identical, idempotent fleet objects; the provider-event
   Service is Node-specific.
5. Render and deploy at least two different Node names and Node Secrets with the same tenant fleet
   name and tenant scope before claiming high availability for that tenant. The hard hostname
   spread constraint requires distinct Kubernetes worker nodes; the zone constraint distributes
   across labeled availability zones when capacity exists. Wait for every StatefulSet rollout and
   its `/healthz`, `/readyz`, and `/livez` exec probes before routing production traffic.
6. Label both the public ingress namespace and its ingress Pods with
   `sdkwork.com/network-role=public-ingress`. Label both the provider callback ingress namespace
   and its ingress Pods with `sdkwork.com/network-role=provider-event-ingress`, then apply
   `network-policy.yaml`.
7. Configure the reviewed internal HTTPS ingress or service mesh to send exact owner callback
   requests at `/nodes/{nodeUuid}/provider-events/drive-website-events` to
   `sdkwork-webserver-events-<tenant-fleet-name>-<node-name>:3811` for that exact Node. Preserve the path;
   the unqualified `/provider-events/{subscriptionId}` route is reserved for Knowledgebase. A fleet
   Service must never randomly distribute signed callbacks because Drive channels, signing
   secrets, and checkpoints are Node-bound. The authored relay sidecar preserves the TCP byte
   stream and forwards it to the loopback receiver, where HMAC, clock-window, tenant,
   organization, channel, and AsyncAPI checks remain authoritative. Every highly available Node
   needs its own owner subscription/callback. Production rollout is blocked until the deployment
   platform supplies TLS/mTLS identity, exact Node routing, and source-policy evidence for that
   internal ingress.
8. After every affected Node is Ready on the new config revision and rollback retention has
   expired, remove only unreferenced older ConfigMaps carrying that Node's
   `sdkwork.com/web-node` label. Never delete the ConfigMap referenced by the active StatefulSet.

Each StatefulSet has exactly one replica because the Node UUID, credentials, and recovery slots are
identity-bound. Its dedicated `ReadWriteOnce` PVC persists runtime-set A/B slots and provider-event
checkpoints across Pod replacement. The workload uses a read-only root filesystem, disabled
service-account token mounting, dropped Linux capabilities, RuntimeDefault seccomp, bounded
ephemeral-storage requests/limits, disabled Service-link environment injection, loopback-only
operations, exec probes, a bounded provider-event relay sidecar, ingress NetworkPolicy, rolling
updates, and a tenant-fleet-scoped PodDisruptionBudget.
The container also sets `SDKWORK_WEBSERVER_WEBSITE_PROVIDER_BUFFERED_CONTENT_BYTES=268435456`, a
non-queueing process-wide 256 MiB admission ceiling for complete Drive/Knowledgebase SDK content
buffers retained by active responses. The compiled route's full `maximumObjectBytes` ceiling is
reserved even for small or Range requests, and the reservation is released when the response
completes, fails, or is cancelled.
Keep this value within the runtime's 16 MiB..2 GiB validation range and below the Pod's memory limit
with headroom for connections, TLS, runtime descriptors, allocator overhead, and the generated SDK
copy itself. Raising the value requires measured capacity evidence; it does not enable streaming.
Hostname topology spread is a hard scheduling constraint for Nodes in the same tenant fleet;
availability-zone spread is preferred without making a single-zone cluster permanently
unschedulable.

Public website HTTP is available only through the per-tenant ClusterIP
`sdkwork-webserver-website-<tenant-fleet-name>`. The external load balancer/CDN owns public exposure and
must route every custom or platform domain to the Service for its assigned tenant fleet before the
runtime performs Host, Binding, device Variant, Mount, and resource selection. No shared Service
may select Pods from different tenant scopes. The `sdkwork.com/tenant-fleet` selector enforces this
within Kubernetes; the runtime-set tenant scope and Secret-bound provider clients enforce it again
inside the process. The data plane accepts
`X-Forwarded-Proto` only from a direct peer in the rendered trusted CIDRs, requires one exact
`http` or `https` value, rejects malformed trusted metadata, and never lets forwarding metadata
downgrade native TLS. Website HTTPS redirects, reverse-proxy forwarding headers, and access logs
consume that same resolved scheme. The runtime can consume validated native TLS assignments and
atomically hot-activate Rustls SNI contexts, but this template explicitly sets
`SDKWORK_WEBSERVER_TLS_RUNTIME_SOURCE=external`. It does not claim native custom-domain certificate
activation until Deploy publishes independent TLS assignments, an approved secret provider mounts
authorized versioned material, the listener and Service expose the reviewed TLS port, and
served-fingerprint/node-convergence probes are recorded.

This dedicated-tenant topology is the production-deployable baseline. A shared multi-tenant edge
fleet is not implemented by these manifests. It requires owner-defined tenant-aware runtime
assignment, a credential broker/resolver with hot rotation, per-tenant generated Drive and
Knowledgebase SDK client lifecycle, tenant-aware provider-event subscription authority, bounded
client/cache eviction, and tenant-qualified readiness, drift, usage, and rollout contracts. Do not
replace those missing contracts with token maps, tenant headers, raw HTTP, or direct cross-service
storage access.

## Scaling Contract

The website data plane is a fleet of **node-scoped StatefulSets**: every Node
owns its own identity (Node UUID, credential secret, provider-event callback
routing, and recovery volume), so a generic HorizontalPodAutoscaler must not
clone a Node identity. Scale horizontally by rendering and applying additional
Node manifests (`render-kubernetes-manifests.mjs --image-digest ... --website-node-name
<new-node> ...`) into the same tenant fleet, then rebalancing the fleet's
public ingress. Capacity-based HPA belongs to stateless components upstream of
the fleet (ingress, cache), not to the Node identity itself. Always keep at
least two Nodes per tenant fleet for high availability (step 5).

## Certificate Worker

`certificate-worker.yaml` deploys `sdkwork-webserver-certificate-worker` (exactly one
replica; the worker id and ACME account credentials are stable process identities). It consumes
`database-url` and `secret-encryption-key` from `sdkwork-webserver-runtime`, `acme-contact-email`
from the same Secret, and `node-uuid` from the Node Secret of the fleet it serves.

The `sdkwork-webserver-certificate-state` PVC carries three state roots:

| Root | Purpose |
| --- | --- |
| `acme-accounts/` | Encrypted ACME account credentials (reuse one CA account across restarts) |
| `acme-webroot/` | HTTP-01 challenge tokens written by the worker |
| `tls-materials/` | Versioned certificate material + `tls-runtime.json` snapshot for the self-hosted TLS runtime |

To activate self-hosted TLS hot reload and HTTP-01 challenge serving on a Node, mount the same PVC
into the website data-plane StatefulSet (e.g. `certificate-state` mounted at
`/var/lib/sdkwork/webserver`), set `SDKWORK_WEBSERVER_TLS_RUNTIME_SOURCE=file` with the matching
`SDKWORK_WEBSERVER_TLS_RUNTIME_SNAPSHOT_FILE`/`SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT`, and configure
`acmeHttp01.webroot` in the Node's `sdkwork.webserver.config.json` to point at the shared
`acme-webroot` directory. No nginx directive or external process is involved.
