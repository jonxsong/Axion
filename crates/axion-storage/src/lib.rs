//! Durable Axion state backed by `RocksDB`.
//!
//! Every mutating operation uses a `RocksDB` write batch with WAL syncing enabled.
//! Therefore, a successful return means the write-ahead log has been handed to
//! the operating system's durable sync primitive. HNSW is derived state: the
//! collection engine reconstructs it from these records after restart.

use std::path::Path;
use std::sync::Arc;

use axion_core::{DistanceMetric, VectorRecord};
use axion_index::HnswConfig;
use rocksdb::{DB, Direction, IteratorMode, Options, WriteBatch, WriteOptions};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const SCHEMA_KEY: &[u8] = b"m/schema-version";
const SCHEMA_VERSION: &[u8] = b"1";
const COLLECTION_KEY_PREFIX: u8 = b'c';
const RECORD_KEY_PREFIX: u8 = b'r';

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),
    #[error("storage serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("unsupported storage schema version: {0}")]
    UnsupportedSchema(String),
    #[error("collection '{0}' already exists")]
    CollectionExists(String),
    #[error("collection '{0}' does not exist")]
    CollectionNotFound(String),
    #[error("collection names must contain 1-128 ASCII letters, digits, '-' or '_'")]
    InvalidCollectionName,
    #[error("record IDs must contain 1-512 bytes and no NUL characters")]
    InvalidRecordId,
    #[error("collection dimensions must be greater than zero")]
    InvalidDimensions,
}

/// Persisted collection definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionConfig {
    pub name: String,
    pub dimensions: usize,
    pub metric: DistanceMetric,
    #[serde(default)]
    pub hnsw: HnswConfig,
}

impl CollectionConfig {
    /// Validates names and index configuration before durable creation.
    ///
    /// # Errors
    ///
    /// Returns a validation error for an invalid name, dimension, or HNSW
    /// configuration.
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.name.is_empty()
            || self.name.len() > 128
            || !self
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(StorageError::InvalidCollectionName);
        }
        if self.dimensions == 0 {
            return Err(StorageError::InvalidDimensions);
        }
        self.hnsw
            .validate()
            .map_err(|error| StorageError::UnsupportedSchema(error.to_string()))
    }
}

/// Thread-safe `RocksDB` handle. This is Axion's default storage implementation.
#[derive(Clone)]
pub struct RocksStore {
    db: Arc<DB>,
}

impl RocksStore {
    /// Opens or creates a `RocksDB` database and validates its format version.
    ///
    /// # Errors
    ///
    /// Returns a `RocksDB` I/O error or an unsupported-schema error.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let mut options = Options::default();
        options.create_if_missing(true);
        options.set_atomic_flush(true);
        options.set_keep_log_file_num(10);
        options.set_max_open_files(256);
        let db = Arc::new(DB::open(&options, path)?);
        match db.get(SCHEMA_KEY)? {
            None => {
                let mut write_options = WriteOptions::default();
                write_options.set_sync(true);
                db.put_opt(SCHEMA_KEY, SCHEMA_VERSION, &write_options)?;
            }
            Some(version) if version == SCHEMA_VERSION => {}
            Some(version) => {
                return Err(StorageError::UnsupportedSchema(
                    String::from_utf8_lossy(&version).into_owned(),
                ));
            }
        }
        Ok(Self { db })
    }

    /// Creates a collection atomically.
    ///
    /// # Errors
    ///
    /// Returns a validation, duplicate, serialization, or `RocksDB` error.
    pub fn create_collection(&self, config: &CollectionConfig) -> Result<(), StorageError> {
        config.validate()?;
        let key = collection_key(&config.name);
        if self.db.get(&key)?.is_some() {
            return Err(StorageError::CollectionExists(config.name.clone()));
        }
        self.sync_put(key, serde_json::to_vec(config)?)
    }

    /// Returns one collection definition.
    ///
    /// # Errors
    ///
    /// Returns not-found, decoding, or `RocksDB` errors.
    pub fn get_collection(&self, name: &str) -> Result<CollectionConfig, StorageError> {
        let value = self
            .db
            .get(collection_key(name))?
            .ok_or_else(|| StorageError::CollectionNotFound(name.to_owned()))?;
        Ok(serde_json::from_slice(&value)?)
    }

    /// Lists collection definitions in lexicographic name order.
    ///
    /// # Errors
    ///
    /// Returns decoding or `RocksDB` iterator errors.
    pub fn list_collections(&self) -> Result<Vec<CollectionConfig>, StorageError> {
        let mut output = Vec::new();
        for item in self.db.iterator(IteratorMode::From(
            &[COLLECTION_KEY_PREFIX],
            Direction::Forward,
        )) {
            let (key, value) = item?;
            if key.first() != Some(&COLLECTION_KEY_PREFIX) {
                break;
            }
            output.push(serde_json::from_slice(&value)?);
        }
        output.sort_by(|left: &CollectionConfig, right| left.name.cmp(&right.name));
        Ok(output)
    }

    /// Deletes a collection and all of its records in one synced batch.
    ///
    /// # Errors
    ///
    /// Returns not-found or `RocksDB` errors.
    pub fn delete_collection(&self, name: &str) -> Result<(), StorageError> {
        let collection_key = collection_key(name);
        if self.db.get(&collection_key)?.is_none() {
            return Err(StorageError::CollectionNotFound(name.to_owned()));
        }
        let mut batch = WriteBatch::default();
        batch.delete(collection_key);
        let prefix = record_prefix(name);
        for item in self
            .db
            .iterator(IteratorMode::From(&prefix, Direction::Forward))
        {
            let (key, _) = item?;
            if !key.starts_with(&prefix) {
                break;
            }
            batch.delete(key);
        }
        self.sync_batch(batch)
    }

    /// Atomically creates or replaces records in one collection.
    ///
    /// # Errors
    ///
    /// Returns not-found, invalid-ID, serialization, or `RocksDB` errors.
    pub fn upsert_records(
        &self,
        collection: &str,
        records: &[VectorRecord],
    ) -> Result<(), StorageError> {
        self.get_collection(collection)?;
        let mut batch = WriteBatch::default();
        for record in records {
            validate_record_id(&record.id)?;
            batch.put(
                record_key(collection, &record.id),
                serde_json::to_vec(record)?,
            );
        }
        self.sync_batch(batch)
    }

    /// Returns one record or `None`.
    ///
    /// # Errors
    ///
    /// Returns invalid-ID, decoding, or `RocksDB` errors.
    pub fn get_record(
        &self,
        collection: &str,
        id: &str,
    ) -> Result<Option<VectorRecord>, StorageError> {
        validate_record_id(id)?;
        self.db
            .get(record_key(collection, id))?
            .map(|value| serde_json::from_slice(&value).map_err(StorageError::from))
            .transpose()
    }

    /// Deletes a record durably. Returns whether it existed.
    ///
    /// # Errors
    ///
    /// Returns invalid-ID or `RocksDB` errors.
    pub fn delete_record(&self, collection: &str, id: &str) -> Result<bool, StorageError> {
        validate_record_id(id)?;
        let key = record_key(collection, id);
        if self.db.get(&key)?.is_none() {
            return Ok(false);
        }
        let mut batch = WriteBatch::default();
        batch.delete(key);
        self.sync_batch(batch)?;
        Ok(true)
    }

    /// Lists records in deterministic ID order, after an optional exclusive ID.
    ///
    /// # Errors
    ///
    /// Returns decoding, invalid-ID, or `RocksDB` errors.
    pub fn list_records(
        &self,
        collection: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<VectorRecord>, StorageError> {
        self.get_collection(collection)?;
        if let Some(after) = after {
            validate_record_id(after)?;
        }
        let prefix = record_prefix(collection);
        let start = after.map_or_else(|| prefix.clone(), |id| record_key(collection, id));
        let mut output = Vec::new();
        for item in self
            .db
            .iterator(IteratorMode::From(&start, Direction::Forward))
        {
            let (key, value) = item?;
            if !key.starts_with(&prefix) {
                break;
            }
            if after.is_some_and(|id| key.as_ref() == record_key(collection, id)) {
                continue;
            }
            output.push(serde_json::from_slice(&value)?);
            if output.len() == limit {
                break;
            }
        }
        Ok(output)
    }

    /// Forces `RocksDB`'s active WAL to stable storage.
    ///
    /// # Errors
    ///
    /// Returns a `RocksDB` I/O error.
    pub fn flush_wal(&self) -> Result<(), StorageError> {
        self.db.flush_wal(true)?;
        Ok(())
    }

    fn sync_put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), StorageError> {
        let mut batch = WriteBatch::default();
        batch.put(key, value);
        self.sync_batch(batch)
    }

    fn sync_batch(&self, batch: WriteBatch) -> Result<(), StorageError> {
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db.write_opt(batch, &options)?;
        Ok(())
    }
}

fn collection_key(name: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(5 + name.len());
    key.push(COLLECTION_KEY_PREFIX);
    key.extend_from_slice(&u32::try_from(name.len()).unwrap_or(u32::MAX).to_be_bytes());
    key.extend_from_slice(name.as_bytes());
    key
}

fn record_prefix(collection: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(5 + collection.len());
    key.push(RECORD_KEY_PREFIX);
    key.extend_from_slice(
        &u32::try_from(collection.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    key.extend_from_slice(collection.as_bytes());
    key
}

fn record_key(collection: &str, id: &str) -> Vec<u8> {
    let mut key = record_prefix(collection);
    key.extend_from_slice(id.as_bytes());
    key
}

fn validate_record_id(id: &str) -> Result<(), StorageError> {
    if id.is_empty() || id.len() > 512 || id.contains('\0') {
        return Err(StorageError::InvalidRecordId);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axion_core::DenseVector;
    use serde_json::{Map, Value};
    use tempfile::TempDir;

    use super::*;

    fn config(name: &str) -> CollectionConfig {
        CollectionConfig {
            name: name.to_owned(),
            dimensions: 3,
            metric: DistanceMetric::Cosine,
            hnsw: HnswConfig::default(),
        }
    }

    fn record(id: &str, value: f32) -> VectorRecord {
        let mut metadata = Map::new();
        metadata.insert("group".to_owned(), Value::String("test".to_owned()));
        VectorRecord {
            id: id.to_owned(),
            values: DenseVector::new(vec![value, 0.0, 1.0]).unwrap(),
            metadata,
        }
    }

    #[test]
    fn validates_collection_names() {
        let directory = TempDir::new().unwrap();
        let store = RocksStore::open(directory.path()).unwrap();
        let error = store.create_collection(&config("bad/name")).unwrap_err();
        assert!(matches!(error, StorageError::InvalidCollectionName));
    }

    #[test]
    fn collection_lifecycle_and_atomic_batch_upsert() {
        let directory = TempDir::new().unwrap();
        let store = RocksStore::open(directory.path()).unwrap();
        store.create_collection(&config("documents")).unwrap();
        store
            .upsert_records("documents", &[record("b", 2.0), record("a", 1.0)])
            .unwrap();

        assert_eq!(
            store
                .list_records("documents", None, 10)
                .unwrap()
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(
            store.list_records("documents", Some("a"), 10).unwrap()[0].id,
            "b"
        );
        assert!(store.delete_record("documents", "a").unwrap());
        assert!(!store.delete_record("documents", "a").unwrap());
        store.delete_collection("documents").unwrap();
        assert_eq!(store.list_collections().unwrap(), []);
    }

    #[test]
    fn recovers_acknowledged_data_after_reopen() {
        let directory = TempDir::new().unwrap();
        {
            let store = RocksStore::open(directory.path()).unwrap();
            store.create_collection(&config("persistent")).unwrap();
            store
                .upsert_records("persistent", &[record("one", 1.0), record("two", 2.0)])
                .unwrap();
            store.flush_wal().unwrap();
        }
        let reopened = RocksStore::open(directory.path()).unwrap();
        assert_eq!(reopened.get_collection("persistent").unwrap().dimensions, 3);
        let recovered = reopened
            .list_records("persistent", None, 10)
            .unwrap()
            .into_iter()
            .map(|record| (record.id, record.values.as_slice().to_vec()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered["one"], [1.0, 0.0, 1.0]);
    }
}
