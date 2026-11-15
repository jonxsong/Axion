# Storage and recovery

RocksDB is Axion's default and currently only production storage backend. Collection definitions use `c + u32(name_length) + name`; vector records use `r + u32(collection_length) + collection + id`. Length-prefixing prevents one collection's prefix scan from crossing into another collection whose name shares a textual prefix.

## Durability contract

Every acknowledged mutation is a RocksDB `WriteBatch` issued with synchronous WAL writes. Collection deletion and all of its current records are deleted in one batch. A successful batch means RocksDB completed its synchronous WAL operation; physical-device guarantees remain bounded by the host, filesystem, and device honoring flush requests.

The HNSW graph is derived state. On process start Axion reads every durable record in deterministic ID order and reconstructs each graph. A crash after a durable record batch therefore cannot permanently orphan an index update. Future persisted graph snapshots may reduce startup time, but records remain authoritative.

## Format compatibility

The database contains `m/schema-version`. Axion refuses to open an unknown version rather than guessing at its layout. Decoding failures are surfaced as errors and do not silently drop records.

