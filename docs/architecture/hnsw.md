# HNSW design

Axion implements Hierarchical Navigable Small World search in Rust rather than wrapping a native ANN library. Nodes receive deterministic random levels, upper layers use greedy descent, and layer zero uses bounded best-first exploration. Neighbor selection and pruning preserve stable ordering so tests and rebuilds are reproducible.

`m` bounds neighborhood connectivity. `ef_construction` controls candidate breadth while inserting, and `ef_search` controls breadth while querying. Larger values generally improve recall while increasing CPU work and, for construction parameters, memory.

Exact search is the correctness oracle. The test suite compares approximate results with exact top-k on seeded datasets, covers insertion/deletion/rebuild behavior, and includes a reachability regression. Criterion measures both algorithms, while official ANN-Benchmarks computes comparative recall and QPS.

The graph is derived state. Records and collection configuration are synchronously persisted in RocksDB; startup reconstructs HNSW from records in deterministic ID order. Logical deletion excludes a node immediately, and rebuild compacts deleted nodes. A future graph snapshot may improve startup speed, but it will not replace RocksDB records as authoritative state.

Filtered queries currently scan and exactly score only matching records. This guarantees boolean-filter correctness; metadata indexes may optimize candidate selection later without changing the contract.
