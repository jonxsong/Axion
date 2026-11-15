# ADR 0001: RocksDB is the default storage engine

- Status: accepted
- Date: 2026-11-15

## Decision

Axion uses embedded RocksDB for its production single-node durable state. Writes use atomic batches and synchronous WAL behavior. HNSW is reconstructed from records at startup.

## Consequences

Axion gains an established WAL, ordered prefix iteration, atomic multi-key mutation, and crash recovery without outsourcing vector search. The native dependency increases build time and requires libclang/CMake. Since RocksDB locks its directory, only one Axion process may own a data path; distributed nodes must use distinct volumes and shard ownership.

