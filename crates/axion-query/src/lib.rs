//! Collection lifecycle and query planning over Axion's durable store.
//!
//! Unfiltered approximate queries use HNSW. Filtered queries pre-filter durable
//! records, then score exactly. This guarantees filter correctness and returns
//! fewer than `top_k` only when fewer matching records exist. Future metadata
//! indexes can replace the scan without changing these semantics.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use axion_core::{CoreError, DenseVector, SearchResult, VectorRecord, exact_search};
use axion_index::{HnswIndex, IndexError};
use axion_storage::{CollectionConfig, RocksStore, StorageError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QueryError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Index(#[from] IndexError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("collection '{0}' is not loaded")]
    CollectionNotLoaded(String),
    #[error("collection state lock is poisoned")]
    LockPoisoned,
    #[error("list limit must be between 1 and 1000")]
    InvalidListLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Exact,
    #[default]
    Approximate,
}

/// Boolean metadata expression. Dot-separated fields traverse nested objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterExpression {
    Eq { field: String, value: Value },
    And { filters: Vec<Self> },
    Or { filters: Vec<Self> },
    Not { filter: Box<Self> },
}

impl FilterExpression {
    #[must_use]
    pub fn matches(&self, metadata: &Map<String, Value>) -> bool {
        match self {
            Self::Eq { field, value } => metadata_value(metadata, field) == Some(value),
            Self::And { filters } => filters.iter().all(|filter| filter.matches(metadata)),
            Self::Or { filters } => filters.iter().any(|filter| filter.matches(metadata)),
            Self::Not { filter } => !filter.matches(metadata),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub vector: DenseVector,
    pub top_k: usize,
    #[serde(default)]
    pub mode: SearchMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<FilterExpression>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ef_search: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionSummary {
    #[serde(flatten)]
    pub config: CollectionConfig,
    pub vector_count: usize,
}

/// RocksDB-backed collection engine with reconstructable HNSW indexes.
#[derive(Clone)]
pub struct CollectionEngine {
    store: RocksStore,
    indexes: Arc<RwLock<BTreeMap<String, HnswIndex>>>,
}

impl CollectionEngine {
    /// Opens `RocksDB` and reconstructs every HNSW graph from durable records.
    ///
    /// # Errors
    ///
    /// Returns storage, decoding, or index reconstruction errors.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, QueryError> {
        let store = RocksStore::open(path)?;
        let mut indexes = BTreeMap::new();
        for config in store.list_collections()? {
            indexes.insert(config.name.clone(), build_index(&store, &config)?);
        }
        Ok(Self {
            store,
            indexes: Arc::new(RwLock::new(indexes)),
        })
    }

    /// Creates and loads a durable collection.
    ///
    /// # Errors
    ///
    /// Returns validation, duplicate, storage, index, or lock errors.
    pub fn create_collection(&self, config: CollectionConfig) -> Result<(), QueryError> {
        let index = HnswIndex::new(config.dimensions, config.metric, config.hnsw.clone())?;
        self.store.create_collection(&config)?;
        self.write_indexes()?.insert(config.name, index);
        Ok(())
    }

    /// Deletes the durable collection and then removes its derived index.
    ///
    /// # Errors
    ///
    /// Returns storage or lock errors.
    pub fn delete_collection(&self, name: &str) -> Result<(), QueryError> {
        self.store.delete_collection(name)?;
        self.write_indexes()?.remove(name);
        Ok(())
    }

    /// Gets a collection summary from durable state.
    ///
    /// # Errors
    ///
    /// Returns storage or lock errors.
    pub fn get_collection(&self, name: &str) -> Result<CollectionSummary, QueryError> {
        let config = self.store.get_collection(name)?;
        let vector_count = self
            .read_indexes()?
            .get(name)
            .ok_or_else(|| QueryError::CollectionNotLoaded(name.to_owned()))?
            .len();
        Ok(CollectionSummary {
            config,
            vector_count,
        })
    }

    /// Lists collection summaries in deterministic name order.
    ///
    /// # Errors
    ///
    /// Returns storage or lock errors.
    pub fn list_collections(&self) -> Result<Vec<CollectionSummary>, QueryError> {
        let configs = self.store.list_collections()?;
        let indexes = self.read_indexes()?;
        configs
            .into_iter()
            .map(|config| {
                let vector_count = indexes
                    .get(&config.name)
                    .ok_or_else(|| QueryError::CollectionNotLoaded(config.name.clone()))?
                    .len();
                Ok(CollectionSummary {
                    config,
                    vector_count,
                })
            })
            .collect()
    }

    /// Atomically persists a batch, then swaps in a prevalidated graph update.
    ///
    /// # Errors
    ///
    /// Returns collection, dimension, duplicate, storage, index, or lock errors.
    pub fn upsert(&self, collection: &str, records: &[VectorRecord]) -> Result<(), QueryError> {
        let config = self.store.get_collection(collection)?;
        for record in records {
            if record.values.dimensions() != config.dimensions {
                return Err(CoreError::DimensionMismatch {
                    expected: config.dimensions,
                    actual: record.values.dimensions(),
                }
                .into());
            }
        }
        let mut indexes = self.write_indexes()?;
        let current = indexes
            .get(collection)
            .ok_or_else(|| QueryError::CollectionNotLoaded(collection.to_owned()))?;
        let mut next = current.clone();
        for record in records {
            next.delete(&record.id);
            next.insert(record.id.clone(), record.values.clone())?;
        }
        self.store.upsert_records(collection, records)?;
        indexes.insert(collection.to_owned(), next);
        Ok(())
    }

    /// Gets one durable record.
    ///
    /// # Errors
    ///
    /// Returns storage errors.
    pub fn get_record(
        &self,
        collection: &str,
        id: &str,
    ) -> Result<Option<VectorRecord>, QueryError> {
        Ok(self.store.get_record(collection, id)?)
    }

    /// Lists durable records with cursor pagination.
    ///
    /// # Errors
    ///
    /// Returns an invalid-limit or storage error.
    pub fn list_records(
        &self,
        collection: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<VectorRecord>, QueryError> {
        if !(1..=1000).contains(&limit) {
            return Err(QueryError::InvalidListLimit);
        }
        Ok(self.store.list_records(collection, after, limit)?)
    }

    /// Deletes one record from durable and derived state.
    ///
    /// # Errors
    ///
    /// Returns collection, storage, or lock errors.
    pub fn delete_record(&self, collection: &str, id: &str) -> Result<bool, QueryError> {
        let mut indexes = self.write_indexes()?;
        let current = indexes
            .get(collection)
            .ok_or_else(|| QueryError::CollectionNotLoaded(collection.to_owned()))?;
        let mut next = current.clone();
        next.delete(id);
        let existed = self.store.delete_record(collection, id)?;
        if existed {
            indexes.insert(collection.to_owned(), next);
        }
        Ok(existed)
    }

    /// Executes exact, HNSW, or exact pre-filtered search.
    ///
    /// # Errors
    ///
    /// Returns validation, storage, index, or lock errors.
    pub fn search(
        &self,
        collection: &str,
        request: &SearchRequest,
    ) -> Result<Vec<SearchResult>, QueryError> {
        let config = self.store.get_collection(collection)?;
        if request.vector.dimensions() != config.dimensions {
            return Err(CoreError::DimensionMismatch {
                expected: config.dimensions,
                actual: request.vector.dimensions(),
            }
            .into());
        }
        if request.top_k == 0 {
            return Err(CoreError::InvalidTopK.into());
        }

        if request.mode == SearchMode::Exact || request.filter.is_some() {
            let records = all_records(&self.store, collection)?
                .into_iter()
                .filter(|record| {
                    request
                        .filter
                        .as_ref()
                        .is_none_or(|filter| filter.matches(&record.metadata))
                });
            return Ok(exact_search(
                &request.vector,
                records,
                config.metric,
                request.top_k,
            )?);
        }

        let indexes = self.read_indexes()?;
        let index = indexes
            .get(collection)
            .ok_or_else(|| QueryError::CollectionNotLoaded(collection.to_owned()))?;
        let mut results = index.search(&request.vector, request.top_k, request.ef_search)?;
        for result in &mut results {
            if let Some(record) = self.store.get_record(collection, &result.id)? {
                result.metadata = record.metadata;
            }
        }
        Ok(results)
    }

    fn read_indexes(&self) -> Result<RwLockReadGuard<'_, BTreeMap<String, HnswIndex>>, QueryError> {
        self.indexes.read().map_err(|_| QueryError::LockPoisoned)
    }

    fn write_indexes(
        &self,
    ) -> Result<RwLockWriteGuard<'_, BTreeMap<String, HnswIndex>>, QueryError> {
        self.indexes.write().map_err(|_| QueryError::LockPoisoned)
    }
}

fn build_index(store: &RocksStore, config: &CollectionConfig) -> Result<HnswIndex, QueryError> {
    let mut index = HnswIndex::new(config.dimensions, config.metric, config.hnsw.clone())?;
    for record in all_records(store, &config.name)? {
        index.insert(record.id, record.values)?;
    }
    Ok(index)
}

fn all_records(store: &RocksStore, collection: &str) -> Result<Vec<VectorRecord>, StorageError> {
    let mut output = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let page = store.list_records(collection, after.as_deref(), 1000)?;
        let count = page.len();
        after = page.last().map(|record| record.id.clone());
        output.extend(page);
        if count < 1000 {
            return Ok(output);
        }
    }
}

fn metadata_value<'a>(metadata: &'a Map<String, Value>, field: &str) -> Option<&'a Value> {
    let mut segments = field.split('.');
    let first = segments.next()?;
    let mut value = metadata.get(first)?;
    for segment in segments {
        value = value.as_object()?.get(segment)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use axion_core::DistanceMetric;
    use axion_index::HnswConfig;
    use tempfile::TempDir;

    use super::*;

    fn collection() -> CollectionConfig {
        CollectionConfig {
            name: "documents".to_owned(),
            dimensions: 2,
            metric: DistanceMetric::Euclidean,
            hnsw: HnswConfig::default(),
        }
    }

    fn record(id: &str, vector: [f32; 2], category: &str, active: bool) -> VectorRecord {
        let metadata = serde_json::from_value(serde_json::json!({
            "category": category,
            "details": { "active": active }
        }))
        .unwrap();
        VectorRecord {
            id: id.to_owned(),
            values: DenseVector::new(vector.to_vec()).unwrap(),
            metadata,
        }
    }

    #[test]
    fn lifecycle_search_filter_update_delete_and_recovery() {
        let directory = TempDir::new().unwrap();
        {
            let engine = CollectionEngine::open(directory.path()).unwrap();
            engine.create_collection(collection()).unwrap();
            engine
                .upsert(
                    "documents",
                    &[
                        record("research", [0.0, 0.0], "research", true),
                        record("archived", [0.1, 0.1], "research", false),
                        record("product", [0.2, 0.2], "product", true),
                    ],
                )
                .unwrap();

            let filter = FilterExpression::And {
                filters: vec![
                    FilterExpression::Eq {
                        field: "category".to_owned(),
                        value: Value::String("research".to_owned()),
                    },
                    FilterExpression::Not {
                        filter: Box::new(FilterExpression::Eq {
                            field: "details.active".to_owned(),
                            value: Value::Bool(false),
                        }),
                    },
                ],
            };
            let results = engine
                .search(
                    "documents",
                    &SearchRequest {
                        vector: DenseVector::new(vec![0.0, 0.0]).unwrap(),
                        top_k: 10,
                        mode: SearchMode::Approximate,
                        filter: Some(filter),
                        ef_search: None,
                    },
                )
                .unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "research");

            engine
                .upsert(
                    "documents",
                    &[record("research", [9.0, 9.0], "research", true)],
                )
                .unwrap();
            let approximate = engine
                .search(
                    "documents",
                    &SearchRequest {
                        vector: DenseVector::new(vec![0.0, 0.0]).unwrap(),
                        top_k: 2,
                        mode: SearchMode::Approximate,
                        filter: None,
                        ef_search: Some(64),
                    },
                )
                .unwrap();
            assert_eq!(approximate[0].id, "archived");
            assert!(engine.delete_record("documents", "archived").unwrap());
        }

        let recovered = CollectionEngine::open(directory.path()).unwrap();
        assert_eq!(
            recovered.get_collection("documents").unwrap().vector_count,
            2
        );
        let results = recovered
            .search(
                "documents",
                &SearchRequest {
                    vector: DenseVector::new(vec![0.2, 0.2]).unwrap(),
                    top_k: 1,
                    mode: SearchMode::Approximate,
                    filter: None,
                    ef_search: None,
                },
            )
            .unwrap();
        assert_eq!(results[0].id, "product");
    }

    #[test]
    fn rejects_dimension_mismatch_without_persisting() {
        let directory = TempDir::new().unwrap();
        let engine = CollectionEngine::open(directory.path()).unwrap();
        engine.create_collection(collection()).unwrap();
        let bad = VectorRecord {
            id: "bad".to_owned(),
            values: DenseVector::new(vec![1.0]).unwrap(),
            metadata: Map::new(),
        };
        assert!(matches!(
            engine.upsert("documents", &[bad]),
            Err(QueryError::Core(CoreError::DimensionMismatch { .. }))
        ));
        assert!(engine.get_record("documents", "bad").unwrap().is_none());
    }
}
