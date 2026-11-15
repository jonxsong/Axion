use std::hint::black_box;
use std::time::Duration;

use axion_core::{DenseVector, DistanceMetric, VectorRecord, exact_search};
use axion_index::{HnswConfig, HnswIndex};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde_json::Map;

const DIMENSIONS: usize = 64;

fn dataset(size: usize) -> Vec<DenseVector> {
    let mut rng = ChaCha8Rng::seed_from_u64(0xA710_BEEF);
    (0..size)
        .map(|_| {
            DenseVector::new(
                (0..DIMENSIONS)
                    .map(|_| rng.random_range(-1.0_f32..1.0_f32))
                    .collect(),
            )
            .expect("generated vectors are finite")
        })
        .collect()
}

fn records(vectors: &[DenseVector]) -> Vec<VectorRecord> {
    vectors
        .iter()
        .enumerate()
        .map(|(index, vector)| VectorRecord {
            id: index.to_string(),
            values: vector.clone(),
            metadata: Map::new(),
        })
        .collect()
}

fn build_hnsw(vectors: &[DenseVector]) -> HnswIndex {
    let mut index = HnswIndex::new(
        DIMENSIONS,
        DistanceMetric::Cosine,
        HnswConfig {
            m: 16,
            ef_construction: 200,
            ef_search: 64,
            random_seed: 0xA710_BEEF,
        },
    )
    .expect("benchmark configuration is valid");
    for (id, vector) in vectors.iter().enumerate() {
        index
            .insert(id.to_string(), vector.clone())
            .expect("benchmark IDs are unique");
    }
    index
}

fn search_benchmarks(criterion: &mut Criterion) {
    let vectors = dataset(10_000);
    let records = records(&vectors);
    let index = build_hnsw(&vectors);
    let queries = dataset(100);

    let mut group = criterion.benchmark_group("search_10k_x_64d");
    group.throughput(Throughput::Elements(1));
    group.bench_function(BenchmarkId::new("exact_cosine", "top10"), |benchmark| {
        let mut query = 0;
        benchmark.iter(|| {
            let result = exact_search(
                black_box(&queries[query % queries.len()]),
                records.iter().cloned(),
                DistanceMetric::Cosine,
                10,
            )
            .expect("benchmark dimensions match");
            query += 1;
            black_box(result);
        });
    });
    for ef_search in [16, 32, 64, 128] {
        group.bench_function(
            BenchmarkId::new("hnsw_cosine", format!("top10_ef{ef_search}")),
            |benchmark| {
                let mut query = 0;
                benchmark.iter(|| {
                    let result = index
                        .search(
                            black_box(&queries[query % queries.len()]),
                            10,
                            Some(ef_search),
                        )
                        .expect("benchmark dimensions match");
                    query += 1;
                    black_box(result);
                });
            },
        );
    }
    group.finish();
}

fn construction_benchmark(criterion: &mut Criterion) {
    let vectors = dataset(1_000);
    let mut group = criterion.benchmark_group("construction_1k_x_64d");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Elements(vectors.len() as u64));
    group.bench_function("hnsw_m16_ef200", |benchmark| {
        benchmark.iter(|| black_box(build_hnsw(black_box(&vectors))));
    });
    group.finish();
}

criterion_group!(benches, search_benchmarks, construction_benchmark);
criterion_main!(benches);
