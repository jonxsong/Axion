//! gRPC transport for Axion's durable collection engine.

#![allow(clippy::result_large_err)] // `tonic::Status` is the generated API contract.

use std::time::Instant;

use axion_core::{DenseVector, DistanceMetric, VectorRecord};
use axion_index::HnswConfig;
use axion_query::{
    CollectionEngine, CollectionSummary, FilterExpression, QueryError, SearchMode,
    SearchRequest as EngineSearchRequest,
};
use axion_storage::{CollectionConfig, StorageError};
use serde_json::Map;
use tokio::task::JoinError;
use tonic::{Request, Response, Status};

#[allow(clippy::all, clippy::pedantic)]
pub mod pb {
    tonic::include_proto!("axion.v1");
}

use pb::axion_server::Axion;

#[derive(Clone)]
pub struct GrpcService {
    engine: CollectionEngine,
}

impl GrpcService {
    #[must_use]
    pub fn new(engine: CollectionEngine) -> Self {
        Self { engine }
    }
}

async fn engine_call<T>(
    operation: impl FnOnce() -> Result<T, QueryError> + Send + 'static,
) -> Result<T, Status>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| map_join_error(&error))?
        .map_err(|error| map_query_error(&error))
}

fn map_join_error(error: &JoinError) -> Status {
    tracing::error!(error = %error, "gRPC blocking engine task failed");
    Status::internal("internal task failure")
}

fn map_query_error(error: &QueryError) -> Status {
    match error {
        QueryError::Storage(StorageError::CollectionNotFound(_))
        | QueryError::CollectionNotLoaded(_) => Status::not_found(error.to_string()),
        QueryError::Storage(StorageError::CollectionExists(_)) => {
            Status::already_exists(error.to_string())
        }
        QueryError::Core(_)
        | QueryError::Index(_)
        | QueryError::Storage(
            StorageError::InvalidCollectionName
            | StorageError::InvalidRecordId
            | StorageError::InvalidDimensions,
        )
        | QueryError::InvalidListLimit => Status::invalid_argument(error.to_string()),
        _ => {
            tracing::error!(error = %error, "gRPC operation failed internally");
            Status::internal("internal storage error")
        }
    }
}

fn metric_from_proto(metric: i32) -> DistanceMetric {
    match pb::DistanceMetric::try_from(metric).unwrap_or(pb::DistanceMetric::Unspecified) {
        pb::DistanceMetric::Unspecified | pb::DistanceMetric::Cosine => DistanceMetric::Cosine,
        pb::DistanceMetric::Euclidean => DistanceMetric::Euclidean,
        pb::DistanceMetric::Dot => DistanceMetric::Dot,
    }
}

fn metric_to_proto(metric: DistanceMetric) -> i32 {
    match metric {
        DistanceMetric::Cosine => pb::DistanceMetric::Cosine.into(),
        DistanceMetric::Euclidean => pb::DistanceMetric::Euclidean.into(),
        DistanceMetric::Dot => pb::DistanceMetric::Dot.into(),
    }
}

fn hnsw_from_proto(config: Option<pb::HnswConfig>) -> Result<HnswConfig, Status> {
    let Some(config) = config else {
        return Ok(HnswConfig::default());
    };
    let parsed = HnswConfig {
        m: usize::try_from(config.m).map_err(|_| Status::invalid_argument("m is too large"))?,
        ef_construction: usize::try_from(config.ef_construction)
            .map_err(|_| Status::invalid_argument("ef_construction is too large"))?,
        ef_search: usize::try_from(config.ef_search)
            .map_err(|_| Status::invalid_argument("ef_search is too large"))?,
        random_seed: config.random_seed,
    };
    parsed
        .validate()
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    Ok(parsed)
}

fn hnsw_to_proto(config: &HnswConfig) -> pb::HnswConfig {
    pb::HnswConfig {
        m: u32::try_from(config.m).unwrap_or(u32::MAX),
        ef_construction: u32::try_from(config.ef_construction).unwrap_or(u32::MAX),
        ef_search: u32::try_from(config.ef_search).unwrap_or(u32::MAX),
        random_seed: config.random_seed,
    }
}

fn collection_to_proto(summary: CollectionSummary) -> pb::Collection {
    pb::Collection {
        name: summary.config.name,
        dimensions: u32::try_from(summary.config.dimensions).unwrap_or(u32::MAX),
        metric: metric_to_proto(summary.config.metric),
        hnsw: Some(hnsw_to_proto(&summary.config.hnsw)),
        vector_count: u64::try_from(summary.vector_count).unwrap_or(u64::MAX),
    }
}

fn vector_from_proto(vector: pb::Vector) -> Result<VectorRecord, Status> {
    let metadata = if vector.metadata_json.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(&vector.metadata_json)
            .map_err(|error| Status::invalid_argument(format!("invalid metadata_json: {error}")))?
    };
    Ok(VectorRecord {
        id: vector.id,
        values: DenseVector::new(vector.values)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
        metadata,
    })
}

fn vector_to_proto(vector: VectorRecord) -> pb::Vector {
    pb::Vector {
        id: vector.id,
        values: vector.values.as_slice().to_vec(),
        metadata_json: serde_json::to_string(&vector.metadata)
            .expect("JSON map serialization cannot fail"),
    }
}

#[tonic::async_trait]
impl Axion for GrpcService {
    async fn create_collection(
        &self,
        request: Request<pb::CreateCollectionRequest>,
    ) -> Result<Response<pb::Collection>, Status> {
        let request = request.into_inner();
        let config = CollectionConfig {
            name: request.name,
            dimensions: usize::try_from(request.dimensions)
                .map_err(|_| Status::invalid_argument("dimensions is too large"))?,
            metric: metric_from_proto(request.metric),
            hnsw: hnsw_from_proto(request.hnsw)?,
        };
        let engine = self.engine.clone();
        let name = config.name.clone();
        engine_call(move || engine.create_collection(config)).await?;
        let engine = self.engine.clone();
        let summary = engine_call(move || engine.get_collection(&name)).await?;
        Ok(Response::new(collection_to_proto(summary)))
    }

    async fn list_collections(
        &self,
        _request: Request<pb::Empty>,
    ) -> Result<Response<pb::ListCollectionsResponse>, Status> {
        let engine = self.engine.clone();
        let collections = engine_call(move || engine.list_collections())
            .await?
            .into_iter()
            .map(collection_to_proto)
            .collect();
        Ok(Response::new(pb::ListCollectionsResponse { collections }))
    }

    async fn get_collection(
        &self,
        request: Request<pb::GetCollectionRequest>,
    ) -> Result<Response<pb::Collection>, Status> {
        let name = request.into_inner().name;
        let engine = self.engine.clone();
        let collection = engine_call(move || engine.get_collection(&name)).await?;
        Ok(Response::new(collection_to_proto(collection)))
    }

    async fn delete_collection(
        &self,
        request: Request<pb::DeleteCollectionRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        let name = request.into_inner().name;
        let engine = self.engine.clone();
        engine_call(move || engine.delete_collection(&name)).await?;
        Ok(Response::new(pb::Empty {}))
    }

    async fn upsert(
        &self,
        request: Request<pb::UpsertRequest>,
    ) -> Result<Response<pb::UpsertResponse>, Status> {
        let request = request.into_inner();
        if request.vectors.is_empty() || request.vectors.len() > 10_000 {
            return Err(Status::invalid_argument(
                "batch must contain between 1 and 10000 vectors",
            ));
        }
        let records = request
            .vectors
            .into_iter()
            .map(vector_from_proto)
            .collect::<Result<Vec<_>, _>>()?;
        let count = u64::try_from(records.len()).unwrap_or(u64::MAX);
        let collection = request.collection;
        let engine = self.engine.clone();
        engine_call(move || engine.upsert(&collection, &records)).await?;
        Ok(Response::new(pb::UpsertResponse { upserted: count }))
    }

    async fn list_vectors(
        &self,
        request: Request<pb::ListVectorsRequest>,
    ) -> Result<Response<pb::ListVectorsResponse>, Status> {
        let request = request.into_inner();
        let collection = request.collection;
        let after = (!request.after.is_empty()).then_some(request.after);
        let limit = if request.limit == 0 {
            100
        } else {
            usize::try_from(request.limit)
                .map_err(|_| Status::invalid_argument("limit is too large"))?
        };
        let engine = self.engine.clone();
        let vectors =
            engine_call(move || engine.list_records(&collection, after.as_deref(), limit))
                .await?
                .into_iter()
                .map(vector_to_proto)
                .collect();
        Ok(Response::new(pb::ListVectorsResponse { vectors }))
    }

    async fn get_vector(
        &self,
        request: Request<pb::GetVectorRequest>,
    ) -> Result<Response<pb::Vector>, Status> {
        let request = request.into_inner();
        let engine = self.engine.clone();
        let record = engine_call(move || engine.get_record(&request.collection, &request.id))
            .await?
            .ok_or_else(|| Status::not_found("vector not found"))?;
        Ok(Response::new(vector_to_proto(record)))
    }

    async fn delete_vector(
        &self,
        request: Request<pb::DeleteVectorRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        let request = request.into_inner();
        let engine = self.engine.clone();
        if !engine_call(move || engine.delete_record(&request.collection, &request.id)).await? {
            return Err(Status::not_found("vector not found"));
        }
        Ok(Response::new(pb::Empty {}))
    }

    async fn search(
        &self,
        request: Request<pb::SearchRequest>,
    ) -> Result<Response<pb::SearchResponse>, Status> {
        let request = request.into_inner();
        let mode = match pb::SearchMode::try_from(request.mode)
            .unwrap_or(pb::SearchMode::Unspecified)
        {
            pb::SearchMode::Exact => SearchMode::Exact,
            pb::SearchMode::Unspecified | pb::SearchMode::Approximate => SearchMode::Approximate,
        };
        let filter = if request.filter_json.trim().is_empty() {
            None
        } else {
            Some(
                serde_json::from_str::<FilterExpression>(&request.filter_json).map_err(
                    |error| Status::invalid_argument(format!("invalid filter_json: {error}")),
                )?,
            )
        };
        let search = EngineSearchRequest {
            vector: DenseVector::new(request.vector)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            top_k: usize::try_from(request.top_k)
                .map_err(|_| Status::invalid_argument("top_k is too large"))?,
            mode,
            filter,
            ef_search: (request.ef_search != 0)
                .then(|| usize::try_from(request.ef_search).unwrap_or(usize::MAX)),
        };
        let collection = request.collection;
        let engine = self.engine.clone();
        let started = Instant::now();
        let results = engine_call(move || engine.search(&collection, &search)).await?;
        Ok(Response::new(pb::SearchResponse {
            results: results
                .into_iter()
                .map(|result| pb::SearchResult {
                    id: result.id,
                    score: result.score,
                    metadata_json: serde_json::to_string(&result.metadata)
                        .expect("JSON map serialization cannot fail"),
                })
                .collect(),
            took_micros: u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        }))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::TempDir;
    use tokio::sync::oneshot;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::Server;

    use super::*;

    #[tokio::test]
    async fn create_upsert_search_and_metadata_round_trip() {
        let directory = TempDir::new().unwrap();
        let engine = CollectionEngine::open(directory.path()).unwrap();
        let service = GrpcService::new(engine);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let incoming = TcpListenerStream::new(listener);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            Server::builder()
                .add_service(pb::axion_server::AxionServer::new(service))
                .serve_with_incoming_shutdown(incoming, async {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });
        let mut client = pb::axion_client::AxionClient::connect(format!("http://{address}"))
            .await
            .unwrap();

        client
            .create_collection(pb::CreateCollectionRequest {
                name: "documents".to_owned(),
                dimensions: 2,
                metric: pb::DistanceMetric::Cosine.into(),
                hnsw: None,
            })
            .await
            .unwrap();
        client
            .upsert(pb::UpsertRequest {
                collection: "documents".to_owned(),
                vectors: vec![pb::Vector {
                    id: "doc-1".to_owned(),
                    values: vec![1.0, 0.0],
                    metadata_json: json!({"category":"research"}).to_string(),
                }],
            })
            .await
            .unwrap();
        let response = client
            .search(pb::SearchRequest {
                collection: "documents".to_owned(),
                vector: vec![1.0, 0.0],
                top_k: 1,
                mode: pb::SearchMode::Approximate.into(),
                ef_search: 64,
                filter_json: String::new(),
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(response.results[0].id, "doc-1");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&response.results[0].metadata_json).unwrap(),
            json!({"category":"research"})
        );
        shutdown_tx.send(()).unwrap();
        server.await.unwrap();
    }
}
