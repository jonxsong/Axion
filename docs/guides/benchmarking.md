# Benchmarking

Axion separates component measurements from comparative search evaluation.

## Criterion

Rust microbenchmarks measure exact top-k, HNSW construction, and HNSW search at multiple `ef_search` values:

```powershell
cargo bench --package axion-index --bench search
```

Criterion reports are local component measurements. They are not claims about HTTP throughput or another implementation.

## Official ANN-Benchmarks

Axion includes an adapter for the official `erikbern/ann-benchmarks` runner, pinned to framework revision `2e081ad32c1eccab72dcb739ad886c310b90f715`. The workflow uses standardized datasets and compares recall versus queries per second against hnswlib under the same host and CPU constraints.

```powershell
make ann-benchmarks
```

The default is `random-xs-20-euclidean`, three timed runs, top 10. Set `AXION_ANN_DATASET` and `AXION_ANN_RUNS` to change them. Release-scale runs can use `fashion-mnist-784-euclidean` but take substantially longer.

Committed evidence under `benchmarks/results/20261115-224055/` contains raw CSV, the framework plot, exact revisions, host metadata, and interpretation. Axion's adapter uses HTTP plus RocksDB while hnswlib is in-process, so treat the result as an end-to-end systems comparison, not a pure index-only contest. Never compare QPS gathered on different hosts.
