# ANN-Benchmarks result — 2026-11-15

This is an official `erikbern/ann-benchmarks` run at pinned framework commit
`2e081ad32c1eccab72dcb739ad886c310b90f715` and Axion commit
`864fba79b47859ae9962d07c49efec2a5192205b`.

- Dataset: `random-xs-20-euclidean` (9,000 train vectors, 1,000 queries,
  20 dimensions)
- Search: top 10, three runs per setting, one pinned CPU per algorithm
- Construction: M=16, efConstruction=200
- Host: AMD Ryzen 7 7800X3D; 8 physical cores; 50.7 GB RAM

| Implementation | Query breadth | Recall@10 | Queries/s |
| --- | ---: | ---: | ---: |
| Axion | 8 | 0.9840 | 3,873.9 |
| Axion | 16 | 0.9880 | 3,730.2 |
| Axion | 32 | 0.9990 | 3,440.4 |
| Axion | 64 | 1.0000 | 3,217.6 |
| Axion | 128 | 1.0000 | 2,662.5 |
| Axion | 256 | 1.0000 | 2,030.4 |
| hnswlib | 8 | 0.9724 | 158,887.2 |
| hnswlib | 16 | 0.9957 | 123,151.8 |
| hnswlib | 32 | 0.9999 | 96,338.8 |
| hnswlib | 64 | 1.0000 | 64,777.9 |
| hnswlib | 128 | 1.0000 | 33,185.1 |
| hnswlib | 256 | 1.0000 | 17,441.2 |

Axion is measured through its real HTTP API and RocksDB-backed collection
engine. The hnswlib adapter calls its in-process native binding, so the QPS
figures are a transparent end-to-end service comparison rather than an
index-kernel-only claim. See `results.csv` for unrounded metrics,
`run-metadata.json` for provenance, and `recall-vs-qps.png` for the plot emitted
by the framework.
