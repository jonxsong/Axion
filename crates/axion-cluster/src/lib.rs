//! Deterministic write routing and fan-out search across independent Axion shards.
//!
//! This coordinator intentionally does not implement replication or consensus.
//! Each record has one owner selected by a stable hash of its ID. Searches query
//! every configured owner and merge their already-ranked top-k results.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axion_core::{SearchResult, VectorRecord};
use axion_query::{FilterExpression, SearchMode};
use axion_storage::CollectionConfig;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures::future::join_all;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
use thiserror::Error;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ClusterConfigError {
    #[error("AXION_SHARD_NODES must define at least two comma-separated name=URL shards")]
    TooFewNodes,
    #[error("invalid shard definition '{0}'; expected name=http://host:port")]
    InvalidNode(String),
    #[error("shard names must be unique")]
    DuplicateNode,
    #[error("minimum successful search shards must be between 1 and the shard count")]
    InvalidMinimum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShardNode {
    pub name: String,
    pub base_url: String,
}

#[derive(Debug, Clone)]
pub struct ClusterConfig {
    pub nodes: Vec<ShardNode>,
    pub min_search_shards: usize,
    pub coordinator_api_key: Option<String>,
    pub shard_api_key: Option<String>,
    pub request_timeout: Duration,
}

impl ClusterConfig {
    /// Parses and deterministically sorts `name=URL` shard entries.
    ///
    /// # Errors
    ///
    /// Returns a validation error for malformed, duplicate, or fewer than two nodes.
    pub fn from_csv(value: &str, min_search_shards: usize) -> Result<Self, ClusterConfigError> {
        let mut nodes = value
            .split(',')
            .filter(|entry| !entry.trim().is_empty())
            .map(parse_node)
            .collect::<Result<Vec<_>, _>>()?;
        if nodes.len() < 2 {
            return Err(ClusterConfigError::TooFewNodes);
        }
        nodes.sort_by(|left, right| left.name.cmp(&right.name));
        if nodes.windows(2).any(|pair| pair[0].name == pair[1].name) {
            return Err(ClusterConfigError::DuplicateNode);
        }
        if min_search_shards == 0 || min_search_shards > nodes.len() {
            return Err(ClusterConfigError::InvalidMinimum);
        }
        Ok(Self {
            nodes,
            min_search_shards,
            coordinator_api_key: None,
            shard_api_key: None,
            request_timeout: Duration::from_secs(5),
        })
    }
}

fn parse_node(entry: &str) -> Result<ShardNode, ClusterConfigError> {
    let entry = entry.trim();
    let (name, base_url) = entry
        .split_once('=')
        .ok_or_else(|| ClusterConfigError::InvalidNode(entry.to_owned()))?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ClusterConfigError::InvalidNode(entry.to_owned()));
    }
    let parsed = reqwest::Url::parse(base_url)
        .map_err(|_| ClusterConfigError::InvalidNode(entry.to_owned()))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ClusterConfigError::InvalidNode(entry.to_owned()));
    }
    Ok(ShardNode {
        name: name.to_owned(),
        base_url: base_url.trim_end_matches('/').to_owned(),
    })
}

#[derive(Clone)]
struct ClusterState {
    config: Arc<ClusterConfig>,
    client: Client,
}

impl ClusterState {
    fn new(config: ClusterConfig) -> Result<Self, reqwest::Error> {
        let client = Client::builder().timeout(config.request_timeout).build()?;
        Ok(Self {
            config: Arc::new(config),
            client,
        })
    }

    fn owner(&self, id: &str) -> &ShardNode {
        let shard_count = u64::try_from(self.config.nodes.len()).expect("shard count fits u64");
        let index = usize::try_from(stable_hash(id) % shard_count)
            .expect("modulo shard count always fits usize");
        &self.config.nodes[index]
    }

    fn request(
        &self,
        node: &ShardNode,
        method: reqwest::Method,
        path: &str,
    ) -> reqwest::RequestBuilder {
        let builder = self
            .client
            .request(method, format!("{}{path}", node.base_url));
        if let Some(key) = self.config.shard_api_key.as_ref() {
            builder.bearer_auth(key)
        } else {
            builder
        }
    }
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[derive(Debug, Deserialize, Serialize)]
struct BatchRequest {
    vectors: Vec<VectorRecord>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterSearchRequest {
    pub vector: Vec<f32>,
    pub top_k: usize,
    #[serde(default)]
    pub mode: SearchMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<FilterExpression>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ef_search: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct NodeSearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShardFailure {
    pub shard: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct ClusterSearchResponse {
    pub results: Vec<SearchResult>,
    pub took_ms: f64,
    pub successful_shards: usize,
    pub total_shards: usize,
    pub partial: bool,
    pub failures: Vec<ShardFailure>,
}

#[derive(Debug, Serialize)]
pub struct ShardHealth {
    pub shard: String,
    pub url: String,
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClusterHealth {
    pub status: &'static str,
    pub ready_shards: usize,
    pub total_shards: usize,
    pub shards: Vec<ShardHealth>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    failures: Vec<ShardFailure>,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
            failures: Vec::new(),
        }
    }

    fn unavailable(message: impl Into<String>, failures: Vec<ShardFailure>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "insufficient_shards",
            message: message.into(),
            failures,
        }
    }

    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "a valid coordinator bearer API key is required".to_owned(),
            failures: Vec::new(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": {
                    "code": self.code,
                    "message": self.message,
                    "failures": self.failures,
                }
            })),
        )
            .into_response()
    }
}

/// Builds the cluster coordinator HTTP API.
///
/// # Errors
///
/// Returns an HTTP-client construction error for invalid platform TLS setup.
pub fn router(config: ClusterConfig) -> Result<Router, reqwest::Error> {
    let state = ClusterState::new(config)?;
    let protected = Router::new()
        .route("/v1/cluster/collections", post(create_collection))
        .route("/v1/cluster/collections/{name}", delete(delete_collection))
        .route(
            "/v1/cluster/collections/{name}/vectors/batch",
            post(upsert_batch),
        )
        .route("/v1/cluster/collections/{name}/search", post(search))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));
    Ok(Router::new()
        .merge(protected)
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/v1/cluster/health", get(health))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state))
}

async fn create_collection(
    State(state): State<ClusterState>,
    Json(config): Json<CollectionConfig>,
) -> Result<impl IntoResponse, ApiError> {
    let path = "/v1/collections";
    let calls = state.config.nodes.iter().map(|node| async {
        let result = state
            .request(node, reqwest::Method::POST, path)
            .json(&config)
            .send()
            .await;
        mutation_outcome(node, result)
    });
    ensure_all_shards(join_all(calls).await)?;
    Ok((StatusCode::CREATED, Json(config)))
}

async fn delete_collection(
    State(state): State<ClusterState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let path = format!("/v1/collections/{name}");
    let calls = state.config.nodes.iter().map(|node| async {
        let result = state
            .request(node, reqwest::Method::DELETE, &path)
            .send()
            .await;
        mutation_outcome(node, result)
    });
    ensure_all_shards(join_all(calls).await)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upsert_batch(
    State(state): State<ClusterState>,
    Path(name): Path<String>,
    Json(batch): Json<BatchRequest>,
) -> Result<StatusCode, ApiError> {
    if batch.vectors.is_empty() || batch.vectors.len() > 10_000 {
        return Err(ApiError::bad_request(
            "batch must contain between 1 and 10000 vectors",
        ));
    }
    let mut partitions = BTreeMap::<String, Vec<VectorRecord>>::new();
    for record in batch.vectors {
        partitions
            .entry(state.owner(&record.id).name.clone())
            .or_default()
            .push(record);
    }
    let path = format!("/v1/collections/{name}/vectors/batch");
    let calls = state.config.nodes.iter().filter_map(|node| {
        let vectors = partitions.remove(&node.name)?;
        Some(async {
            let result = state
                .request(node, reqwest::Method::POST, &path)
                .json(&BatchRequest { vectors })
                .send()
                .await;
            mutation_outcome(node, result)
        })
    });
    ensure_all_shards(join_all(calls).await)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn search(
    State(state): State<ClusterState>,
    Path(name): Path<String>,
    Json(request): Json<ClusterSearchRequest>,
) -> Result<Json<ClusterSearchResponse>, ApiError> {
    if request.top_k == 0 || request.vector.is_empty() {
        return Err(ApiError::bad_request(
            "vector must be non-empty and top_k must be greater than zero",
        ));
    }
    let started = Instant::now();
    let path = format!("/v1/collections/{name}/search");
    let calls = state.config.nodes.iter().map(|node| async {
        let result = state
            .request(node, reqwest::Method::POST, &path)
            .json(&request)
            .send()
            .await;
        match result {
            Ok(response) if response.status().is_success() => response
                .json::<NodeSearchResponse>()
                .await
                .map(|body| (node.name.clone(), body.results))
                .map_err(|error| failure(node, format!("invalid response: {error}"))),
            Ok(response) => Err(failure(node, format!("HTTP {}", response.status()))),
            Err(error) => Err(failure(node, transport_reason(&error))),
        }
    });
    let outcomes = join_all(calls).await;
    let mut failures = Vec::new();
    let mut successful_shards = 0;
    let mut by_id = BTreeMap::<String, SearchResult>::new();
    for outcome in outcomes {
        match outcome {
            Ok((_shard, results)) => {
                successful_shards += 1;
                for result in results {
                    by_id
                        .entry(result.id.clone())
                        .and_modify(|current| {
                            if result.score > current.score {
                                current.clone_from(&result);
                            }
                        })
                        .or_insert(result);
                }
            }
            Err(error) => failures.push(error),
        }
    }
    if successful_shards < state.config.min_search_shards {
        return Err(ApiError::unavailable(
            format!(
                "only {successful_shards} shards succeeded; {} required",
                state.config.min_search_shards
            ),
            failures,
        ));
    }
    let mut results = by_id.into_values().collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });
    results.truncate(request.top_k);
    Ok(Json(ClusterSearchResponse {
        results,
        took_ms: started.elapsed().as_secs_f64() * 1000.0,
        successful_shards,
        total_shards: state.config.nodes.len(),
        partial: !failures.is_empty(),
        failures,
    }))
}

fn mutation_outcome(
    node: &ShardNode,
    result: Result<reqwest::Response, reqwest::Error>,
) -> Result<(), ShardFailure> {
    match result {
        Ok(response) if response.status().is_success() => Ok(()),
        Ok(response) => Err(failure(node, format!("HTTP {}", response.status()))),
        Err(error) => Err(failure(node, transport_reason(&error))),
    }
}

fn ensure_all_shards(outcomes: Vec<Result<(), ShardFailure>>) -> Result<(), ApiError> {
    let failures = outcomes
        .into_iter()
        .filter_map(Result::err)
        .collect::<Vec<_>>();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(ApiError::unavailable(
            "one or more shard mutations failed; retry the idempotent operation",
            failures,
        ))
    }
}

fn failure(node: &ShardNode, reason: String) -> ShardFailure {
    ShardFailure {
        shard: node.name.clone(),
        reason,
    }
}

fn transport_reason(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "timeout".to_owned()
    } else if error.is_connect() {
        "connection failed".to_owned()
    } else {
        "transport failed".to_owned()
    }
}

async fn live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn ready(State(state): State<ClusterState>) -> Response {
    let report = health_report(&state).await;
    if report.ready_shards == report.total_shards {
        (StatusCode::OK, Json(report)).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(report)).into_response()
    }
}

async fn health(State(state): State<ClusterState>) -> Json<ClusterHealth> {
    Json(health_report(&state).await)
}

async fn health_report(state: &ClusterState) -> ClusterHealth {
    let calls = state.config.nodes.iter().map(|node| async {
        let result = state
            .request(node, reqwest::Method::GET, "/health/ready")
            .send()
            .await;
        match result {
            Ok(response) if response.status().is_success() => ShardHealth {
                shard: node.name.clone(),
                url: node.base_url.clone(),
                ready: true,
                error: None,
            },
            Ok(response) => ShardHealth {
                shard: node.name.clone(),
                url: node.base_url.clone(),
                ready: false,
                error: Some(format!("HTTP {}", response.status())),
            },
            Err(error) => ShardHealth {
                shard: node.name.clone(),
                url: node.base_url.clone(),
                ready: false,
                error: Some(transport_reason(&error)),
            },
        }
    });
    let shards = join_all(calls).await;
    let ready_shards = shards.iter().filter(|shard| shard.ready).count();
    ClusterHealth {
        status: if ready_shards == shards.len() {
            "ready"
        } else {
            "degraded"
        },
        ready_shards,
        total_shards: shards.len(),
        shards,
    }
}

async fn require_auth(State(state): State<ClusterState>, request: Request, next: Next) -> Response {
    let Some(expected) = &state.config.coordinator_api_key else {
        return next.run(request).await;
    };
    let supplied = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let valid = supplied.is_some_and(|supplied| {
        supplied.len() == expected.len()
            && bool::from(supplied.as_bytes().ct_eq(expected.as_bytes()))
    });
    if valid {
        next.run(request).await
    } else {
        ApiError::unauthorized().into_response()
    }
}

#[cfg(test)]
mod tests {
    use axion_core::DenseVector;
    use serde_json::Map;

    use super::*;

    #[test]
    fn configuration_is_sorted_and_validated() {
        let config =
            ClusterConfig::from_csv("z=http://127.0.0.1:9002,a=http://127.0.0.1:9001", 1).unwrap();
        assert_eq!(config.nodes[0].name, "a");
        assert_eq!(config.nodes[1].name, "z");
        assert!(matches!(
            ClusterConfig::from_csv("a=http://localhost:1", 1),
            Err(ClusterConfigError::TooFewNodes)
        ));
    }

    #[test]
    fn stable_assignment_is_independent_of_configuration_order() {
        let left = ClusterState::new(
            ClusterConfig::from_csv("b=http://b:8080,a=http://a:8080", 1).unwrap(),
        )
        .unwrap();
        let right = ClusterState::new(
            ClusterConfig::from_csv("a=http://a:8080,b=http://b:8080", 1).unwrap(),
        )
        .unwrap();
        for id in ["one", "two", "customer-42", "unicode-λ"] {
            assert_eq!(left.owner(id), right.owner(id));
        }
    }

    #[test]
    fn merge_order_is_score_then_id() {
        let mut results = [
            SearchResult {
                id: "b".to_owned(),
                score: 0.5,
                metadata: Map::new(),
            },
            SearchResult {
                id: "a".to_owned(),
                score: 0.5,
                metadata: Map::new(),
            },
            SearchResult {
                id: "c".to_owned(),
                score: 0.9,
                metadata: Map::new(),
            },
        ];
        results.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });
        assert_eq!(results.map(|result| result.id), ["c", "a", "b"]);

        let record = VectorRecord {
            id: "example".to_owned(),
            values: DenseVector::new(vec![1.0]).unwrap(),
            metadata: Map::new(),
        };
        assert!(stable_hash(&record.id) > 0);
    }
}
