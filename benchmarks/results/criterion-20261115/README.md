# Criterion run — 2026-11-15

Command: `cargo bench --locked -p axion-index --bench search`

Host: AMD Ryzen 7 7800X3D (8 cores / 16 logical processors), 50.7 GB RAM,
Windows, Rust 1.100.0. Criterion 0.7.0 used the Plotters backend. Docker
services were running, so these figures are an implementation checkpoint, not
a cross-product comparison.

| Benchmark | Median estimate | Throughput |
| --- | ---: | ---: |
| Exact cosine, 10k × 64d, top-10 | 2.161 ms | 462.8 queries/s |
| HNSW cosine, 10k × 64d, top-10, ef=16 | 62.09 µs | 16,106 queries/s |
| HNSW cosine, 10k × 64d, top-10, ef=32 | 104.72 µs | 9,550 queries/s |
| HNSW cosine, 10k × 64d, top-10, ef=64 | 204.00 µs | 4,902 queries/s |
| HNSW cosine, 10k × 64d, top-10, ef=128 | 370.44 µs | 2,700 queries/s |
| HNSW construction, 1k × 64d, M=16, efConstruction=200 | 366.21 ms | 2,731 vectors/s |

The adjacent JSON files are Criterion's raw confidence-interval and point
estimate artifacts for this run. Search quality is deliberately not inferred
from these microbenchmarks; the official ANN-Benchmarks run owns recall-versus-
QPS evaluation.

