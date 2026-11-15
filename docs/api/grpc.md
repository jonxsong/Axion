# gRPC API

The source contract is [`proto/axion/v1/axion.proto`](../../proto/axion/v1/axion.proto). The default listener is `127.0.0.1:50051`; set `AXION_HOST` and `AXION_GRPC_PORT` for container or cluster deployments.

The `axion.v1.Axion` service exposes collection create/list/get/delete, vector upsert/list/get/delete, and exact or approximate search. `metadata_json` accepts a JSON object. `filter_json` accepts the same externally tagged filter expression as REST, for example:

```json
{"and":{"filters":[{"eq":{"field":"category","value":"research"}},{"not":{"filter":{"eq":{"field":"archived","value":true}}}}]}}
```

Unset protobuf metric and search-mode enums default to cosine and approximate search respectively. A missing HNSW configuration uses the server defaults.

