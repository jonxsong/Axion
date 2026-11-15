use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use axion_api::{ApiConfig, router};
use axion_grpc::GrpcService;
use axion_grpc::pb::axion_server::AxionServer;
use axion_query::{CollectionEngine, QueryError};
use axion_storage::StorageError;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tokio::sync::watch;
use tokio::time::Instant;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tracer_provider = init_tracing()?;

    let data_dir =
        env::var_os("AXION_DATA_DIR").map_or_else(|| PathBuf::from("./data"), PathBuf::from);
    let host = env::var("AXION_HOST")
        .ok()
        .and_then(|value| value.parse::<IpAddr>().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let port = env::var("AXION_HTTP_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let grpc_port = env::var("AXION_GRPC_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(50_051);
    let api_key = env::var("AXION_API_KEY")
        .ok()
        .filter(|value| !value.is_empty());
    let allow_any_origin =
        env::var("AXION_CORS_ALLOW_ANY").is_ok_and(|value| value.eq_ignore_ascii_case("true"));

    let storage_open_timeout = env::var("AXION_STORAGE_OPEN_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(Duration::from_secs(30), Duration::from_secs);
    let engine = open_engine_with_retry(&data_dir, storage_open_timeout).await?;
    let app = router(
        engine.clone(),
        ApiConfig {
            api_key,
            allow_any_origin,
            ..ApiConfig::default()
        },
    );
    let http_address = SocketAddr::new(host, port);
    let grpc_address = SocketAddr::new(host, grpc_port);
    let listener = tokio::net::TcpListener::bind(http_address).await?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    tracing::info!(address = %http_address, "Axion REST server listening");
    tracing::info!(address = %grpc_address, "Axion gRPC server listening");
    let rest_shutdown = shutdown_rx.clone();
    let rest = async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(wait_for_shutdown(rest_shutdown))
            .await
            .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
    };
    let grpc = async move {
        tonic::transport::Server::builder()
            .add_service(AxionServer::new(GrpcService::new(engine)))
            .serve_with_shutdown(grpc_address, wait_for_shutdown(shutdown_rx))
            .await
            .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
    };
    let serve_result = tokio::try_join!(rest, grpc);
    if let Some(provider) = tracer_provider {
        provider.shutdown()?;
    }
    serve_result?;
    Ok(())
}

fn init_tracing() -> Result<Option<SdkTracerProvider>, Box<dyn std::error::Error>> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("axion=info"));
    let provider = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|endpoint| !endpoint.is_empty())
        .map(|endpoint| {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint)
                .build()?;
            let provider = SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .build();
            global::set_tracer_provider(provider.clone());
            Ok::<_, opentelemetry_otlp::ExporterBuildError>(provider)
        })
        .transpose()?;
    let telemetry = provider.as_ref().map(|provider| {
        tracing_opentelemetry::layer().with_tracer(provider.tracer("axion-server"))
    });
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .with(telemetry)
        .init();
    Ok(provider)
}

async fn open_engine_with_retry(
    data_dir: &PathBuf,
    timeout: Duration,
) -> Result<CollectionEngine, QueryError> {
    let deadline = Instant::now() + timeout;
    loop {
        match CollectionEngine::open(data_dir) {
            Ok(engine) => return Ok(engine),
            Err(error) if is_transient_storage_lock(&error) && Instant::now() < deadline => {
                tracing::warn!(
                    error = %error,
                    retry_timeout_seconds = timeout.as_secs(),
                    "RocksDB is still locked by the previous process; retrying"
                );
                let remaining = deadline.saturating_duration_since(Instant::now());
                tokio::time::sleep(remaining.min(Duration::from_millis(500))).await;
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_transient_storage_lock(error: &QueryError) -> bool {
    matches!(
        error,
        QueryError::Storage(StorageError::RocksDb(error))
            if error.to_string().to_ascii_lowercase().contains("lock")
    )
}

async fn wait_for_shutdown(mut receiver: watch::Receiver<bool>) {
    if *receiver.borrow() {
        return;
    }
    while receiver.changed().await.is_ok() {
        if *receiver.borrow() {
            return;
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("graceful shutdown requested");
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn retries_a_transient_rocksdb_lock_and_opens_after_release() {
        let directory = tempdir().unwrap();
        let first = CollectionEngine::open(directory.path()).unwrap();
        let path = directory.path().to_path_buf();

        let result = open_engine_with_retry(&path, Duration::from_millis(10)).await;
        let Err(error) = result else {
            panic!("a second RocksDB owner must not open the same directory");
        };
        assert!(is_transient_storage_lock(&error));

        drop(first);
        open_engine_with_retry(&path, Duration::from_millis(10))
            .await
            .expect("the directory opens after its first owner releases the lock");
    }
}
