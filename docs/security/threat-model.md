# Security and threat model

## Scope and trust boundaries

Axion stores caller-supplied vectors, record IDs, collection names, and JSON
metadata. REST and gRPC listeners, the web proxy, RocksDB directory, container
runtime, Kubernetes Secret provider, and telemetry exporter are separate trust
boundaries. The operator is trusted to control deployment configuration, the
host or cluster, filesystem permissions, TLS termination, backups, and API-key
distribution.

## Controls implemented

- The server binds to loopback by default; container and Kubernetes manifests
  opt into `0.0.0.0` only inside their network boundary.
- Optional bearer authentication uses constant-time comparison. Protected
  collection/vector/search routes share the same enforcement middleware.
- Collection names, record IDs, dimensions, top-k, list limits, batch sizes,
  request-body bytes, and finite floating-point values are validated.
- Internal storage errors are logged server-side but returned as a generic
  response, limiting implementation detail leakage.
- RocksDB is the only production backend. Synced WAL writes and atomic batches
  define acknowledgment semantics; unknown schema versions fail closed.
- Rust workspace policy forbids unsafe code. Containers run as UID/GID 10001
  with no privilege escalation and a read-only root filesystem. Kubernetes
  enforces the Restricted Pod Security Standard and disables service-account
  token mounting.
- Metrics use bounded labels; record IDs, collection names, API keys, vectors,
  and metadata never become Prometheus labels.
- API keys are referenced through environment variables or Kubernetes Secrets
  and are never checked into the repository.

## Known limitations and operator responsibilities

- Axion does not terminate TLS. Use an authenticated TLS ingress, service mesh,
  or private network; never expose a plaintext unauthenticated listener to the
  public internet.
- The built-in API key is one shared secret. It has no users, roles, rotation
  protocol, or per-collection authorization. Use an external identity-aware
  proxy when multi-tenant isolation is required.
- Health, metrics, OpenAPI, and documentation endpoints are intentionally
  unprotected for orchestration and scraping. Restrict them with network policy
  when their metadata is sensitive.
- Stored vectors and metadata are not encrypted by Axion. Use encrypted disks,
  encrypted backups, and a storage class with appropriate access controls.
- RocksDB supports one writer for a data directory. Do not increase the
  single-node Deployment replica count; distributed shards own separate data.
- Resource limits reduce blast radius but do not provide per-client quotas or
  denial-of-service protection. Put rate and connection limits at the edge.
- OpenTelemetry can disclose operation timing and request paths to its backend.
  Treat the collector as trusted and keep authentication material out of span
  fields.
- No claim is made for consensus, replication, automatic failover, or Byzantine
  fault tolerance.

## Security reporting

Follow `SECURITY.md` for private vulnerability reporting. Dependency audits run
in CI, but a clean audit is not a substitute for patch management, secret
rotation, backup restore exercises, or deployment-specific review.
