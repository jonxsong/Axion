//! Axion's from-first-principles Hierarchical Navigable Small World index.
//!
//! The graph stores stable node slots, uses greedy descent through upper layers,
//! and performs a bounded best-first search on the selected layer. Logical
//! deletion never returns a deleted node; [`HnswIndex::rebuilt`] compacts one.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

use axion_core::{CoreError, DenseVector, DistanceMetric, SearchResult};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("HNSW parameter m must be at least 2")]
    InvalidM,
    #[error("HNSW ef_construction must be at least m")]
    InvalidEfConstruction,
    #[error("HNSW ef_search must be greater than zero")]
    InvalidEfSearch,
    #[error("a vector with ID '{0}' already exists")]
    DuplicateId(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HnswConfig {
    pub m: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
    pub random_seed: u64,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construction: 200,
            ef_search: 64,
            random_seed: 0xA710_0001,
        }
    }
}

impl HnswConfig {
    /// Validates construction and search bounds.
    ///
    /// # Errors
    ///
    /// Returns a parameter-specific [`IndexError`] for an invalid bound.
    pub fn validate(&self) -> Result<(), IndexError> {
        if self.m < 2 {
            return Err(IndexError::InvalidM);
        }
        if self.ef_construction < self.m {
            return Err(IndexError::InvalidEfConstruction);
        }
        if self.ef_search == 0 {
            return Err(IndexError::InvalidEfSearch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Node {
    id: String,
    vector: DenseVector,
    /// One adjacency list per layer, starting at layer zero.
    neighbors: Vec<Vec<usize>>,
    deleted: bool,
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    node: usize,
    score: f32,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.score.to_bits() == other.score.to_bits()
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.node.cmp(&self.node))
    }
}

/// Serializable HNSW graph. The random generator is restored from a seed and
/// insertion count; graph topology itself is fully persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswIndex {
    dimensions: usize,
    metric: DistanceMetric,
    config: HnswConfig,
    nodes: Vec<Node>,
    entry_point: Option<usize>,
    max_level: usize,
    insertion_count: u64,
}

impl HnswIndex {
    /// Creates an empty graph.
    ///
    /// # Errors
    ///
    /// Returns an error for zero dimensions or invalid configuration.
    pub fn new(
        dimensions: usize,
        metric: DistanceMetric,
        config: HnswConfig,
    ) -> Result<Self, IndexError> {
        config.validate()?;
        if dimensions == 0 {
            return Err(CoreError::EmptyVector.into());
        }
        Ok(Self {
            dimensions,
            metric,
            config,
            nodes: Vec::new(),
            entry_point: None,
            max_level: 0,
            insertion_count: 0,
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|node| !node.deleted).count()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Inserts one vector and incrementally connects it to the graph.
    ///
    /// # Errors
    ///
    /// Returns an error for a dimension mismatch or duplicate live ID.
    pub fn insert(&mut self, id: String, vector: DenseVector) -> Result<(), IndexError> {
        self.validate_dimensions(&vector)?;
        if self.nodes.iter().any(|node| node.id == id && !node.deleted) {
            return Err(IndexError::DuplicateId(id));
        }

        let level = self.random_level();
        let new_index = self.nodes.len();
        self.nodes.push(Node {
            id,
            vector,
            neighbors: vec![Vec::new(); level + 1],
            deleted: false,
        });
        self.insertion_count += 1;

        let Some(mut entry) = self.entry_point else {
            self.entry_point = Some(new_index);
            self.max_level = level;
            return Ok(());
        };

        // Descend greedily through layers that the new node does not occupy.
        for layer in ((level + 1)..=self.max_level).rev() {
            entry = self.greedy_closest(&self.nodes[new_index].vector, entry, layer)?;
        }

        // At each shared layer, connect to the best bounded candidate set.
        for layer in (0..=level.min(self.max_level)).rev() {
            let candidates = self.search_layer(
                &self.nodes[new_index].vector,
                &[entry],
                self.config.ef_construction,
                layer,
            )?;
            let selected = candidates
                .into_iter()
                .filter(|candidate| candidate.node != new_index)
                .take(self.max_connections(layer))
                .map(|candidate| candidate.node)
                .collect::<Vec<_>>();
            self.nodes[new_index].neighbors[layer].clone_from(&selected);
            for neighbor in selected {
                self.nodes[neighbor].neighbors[layer].push(new_index);
                self.prune_neighbors(neighbor, layer);
            }
            if let Some(closest) = self.nodes[new_index].neighbors[layer].first() {
                entry = *closest;
            }
        }

        if level > self.max_level {
            self.entry_point = Some(new_index);
            self.max_level = level;
        }
        Ok(())
    }

    /// Searches the graph using greedy upper-layer traversal and bounded
    /// best-first layer-zero expansion.
    ///
    /// # Errors
    ///
    /// Returns an error for a dimension mismatch or `top_k == 0`.
    pub fn search(
        &self,
        query: &DenseVector,
        top_k: usize,
        ef_search: Option<usize>,
    ) -> Result<Vec<SearchResult>, IndexError> {
        self.validate_dimensions(query)?;
        if top_k == 0 {
            return Err(CoreError::InvalidTopK.into());
        }
        let Some(mut entry) = self.live_entry_point() else {
            return Ok(Vec::new());
        };

        for layer in (1..=self.max_level).rev() {
            if self.nodes[entry].neighbors.len() > layer {
                entry = self.greedy_closest(query, entry, layer)?;
            }
        }
        let ef = ef_search.unwrap_or(self.config.ef_search).max(top_k);
        let mut results = self.search_layer(query, &[entry], ef, 0)?;
        results.retain(|candidate| !self.nodes[candidate.node].deleted);
        results.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| self.nodes[left.node].id.cmp(&self.nodes[right.node].id))
        });
        results.truncate(top_k);
        Ok(results
            .into_iter()
            .map(|candidate| SearchResult {
                id: self.nodes[candidate.node].id.clone(),
                score: candidate.score,
                metadata: serde_json::Map::new(),
            })
            .collect())
    }

    /// Marks an ID deleted. Returns whether a live node was found.
    pub fn delete(&mut self, id: &str) -> bool {
        if let Some(node) = self
            .nodes
            .iter_mut()
            .find(|node| node.id == id && !node.deleted)
        {
            node.deleted = true;
            true
        } else {
            false
        }
    }

    /// Reconstructs a compact graph from all live vectors.
    ///
    /// # Errors
    ///
    /// Returns an index error if a retained vector cannot be reinserted.
    pub fn rebuilt(&self) -> Result<Self, IndexError> {
        let mut rebuilt = Self::new(self.dimensions, self.metric, self.config.clone())?;
        for node in self.nodes.iter().filter(|node| !node.deleted) {
            rebuilt.insert(node.id.clone(), node.vector.clone())?;
        }
        Ok(rebuilt)
    }

    fn validate_dimensions(&self, vector: &DenseVector) -> Result<(), IndexError> {
        if vector.dimensions() != self.dimensions {
            return Err(CoreError::DimensionMismatch {
                expected: self.dimensions,
                actual: vector.dimensions(),
            }
            .into());
        }
        Ok(())
    }

    fn random_level(&self) -> usize {
        // Re-seeding by insertion ordinal makes a deserialized index continue
        // deterministically without serializing RNG internals.
        let mut rng = ChaCha8Rng::seed_from_u64(
            self.config.random_seed ^ self.insertion_count.wrapping_mul(0x9E37_79B9),
        );
        let mut level = 0;
        while level < 32 && rng.random_range(0..self.config.m) == 0 {
            level += 1;
        }
        level
    }

    fn live_entry_point(&self) -> Option<usize> {
        self.entry_point
            .filter(|index| !self.nodes[*index].deleted)
            .or_else(|| self.nodes.iter().position(|node| !node.deleted))
    }

    fn max_connections(&self, layer: usize) -> usize {
        if layer == 0 {
            self.config.m * 2
        } else {
            self.config.m
        }
    }

    fn score(&self, query: &DenseVector, node: usize) -> Result<f32, IndexError> {
        Ok(self.metric.score(query, &self.nodes[node].vector)?)
    }

    fn greedy_closest(
        &self,
        query: &DenseVector,
        mut current: usize,
        layer: usize,
    ) -> Result<usize, IndexError> {
        let mut current_score = self.score(query, current)?;
        loop {
            let mut improved = false;
            if let Some(neighbors) = self.nodes[current].neighbors.get(layer) {
                for &neighbor in neighbors {
                    if self.nodes[neighbor].deleted {
                        continue;
                    }
                    let score = self.score(query, neighbor)?;
                    if score > current_score
                        || (score.total_cmp(&current_score) == Ordering::Equal
                            && self.nodes[neighbor].id < self.nodes[current].id)
                    {
                        current = neighbor;
                        current_score = score;
                        improved = true;
                    }
                }
            }
            if !improved {
                return Ok(current);
            }
        }
    }

    fn search_layer(
        &self,
        query: &DenseVector,
        entry_points: &[usize],
        ef: usize,
        layer: usize,
    ) -> Result<Vec<Candidate>, IndexError> {
        let mut visited = HashSet::with_capacity(ef.saturating_mul(4));
        let mut frontier = BinaryHeap::new();
        // Reverse score ordering turns this into a min-heap of retained results.
        let mut retained = BinaryHeap::new();

        for &entry in entry_points {
            if !visited.insert(entry) {
                continue;
            }
            let candidate = Candidate {
                node: entry,
                score: self.score(query, entry)?,
            };
            frontier.push(candidate);
            retained.push(std::cmp::Reverse(candidate));
        }

        while let Some(current) = frontier.pop() {
            let worst_score = retained
                .peek()
                .map_or(f32::NEG_INFINITY, |item| item.0.score);
            if retained.len() >= ef && current.score < worst_score {
                break;
            }
            if let Some(neighbors) = self.nodes[current.node].neighbors.get(layer) {
                for &neighbor in neighbors {
                    if !visited.insert(neighbor) {
                        continue;
                    }
                    let candidate = Candidate {
                        node: neighbor,
                        score: self.score(query, neighbor)?,
                    };
                    let worst_score = retained
                        .peek()
                        .map_or(f32::NEG_INFINITY, |item| item.0.score);
                    if retained.len() < ef || candidate.score > worst_score {
                        frontier.push(candidate);
                        retained.push(std::cmp::Reverse(candidate));
                        if retained.len() > ef {
                            retained.pop();
                        }
                    }
                }
            }
        }

        let mut output = retained
            .into_iter()
            .map(|candidate| candidate.0)
            .collect::<Vec<_>>();
        output.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| self.nodes[left.node].id.cmp(&self.nodes[right.node].id))
        });
        Ok(output)
    }

    fn prune_neighbors(&mut self, node: usize, layer: usize) {
        let limit = self.max_connections(layer);
        if self.nodes[node].neighbors[layer].len() <= limit {
            return;
        }
        let reference = self.nodes[node].vector.clone();
        let mut neighbors = std::mem::take(&mut self.nodes[node].neighbors[layer]);
        neighbors.sort_by(|left, right| {
            let left_score = self
                .metric
                .score(&reference, &self.nodes[*left].vector)
                .expect("all indexed vectors share dimensions");
            let right_score = self
                .metric
                .score(&reference, &self.nodes[*right].vector)
                .expect("all indexed vectors share dimensions");
            right_score
                .total_cmp(&left_score)
                .then_with(|| self.nodes[*left].id.cmp(&self.nodes[*right].id))
        });
        neighbors.dedup();
        neighbors.truncate(limit);
        self.nodes[node].neighbors[layer] = neighbors;
    }
}

#[cfg(test)]
mod tests {
    use axion_core::{VectorRecord, exact_search};
    use serde_json::Map;

    use super::*;

    fn vector(values: Vec<f32>) -> DenseVector {
        DenseVector::new(values).unwrap()
    }

    #[test]
    fn validates_configuration_and_dimensions() {
        let error = HnswIndex::new(
            2,
            DistanceMetric::Cosine,
            HnswConfig {
                m: 1,
                ..HnswConfig::default()
            },
        )
        .unwrap_err();
        assert!(matches!(error, IndexError::InvalidM));

        let mut index = HnswIndex::new(2, DistanceMetric::Cosine, HnswConfig::default()).unwrap();
        let error = index.insert("bad".into(), vector(vec![1.0])).unwrap_err();
        assert!(matches!(
            error,
            IndexError::Core(CoreError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn inserts_searches_deletes_and_rebuilds() {
        let mut index =
            HnswIndex::new(2, DistanceMetric::Euclidean, HnswConfig::default()).unwrap();
        index
            .insert("origin".into(), vector(vec![0.0, 0.0]))
            .unwrap();
        index.insert("near".into(), vector(vec![0.1, 0.1])).unwrap();
        index.insert("far".into(), vector(vec![9.0, 9.0])).unwrap();

        let results = index.search(&vector(vec![0.0, 0.0]), 2, None).unwrap();
        assert_eq!(results[0].id, "origin");
        assert!(index.delete("origin"));
        assert!(!index.delete("origin"));
        assert!(
            index
                .search(&vector(vec![0.0, 0.0]), 3, None)
                .unwrap()
                .iter()
                .all(|result| result.id != "origin")
        );

        let rebuilt = index.rebuilt().unwrap();
        assert_eq!(rebuilt.len(), 2);
        assert_eq!(
            rebuilt.search(&vector(vec![0.0, 0.0]), 1, None).unwrap()[0].id,
            "near"
        );
    }

    #[test]
    fn approximate_recall_is_measured_against_exact_search() {
        let config = HnswConfig {
            m: 32,
            ef_construction: 300,
            ef_search: 256,
            random_seed: 42,
        };
        let mut index = HnswIndex::new(8, DistanceMetric::Euclidean, config).unwrap();
        let mut records = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(8_675_309);
        for row in 0..500_u32 {
            let values = (0..8)
                .map(|_| rng.random_range(-1.0_f32..1.0))
                .collect::<Vec<_>>();
            let id = format!("v-{row:04}");
            let dense = vector(values);
            index.insert(id.clone(), dense.clone()).unwrap();
            records.push(VectorRecord {
                id,
                values: dense,
                metadata: Map::new(),
            });
        }

        let mut hits = 0_u32;
        let mut total = 0_u32;
        for query_index in (0..500).step_by(17) {
            let query = &records[query_index].values;
            let exact =
                exact_search(query, records.clone(), DistanceMetric::Euclidean, 10).unwrap();
            let approximate = index.search(query, 10, Some(256)).unwrap();
            let expected = exact
                .iter()
                .map(|result| &result.id)
                .collect::<HashSet<_>>();
            hits += u32::try_from(
                approximate
                    .iter()
                    .filter(|result| expected.contains(&result.id))
                    .count(),
            )
            .unwrap();
            total += 10;
        }
        let recall = f64::from(hits) / f64::from(total);
        assert!(recall >= 0.90, "recall@10 was {recall:.3}");
    }

    #[test]
    fn serialized_graph_preserves_search_results() {
        let mut index = HnswIndex::new(2, DistanceMetric::Dot, HnswConfig::default()).unwrap();
        index.insert("x".into(), vector(vec![1.0, 0.0])).unwrap();
        index.insert("y".into(), vector(vec![0.0, 1.0])).unwrap();
        let json = serde_json::to_vec(&index).unwrap();
        let restored: HnswIndex = serde_json::from_slice(&json).unwrap();
        assert_eq!(
            index.search(&vector(vec![1.0, 0.0]), 2, None).unwrap(),
            restored.search(&vector(vec![1.0, 0.0]), 2, None).unwrap()
        );
    }
}
