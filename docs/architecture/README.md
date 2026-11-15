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

Detailed component, persistence, recovery, query, and distributed-search diagrams will evolve with the corresponding tested implementation. The public SVG is generated only from the architecture that exists in code.

