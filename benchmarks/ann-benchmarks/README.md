# Official ANN-Benchmarks integration

Axion is integrated as a native algorithm definition for the official
[`erikbern/ann-benchmarks`](https://github.com/erikbern/ann-benchmarks)
framework. The adapter starts an isolated Axion server, loads the framework's
standard train split through batch writes, and returns integer candidates from
approximate searches. RocksDB remains the storage backend during the run.

`scripts/run-ann-benchmarks.ps1` pins the framework revision, stages this
adapter into an unmodified clone, builds the official Docker runners, and runs
Axion and hnswlib on the same `random-xs-20-euclidean` dataset. The framework
itself calculates recall and queries per second. It writes the CSV and
recall-versus-QPS plot to a dated directory under `benchmarks/results/`.

Run the complete comparison with:

```powershell
make ann-benchmarks
```

For release-scale reporting, set `AXION_ANN_DATASET` to another official
dataset such as `fashion-mnist-784-euclidean`. Never compare results from
different hosts or runner settings.

