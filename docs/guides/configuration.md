# Configuration reference

Axion reads configuration from environment variables and fails closed on unknown RocksDB schema versions. RocksDB is the default and only production storage backend.

| Variable | Default | Purpose |
| --- | --- | --- |
| `AXION_HOST` | `127.0.0.1` | REST and gRPC bind address |
| `AXION_HTTP_PORT` | `8080` | REST, health, metrics, and docs port |
| `AXION_GRPC_PORT` | `50051` | gRPC port |
| `AXION_DATA_DIR` | `./data` | RocksDB directory; one process may own it |
| `AXION_API_KEY` | empty | Optional bearer token for data APIs |
| `AXION_CORS_ALLOW_ANY` | `false` | Allow any browser origin; avoid in exposed deployments |
| `AXION_STORAGE_OPEN_TIMEOUT_SECS` | `30` | Bounded retry window for a retiring RocksDB lock |
| `RUST_LOG` | `axion=info` | Structured-log filter |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | empty | Optional OTLP/gRPC trace endpoint |
| `OTEL_SERVICE_NAME` | `axion-server` in Compose | Service name consumed by telemetry environments |

The optional coordinator uses `AXION_SHARD_NODES`, `AXION_MIN_SEARCH_SHARDS`, `AXION_CLUSTER_API_KEY`, `AXION_SHARD_API_KEY`, `AXION_SHARD_TIMEOUT_MS`, `AXION_CLUSTER_HOST`, and `AXION_CLUSTER_PORT`. See [distributed search](../architecture/distributed-search.md) before adopting it; changing the shard-name set changes ownership.

Collection-level HNSW defaults are `m=16`, `ef_construction=200`, `ef_search=64`, and deterministic seed `0xA7100001`. Override them in the collection creation body. `m` must be at least 2, `ef_construction` must be at least `m`, and per-query `ef_search` must be positive.

For an exposed deployment, set a long random API key through a secret manager, keep permissive CORS disabled, terminate TLS at an ingress or load balancer, use a dedicated persistent volume, and set resource limits. Health endpoints remain unauthenticated for orchestrator probes.
