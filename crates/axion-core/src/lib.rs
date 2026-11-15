//! Mathematical foundations and shared data types for Axion.
//!
//! All public search scores use one convention: larger values are better.
//! Cosine and dot-product scores are returned directly; Euclidean distance is
//! negated. Vectors are validated at construction, so downstream indexing code
//! never has to order NaN values.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

/// Errors returned by vector validation and search.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("vectors must contain at least one dimension")]
    EmptyVector,
    #[error("vector component at position {index} is not finite")]
    NonFinite { index: usize },
    #[error("dimension mismatch: expected {expected}, received {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
    #[error("top_k must be greater than zero")]
    InvalidTopK,
}

/// A dense, finite, non-empty vector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DenseVector(Vec<f32>);

impl DenseVector {
    /// Validates and constructs a dense vector.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::EmptyVector`] for no components or
    /// [`CoreError::NonFinite`] for a NaN or infinity.
    pub fn new(values: Vec<f32>) -> Result<Self, CoreError> {
        if values.is_empty() {
            return Err(CoreError::EmptyVector);
        }
        if let Some(index) = values.iter().position(|value| !value.is_finite()) {
            return Err(CoreError::NonFinite { index });
        }
        Ok(Self(values))
    }

    #[must_use]
    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }

    #[must_use]
    pub fn dimensions(&self) -> usize {
        self.0.len()
    }

    /// Returns a unit-length copy. The all-zero vector remains all zero.
    #[must_use]
    pub fn normalized(&self) -> Self {
        let norm_squared = self.0.iter().map(|value| value * value).sum::<f32>();
        if norm_squared == 0.0 {
            return self.clone();
        }
        let inverse_norm = norm_squared.sqrt().recip();
        Self(self.0.iter().map(|value| value * inverse_norm).collect())
    }
}

/// Metric configured for a collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    #[default]
    Cosine,
    Euclidean,
    Dot,
}

impl DistanceMetric {
    /// Calculates a comparable score where larger is always better.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::DimensionMismatch`] when vector lengths differ.
    pub fn score(self, left: &DenseVector, right: &DenseVector) -> Result<f32, CoreError> {
        ensure_same_dimensions(left, right)?;
        let score = match self {
            Self::Cosine => cosine_similarity(left, right),
            Self::Euclidean => -euclidean_distance(left, right),
            Self::Dot => dot_product(left, right),
        };
        Ok(score)
    }
}

/// User record stored in a collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorRecord {
    pub id: String,
    pub values: DenseVector,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

/// Stable public search result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

/// Checks that two vectors can participate in a metric operation.
///
/// # Errors
///
/// Returns [`CoreError::DimensionMismatch`] when vector lengths differ.
pub fn ensure_same_dimensions(left: &DenseVector, right: &DenseVector) -> Result<(), CoreError> {
    if left.dimensions() != right.dimensions() {
        return Err(CoreError::DimensionMismatch {
            expected: left.dimensions(),
            actual: right.dimensions(),
        });
    }
    Ok(())
}

#[must_use]
pub fn dot_product(left: &DenseVector, right: &DenseVector) -> f32 {
    left.as_slice()
        .iter()
        .zip(right.as_slice())
        .map(|(left, right)| left * right)
        .sum()
}

#[must_use]
pub fn euclidean_distance(left: &DenseVector, right: &DenseVector) -> f32 {
    left.as_slice()
        .iter()
        .zip(right.as_slice())
        .map(|(left, right)| {
            let delta = left - right;
            delta * delta
        })
        .sum::<f32>()
        .sqrt()
}

/// Cosine similarity. Axion defines similarity involving a zero vector as 0.
#[must_use]
pub fn cosine_similarity(left: &DenseVector, right: &DenseVector) -> f32 {
    let left_norm = dot_product(left, left);
    let right_norm = dot_product(right, right);
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    dot_product(left, right) / (left_norm * right_norm).sqrt()
}

/// Performs an exhaustive scan with deterministic tie ordering by record ID.
///
/// # Errors
///
/// Returns [`CoreError::InvalidTopK`] for zero, or
/// [`CoreError::DimensionMismatch`] if any record differs from the query.
pub fn exact_search(
    query: &DenseVector,
    records: impl IntoIterator<Item = VectorRecord>,
    metric: DistanceMetric,
    top_k: usize,
) -> Result<Vec<SearchResult>, CoreError> {
    if top_k == 0 {
        return Err(CoreError::InvalidTopK);
    }

    let mut results = records
        .into_iter()
        .map(|record| {
            metric
                .score(query, &record.values)
                .map(|score| SearchResult {
                    id: record.id,
                    score,
                    metadata: record.metadata,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    results.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });
    results.truncate(top_k);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn vector(values: &[f32]) -> DenseVector {
        DenseVector::new(values.to_vec()).expect("test vector is valid")
    }

    fn record(id: &str, values: &[f32]) -> VectorRecord {
        VectorRecord {
            id: id.to_owned(),
            values: vector(values),
            metadata: Map::new(),
        }
    }

    #[test]
    fn validates_dense_vectors() {
        assert_eq!(DenseVector::new(vec![]), Err(CoreError::EmptyVector));
        assert_eq!(
            DenseVector::new(vec![1.0, f32::NAN]),
            Err(CoreError::NonFinite { index: 1 })
        );
        assert_eq!(
            DenseVector::new(vec![f32::INFINITY]),
            Err(CoreError::NonFinite { index: 0 })
        );
    }

    #[test]
    fn calculates_known_metric_values() {
        let x = vector(&[1.0, 0.0]);
        let y = vector(&[0.0, 1.0]);
        assert_eq!(cosine_similarity(&x, &x), 1.0);
        assert_eq!(cosine_similarity(&x, &y), 0.0);
        assert_eq!(dot_product(&x, &y), 0.0);
        assert!((euclidean_distance(&x, &y) - 2.0_f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn gives_zero_vectors_defined_cosine_semantics() {
        let zero = vector(&[0.0, 0.0]);
        let other = vector(&[1.0, 2.0]);
        assert_eq!(cosine_similarity(&zero, &zero), 0.0);
        assert_eq!(cosine_similarity(&zero, &other), 0.0);
        assert_eq!(zero.normalized(), zero);
    }

    #[test]
    fn rejects_dimension_mismatch() {
        let error = DistanceMetric::Cosine
            .score(&vector(&[1.0]), &vector(&[1.0, 2.0]))
            .expect_err("dimensions differ");
        assert_eq!(
            error,
            CoreError::DimensionMismatch {
                expected: 1,
                actual: 2
            }
        );
    }

    #[test]
    fn exact_search_is_deterministic_for_ties_and_duplicates() {
        let records = vec![
            record("z", &[1.0, 0.0]),
            record("a", &[1.0, 0.0]),
            record("middle", &[0.5, 0.5]),
        ];
        let results = exact_search(&vector(&[1.0, 0.0]), records, DistanceMetric::Cosine, 3)
            .expect("search succeeds");
        assert_eq!(
            results
                .iter()
                .map(|result| result.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "z", "middle"]
        );
    }

    #[test]
    fn exact_search_handles_empty_input_and_invalid_top_k() {
        assert_eq!(
            exact_search(&vector(&[1.0]), vec![], DistanceMetric::Dot, 2)
                .expect("empty scan succeeds"),
            []
        );
        assert_eq!(
            exact_search(&vector(&[1.0]), vec![], DistanceMetric::Dot, 0),
            Err(CoreError::InvalidTopK)
        );
    }

    proptest! {
        #[test]
        fn euclidean_is_symmetric_and_non_negative(
            left in prop::collection::vec(-10.0_f32..10.0, 1..64),
            right_seed in prop::collection::vec(-10.0_f32..10.0, 1..64),
        ) {
            let right = (0..left.len())
                .map(|index| right_seed[index % right_seed.len()])
                .collect::<Vec<_>>();
            let left = DenseVector::new(left).unwrap();
            let right = DenseVector::new(right).unwrap();
            let forward = euclidean_distance(&left, &right);
            let reverse = euclidean_distance(&right, &left);
            prop_assert!(forward >= 0.0);
            prop_assert!((forward - reverse).abs() < 1e-5);
        }

        #[test]
        fn normalized_non_zero_vectors_have_unit_length(
            values in prop::collection::vec(-10.0_f32..10.0, 1..64)
                .prop_filter("must have magnitude", |values| values.iter().any(|v| v.abs() > 1e-4)),
        ) {
            let normalized = DenseVector::new(values).unwrap().normalized();
            let length = dot_product(&normalized, &normalized).sqrt();
            prop_assert!((length - 1.0).abs() < 1e-4, "length={length}");
        }
    }
}
