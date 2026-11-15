# Troubleshooting

## Server will not become ready

Check `docker compose logs axion` or the pod logs. `/health/live` confirms the process; `/health/ready` also checks storage. An unknown schema version or a non-lock RocksDB error is intentionally fatal.

## RocksDB reports a lock

Only one server may own an `AXION_DATA_DIR`. Stop the other process or correct the volume mount. Axion retries lock errors for at most `AXION_STORAGE_OPEN_TIMEOUT_SECS` to cover orchestrator handoff; it does not bypass the lock.

## Native Rust build cannot find libclang

Install Clang, libclang, and CMake. On Windows, if LLVM is installed outside the process path, set `LIBCLANG_PATH` to its `bin` directory for the build shell. The production Dockerfile installs the required Debian packages.

## Search returns fewer than `top_k`

The collection may contain fewer records, a metadata filter may match fewer records, or a distributed response may be partial. Inspect `partial`, `successful_shards`, and `failures` on coordinator responses. Increase `ef_search` to trade approximate-query throughput for recall; use `mode: exact` to establish ground truth.

## Web console cannot reach the API

The production web image bakes `AXION_API_URL` at build time and proxies `/api/*` to that backend. In Compose it is `http://axion:8080`. Rebuild the web image after changing the backend URL and confirm the backend health endpoint is reachable from the web container.

## Kubernetes PVC or replacement pod is stuck

Axion requires single-writer storage. Use `ReadWriteOnce` and the shipped `Recreate` deployment strategy. Inspect PVC binding, old pod termination, and new pod events. Run `./scripts/kind-smoke.ps1` to reproduce the supported local path.
