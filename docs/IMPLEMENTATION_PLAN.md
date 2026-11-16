# Axion implementation plan

This checklist is an acceptance ledger, not a roadmap graphic. An item is checked only after the implementation has been exercised by the stated validation. Test output and environmental limitations are recorded in `IMPLEMENTATION_LOG.md`.

## Release gates

- RocksDB is the default and production storage backend. In-memory storage may exist only as an explicit test/development option.
- Criterion is the Rust microbenchmark harness.
- Official ANN-Benchmarks integration is required for comparative recall-versus-QPS evaluation on standardized datasets.
- Kubernetes is a release gate. Manifests must cover Deployments, Services, ConfigMaps, Secret references, persistent volumes, resource limits, health probes, and a validated kind workflow including persistence after pod replacement.
- No measured performance claim is published without a reproducible result artifact.

## Phase checklist

- [x] **0 — Repository audit and architecture plan**
  - [x] Inspect the repository, remote, host tools, and Docker daemon.
  - [x] Record toolchain gaps without hiding them.
  - [x] Define validation-driven milestones.
- [x] **1 — Mathematical core**
  - [x] Validated dense vectors, cosine, Euclidean, dot product, normalization.
  - [x] Deterministic exact top-k with edge-case and property tests.
- [x] **2 — HNSW**
  - [x] From-first-principles multilayer graph, insertion, search, deletion, rebuild.
  - [x] Deterministic recall tests against exact search.
- [x] **3 — Durable collection engine**
  - [x] RocksDB default, versioned keys, atomic writes, restart recovery.
  - [x] CRUD, batch upserts, filtering, pagination, index reconstruction.
- [x] **4 — REST and gRPC**
  - [x] Axum API, structured errors, auth, limits, CORS, OpenAPI, graceful shutdown.
  - [x] Equivalent collection/vector/search gRPC operations.
- [x] **5 — Python SDK**
  - [x] Typed sync/async clients, timeouts, retry policy, packaging, tests, example.
- [x] **6 — Container workflow**
  - [x] Multi-stage images and `docker compose up --build`.
  - [x] Health, real API integration flow, restart persistence, clean shutdown.
- [x] **7 — Web product**
  - [x] Responsive landing page and real management console.
  - [x] Collections, vector explorer, search playground, performance, health.
  - [x] Vitest, Playwright against the real backend, accessibility and visual QA.
- [x] **8 — Observability and hardening**
  - [x] Prometheus metrics, structured logs, request IDs, OpenTelemetry hooks.
  - [x] Threat model, dependency audit, concurrency and recovery validation.
- [x] **9 — Benchmarks**
  - [x] Criterion exact/HNSW/index microbenchmarks.
  - [x] Official ANN-Benchmarks adapter and standardized dataset workflow.
  - [x] Recall/QPS results compared with established algorithms on the same host.
- [x] **10 — Kubernetes**
  - [x] Production-oriented manifests and documented configuration.
  - [x] Build/load/deploy to kind, exercise APIs, replace pod, verify persisted data.
- [x] **11 — Distributed search**
  - [x] Deterministic sharding, fan-out, merge, health, partial failure behavior.
  - [x] Multi-container integration tests and explicit consistency limitations.
- [x] **12 — Documentation and release**
  - [x] Architecture SVG/Mermaid, guides, ADRs, API and storage documentation.
  - [x] Real screenshots, polished README, CI, final clean-clone validation.
  - [x] All checks pass and commits are pushed without rewriting history.

## Validation commands

The canonical release commands will be maintained as targets in the root `Makefile`:

```text
make check              # formatting, lint, unit and integration tests
make compose-test       # image build, API flow, and restart persistence
make distributed-test   # two-shard partition, degradation, and recovery
make ann-benchmarks     # official framework comparative run
make kind-test          # local cluster deployment and persistence validation
make e2e                # real-backend browser acceptance tests
```

