//! Versioned REST API for the Axion collection engine.

use std::sync::Arc;
use std::time::Instant;

use axion_core::{DenseVector, DistanceMetric, VectorRecord};
use axion_index::HnswConfig;
use axion_query::{
    CollectionEngine, FilterExpression, QueryError, SearchMode,
    SearchRequest as EngineSearchRequest,
};
use axion_storage::{CollectionConfig, StorageError};
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Path, Query, Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use subtle::ConstantTimeEq;
use tokio::task::JoinError;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

const OPENAPI: &str = include_str!("../../../docs/api/openapi.yaml");
const DOCS_HTML: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Axion API</title><link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css"></head><body><div id="swagger-ui"></div><script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script><script>SwaggerUIBundle({url:'/openapi.yaml',dom_id:'#swagger-ui',deepLinking:true});</script></body></html>"#;

/// Runtime REST configuration.
#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub api_key: Option<String>,
    pub max_body_bytes: usize,
    pub allow_any_origin: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            max_body_bytes: 8 * 1024 * 1024,
            allow_any_origin: false,
        }
    }
}

#[derive(Clone)]
struct AppState {
    engine: CollectionEngine,
    api_key: Option<Arc<str>>,
    metrics: ApiMetrics,
}

#[derive(Clone)]
struct ApiMetrics {
    registry: Arc<Registry>,
    requests: IntCounterVec,
    request_duration: HistogramVec,
    searches: IntCounterVec,
    vectors_written: IntCounterVec,
}

impl ApiMetrics {
    fn new() -> Self {
        let registry = Registry::new();
        let requests = IntCounterVec::new(
            Opts::new(
                "axion_http_requests_total",
                "HTTP responses by method and status",
            ),
            &["method", "status"],
        )
        .expect("static metric definition is valid");
        let request_duration = HistogramVec::new(
            HistogramOpts::new(
                "axion_http_request_duration_seconds",
                "HTTP request duration in seconds",
            ),
            &["method"],
        )
        .expect("static metric definition is valid");
        let searches = IntCounterVec::new(
            Opts::new("axion_searches_total", "Vector searches by requested mode"),
            &["mode"],
        )
        .expect("static metric definition is valid");
        let vectors_written = IntCounterVec::new(
            Opts::new(
                "axion_vectors_written_total",
                "Vectors accepted by batch type",
            ),
            &["operation"],
        )
        .expect("static metric definition is valid");
        registry
            .register(Box::new(requests.clone()))
            .expect("metric names are unique");
        registry
            .register(Box::new(request_duration.clone()))
            .expect("metric names are unique");
        registry
            .register(Box::new(searches.clone()))
            .expect("metric names are unique");
        registry
            .register(Box::new(vectors_written.clone()))
            .expect("metric names are unique");
        Self {
            registry: Arc::new(registry),
            requests,
            request_duration,
            searches,
            vectors_written,
        }
    }
}

/// Constructs the complete HTTP application.
pub fn router(engine: CollectionEngine, config: ApiConfig) -> Router {
    let state = AppState {
        engine,
        api_key: config.api_key.map(Arc::from),
        metrics: ApiMetrics::new(),
    };

    let protected = Router::new()
        .route(
            "/v1/collections",
            post(create_collection).get(list_collections),
        )
        .route(
            "/v1/collections/{name}",
            get(get_collection).delete(delete_collection),
        )
        .route(
            "/v1/collections/{name}/vectors",
            post(upsert_one).get(list_vectors),
        )
        .route("/v1/collections/{name}/vectors/batch", post(upsert_batch))
        .route(
            "/v1/collections/{name}/vectors/{id}",
            get(get_vector).delete(delete_vector),
        )
        .route("/v1/collections/{name}/search", post(search))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let cors = if config.allow_any_origin {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_headers([AUTHORIZATION, CONTENT_TYPE])
            .allow_methods(Any)
    } else {
        CorsLayer::new()
    };

    Router::new()
        .merge(protected)
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/metrics", get(metrics))
        .route("/openapi.yaml", get(openapi))
        .route("/docs", get(docs))
        .fallback(not_found)
        .layer(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            record_http_metrics,
        ))
        .with_state(state)
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
        }
    }

    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "a valid bearer API key is required".to_owned(),
        }
    }

    fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "an internal error occurred".to_owned(),
        }
    }
}

impl From<QueryError> for ApiError {
    fn from(error: QueryError) -> Self {
        match error {
            QueryError::Storage(StorageError::CollectionNotFound(_))
            | QueryError::CollectionNotLoaded(_) => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: error.to_string(),
            },
            QueryError::Storage(StorageError::CollectionExists(_)) => Self {
                status: StatusCode::CONFLICT,
                code: "already_exists",
                message: error.to_string(),
            },
            QueryError::Core(_)
            | QueryError::Index(_)
            | QueryError::Storage(
                StorageError::InvalidCollectionName
                | StorageError::InvalidRecordId
                | StorageError::InvalidDimensions,
            )
            | QueryError::InvalidListLimit => Self::bad_request(error.to_string()),
            _ => {
                tracing::error!(error = %error, "request failed internally");
                Self::internal()
            }
        }
    }
}

impl From<JoinError> for ApiError {
    fn from(error: JoinError) -> Self {
        tracing::error!(error = %error, "blocking engine task failed");
        Self::internal()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": {
                    "code": self.code,
                    "message": self.message
                }
            })),
        )
            .into_response()
    }
}

struct ApiJson<T>(T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|error: JsonRejection| ApiError::bad_request(error.body_text()))
    }
}

#[derive(Debug, Deserialize)]
struct CreateCollectionRequest {
    name: String,
    dimensions: usize,
    #[serde(default)]
    metric: DistanceMetric,
    #[serde(default)]
    hnsw: HnswConfig,
}

#[derive(Debug, Deserialize)]
struct VectorInput {
    id: String,
    values: Vec<f32>,
    #[serde(default)]
    metadata: Map<String, Value>,
}

impl TryFrom<VectorInput> for VectorRecord {
    type Error = ApiError;

    fn try_from(input: VectorInput) -> Result<Self, Self::Error> {
        Ok(Self {
            id: input.id,
            values: DenseVector::new(input.values)
                .map_err(|error| ApiError::bad_request(error.to_string()))?,
            metadata: input.metadata,
        })
    }
}

#[derive(Debug, Deserialize)]
struct BatchInput {
    vectors: Vec<VectorInput>,
}

#[derive(Debug, Deserialize)]
struct SearchInput {
    vector: Vec<f32>,
    top_k: usize,
    #[serde(default)]
    mode: SearchMode,
    #[serde(default)]
    filter: Option<FilterExpression>,
    #[serde(default)]
    ef_search: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    after: Option<String>,
    #[serde(default = "default_list_limit")]
    limit: usize,
}

const fn default_list_limit() -> usize {
    100
}

#[derive(Serialize)]
struct SearchResponse {
    results: Vec<axion_core::SearchResult>,
    took_ms: f64,
}

async fn engine_call<T>(
    operation: impl FnOnce() -> Result<T, QueryError> + Send + 'static,
) -> Result<T, ApiError>
where
    T: Send + 'static,
{
    Ok(tokio::task::spawn_blocking(operation).await??)
}

async fn create_collection(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<CreateCollectionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = CollectionConfig {
        name: request.name,
        dimensions: request.dimensions,
        metric: request.metric,
        hnsw: request.hnsw,
    };
    let engine = state.engine.clone();
    let response = config.clone();
    engine_call(move || engine.create_collection(config)).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn list_collections(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let engine = state.engine.clone();
    Ok(Json(engine_call(move || engine.list_collections()).await?))
}

async fn get_collection(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let engine = state.engine.clone();
    Ok(Json(
        engine_call(move || engine.get_collection(&name)).await?,
    ))
}

async fn delete_collection(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let engine = state.engine.clone();
    engine_call(move || engine.delete_collection(&name)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upsert_one(
    State(state): State<AppState>,
    Path(name): Path<String>,
    ApiJson(input): ApiJson<VectorInput>,
) -> Result<impl IntoResponse, ApiError> {
    let record = VectorRecord::try_from(input)?;
    let engine = state.engine.clone();
    engine_call(move || engine.upsert(&name, &[record])).await?;
    state
        .metrics
        .vectors_written
        .with_label_values(&["single"])
        .inc();
    Ok(StatusCode::NO_CONTENT)
}

async fn upsert_batch(
    State(state): State<AppState>,
    Path(name): Path<String>,
    ApiJson(input): ApiJson<BatchInput>,
) -> Result<impl IntoResponse, ApiError> {
    if input.vectors.is_empty() || input.vectors.len() > 10_000 {
        return Err(ApiError::bad_request(
            "batch must contain between 1 and 10000 vectors",
        ));
    }
    let records = input
        .vectors
        .into_iter()
        .map(VectorRecord::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let count = records.len();
    let engine = state.engine.clone();
    engine_call(move || engine.upsert(&name, &records)).await?;
    state
        .metrics
        .vectors_written
        .with_label_values(&["batch"])
        .inc_by(u64::try_from(count).unwrap_or(u64::MAX));
    Ok(StatusCode::NO_CONTENT)
}

async fn get_vector(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let engine = state.engine.clone();
    match engine_call(move || engine.get_record(&name, &id)).await? {
        Some(record) => Ok(Json(record)),
        None => Err(ApiError {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: "vector not found".to_owned(),
        }),
    }
}

async fn list_vectors(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let engine = state.engine.clone();
    Ok(Json(
        engine_call(move || engine.list_records(&name, query.after.as_deref(), query.limit))
            .await?,
    ))
}

async fn delete_vector(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let engine = state.engine.clone();
    if engine_call(move || engine.delete_record(&name, &id)).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: "vector not found".to_owned(),
        })
    }
}

async fn search(
    State(state): State<AppState>,
    Path(name): Path<String>,
    ApiJson(input): ApiJson<SearchInput>,
) -> Result<impl IntoResponse, ApiError> {
    let mode = input.mode;
    let request = EngineSearchRequest {
        vector: DenseVector::new(input.vector)
            .map_err(|error| ApiError::bad_request(error.to_string()))?,
        top_k: input.top_k,
        mode,
        filter: input.filter,
        ef_search: input.ef_search,
    };
    let started = Instant::now();
    let engine = state.engine.clone();
    let results = engine_call(move || engine.search(&name, &request)).await?;
    state
        .metrics
        .searches
        .with_label_values(&[match mode {
            SearchMode::Exact => "exact",
            SearchMode::Approximate => "approximate",
        }])
        .inc();
    Ok(Json(SearchResponse {
        results,
        took_ms: started.elapsed().as_secs_f64() * 1000.0,
    }))
}

async fn live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn ready(State(state): State<AppState>) -> Response {
    let engine = state.engine.clone();
    match engine_call(move || engine.list_collections()).await {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ready"}))).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn metrics(State(state): State<AppState>) -> Response {
    let families = state.metrics.registry.gather();
    let mut buffer = Vec::new();
    if TextEncoder::new().encode(&families, &mut buffer).is_err() {
        return ApiError::internal().into_response();
    }
    (
        [(
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        buffer,
    )
        .into_response()
}

async fn openapi() -> impl IntoResponse {
    ([(CONTENT_TYPE, "application/yaml")], OPENAPI)
}

async fn docs() -> Html<&'static str> {
    Html(DOCS_HTML)
}

async fn not_found() -> ApiError {
    ApiError {
        status: StatusCode::NOT_FOUND,
        code: "not_found",
        message: "route not found".to_owned(),
    }
}

async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let Some(expected) = &state.api_key else {
        return next.run(request).await;
    };
    let supplied = request
        .headers()
        .get(AUTHORIZATION)
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

async fn record_http_metrics(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_owned();
    let started = Instant::now();
    let response = next.run(request).await;
    state
        .metrics
        .requests
        .with_label_values(&[&method, response.status().as_str()])
        .inc();
    state
        .metrics
        .request_duration
        .with_label_values(&[&method])
        .observe(started.elapsed().as_secs_f64());
    response
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Method, Request};
    use http_body_util::BodyExt;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use super::*;

    async fn body_json(response: Response) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn json_request(method: Method, uri: &str, value: &Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&value).unwrap()))
            .unwrap()
    }

    #[tokio::test]
    async fn real_engine_crud_search_and_validation_flow() {
        let directory = TempDir::new().unwrap();
        let engine = CollectionEngine::open(directory.path()).unwrap();
        let app = router(engine, ApiConfig::default());

        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/v1/collections",
                &json!({"name":"documents","dimensions":2,"metric":"cosine"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/v1/collections/documents/vectors/batch",
                &json!({"vectors":[
                    {"id":"a","values":[1.0,0.0],"metadata":{"category":"research"}},
                    {"id":"b","values":[0.0,1.0],"metadata":{"category":"product"}}
                ]}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/v1/collections/documents/search",
                &json!({
                    "vector":[1.0,0.0],
                    "top_k":5,
                    "mode":"approximate",
                    "filter":{"eq":{"field":"category","value":"research"}}
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["results"][0]["id"], "a");
        assert_eq!(body["results"].as_array().unwrap().len(), 1);

        let invalid = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/v1/collections/documents/vectors",
                &json!({"id":"bad","values":[null,0.0]}),
            ))
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body_json(invalid).await["error"]["code"], "invalid_request");

        let metrics = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(metrics.status(), StatusCode::OK);
        let body = metrics.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("axion_searches_total"));
    }

    #[tokio::test]
    async fn bearer_auth_protects_versioned_routes_but_not_health() {
        let directory = TempDir::new().unwrap();
        let engine = CollectionEngine::open(directory.path()).unwrap();
        let app = router(
            engine,
            ApiConfig {
                api_key: Some("secret-test-key".to_owned()),
                ..ApiConfig::default()
            },
        );
        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/collections")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let authorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/collections")
                    .header(AUTHORIZATION, "Bearer secret-test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);

        let health = app
            .oneshot(
                Request::builder()
                    .uri("/health/live")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
    }
}
