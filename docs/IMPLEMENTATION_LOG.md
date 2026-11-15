# Implementation log

## 2026-11-15 — Phase 0 audit

### Repository state

- Cloned `https://github.com/jonxsong/Axion.git` into the requested workspace.
- The remote repository was empty; there was no code or history to preserve.
- Docker Desktop is running and the daemon reports 16 CPUs and approximately 23 GiB RAM.

### Available tools

- Rust/Cargo 1.100.0, Node.js 24.22.1, pnpm 11.25.0, Docker 29.9.3, Compose 5.7.0, and kubectl 1.36.1 are available.
- The system `python` is 3.11.0, below the Python SDK's declared 3.12 minimum. SDK validation will use an isolated Python 3.12 runtime or container.
- kind and minikube were not installed at audit time. The local Kubernetes workflow will install/use kind before validation.

### Decisions

- RocksDB is the sole default persistent backend. This is a release invariant.
- HNSW is implemented in Rust in this repository; no third-party ANN index is used by the engine.
- ANN-Benchmarks is the comparative benchmark authority; Criterion covers Rust-local microbenchmarks.
- Kubernetes manifests are incomplete until a kind deployment, API exercise, pod replacement, and persistence check have passed.

### Validation

- Docker daemon connectivity: passed.
- Git remote clone: passed; repository confirmed empty.
- Initial environment audit: passed with the Python/kind gaps described above.

## 2026-11-15 — Mathematical core and HNSW

### Implemented

- Added finite, non-empty dense vectors with explicit zero-vector cosine semantics.
- Added cosine, Euclidean, and dot-product scoring with a uniform higher-is-better API convention.
- Added exhaustive top-k search with stable score/ID ordering.
- Added a serializable, incremental multilayer HNSW graph implemented directly in Rust.
- Added configurable M, construction/search breadth, deterministic levels, logical deletion, and compaction by rebuild.

### Validation

- `cargo test --workspace`: 12 passed, 0 failed, including property tests and measured HNSW recall@10 against exact search on 500 deterministic random vectors.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all`: passed.

### Notes

- The recall assertion uses a high-recall HNSW configuration and requires at least 0.90 recall@10. Performance/recall trade-off curves remain a separate ANN-Benchmarks release gate.

## 2026-11-15 — RocksDB storage and query engine

### Implemented

- Installed local LLVM/libclang 23.1.5 and CMake 4.4.4 to build the native RocksDB binding.
- Made RocksDB the concrete default backend with versioned, length-prefixed keys.
- Added synchronous WAL-backed atomic batches for collection lifecycle, batch upsert, update, and delete.
- Added deterministic record pagination and index reconstruction from durable records.
- Added a collection/query engine with exact and approximate modes, nested metadata fields, and boolean equality filters (AND/OR/NOT).
- Filtered searches use exact pre-filtering, ensuring all returned records match and fewer than k are returned only when the candidate set is smaller.

### Validation

- `cargo test --workspace`: 17 passed, 0 failed.
- Storage tests close and reopen RocksDB before asserting acknowledged collection/record recovery.
- Query tests update and delete vectors, close the engine, reconstruct HNSW, and successfully search recovered data.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.

## 2026-11-15 — Python SDK

### Implemented

- Added an installable Python 3.12+ `axion-client` package with typed Pydantic models.
- Added synchronous and asynchronous httpx clients, context management, bearer auth, configurable timeouts, and helpful structured exceptions.
- Retries are bounded and restricted to reads, search, idempotent upserts, and deletes; collection creation is intentionally not retried.
- Added a sentence-transformers semantic-search example with embedding generation kept outside Axion.

### Validation

- Installed the package in a clean Python 3.12 virtual environment.
- Ruff lint and format checks: passed.
- Pytest: 4 passed, covering sync/async behavior, serialization, auth, safe retry semantics, structured errors, and finite-vector validation.
- Built `axion_client-0.1.0-py3-none-any.whl` successfully.
- Live SDK-to-server smoke test: created `sdk-smoke`, upserted `py-1`, searched it, and round-tripped `{"client":"python"}` metadata.


## 2026-11-15 — REST API

### Implemented

- Added versioned collection, vector, batch, pagination, delete, and search routes in Axum.
- Added structured JSON errors, JSON-rejection handling, optional constant-time bearer authentication, request body and batch limits, configurable CORS, request IDs, tracing, and graceful shutdown.
- Added liveness, storage-backed readiness, Prometheus metrics, a versioned OpenAPI 3.1 contract, and Swagger UI entry point.
- Blocking RocksDB work runs on Tokio's blocking pool instead of executor workers.

### Validation

- `cargo test --workspace`: 19 passed, 0 failed.
- REST integration tests use a temporary real RocksDB database and cover CRUD/search, filtering, invalid JSON values, authentication, health, and metrics.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Live TCP smoke test on `127.0.0.1:18080`: created collection `smoke`, durably inserted `v1`, and returned it at cosine score `1.0` from approximate search.

## 2026-11-15 — gRPC API

### Implemented

- Added a versioned `axion.v1.Axion` protobuf service for collection lifecycle, vector CRUD/listing, batch upsert, and search.
- Structured metadata and boolean filters cross the protobuf boundary as validated JSON strings so their full JSON types are preserved.
- Added consistent gRPC status mapping for validation, not-found, duplicate, and internal storage failures.
- The server process now runs REST and gRPC listeners together over one durable engine and coordinates graceful shutdown.
- Protobuf compilation uses a vendored `protoc`, keeping fresh builds independent of host installation.

### Validation

- `cargo test --workspace`: 20 passed, 0 failed.
- The gRPC integration test starts a real ephemeral TCP/HTTP2 server, connects the generated client, creates a collection, upserts a vector, searches it, and verifies structured metadata round-trip.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.




