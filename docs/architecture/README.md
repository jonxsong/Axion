# Architecture

Axion follows a strict dependency direction:

```mermaid
flowchart LR
  C[REST / gRPC / Python / Web] --> A[API services]
  A --> Q[Collection and query engine]
  Q --> E[Exact search]
  Q --> H[HNSW search]
  Q --> F[Metadata filtering]
  Q --> S[(RocksDB)]
  A --> O[Metrics / traces / logs]
```

See [storage and recovery](storage.md) and [distributed search](distributed-search.md) for the tested persistence and multi-node consistency models. The public SVG documents only architecture that exists in code.

