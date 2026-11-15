use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use axion_api::{ApiConfig, router};
use axion_grpc::GrpcService;
use axion_grpc::pb::axion_server::AxionServer;
use axion_query::CollectionEngine;
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("axion=info")),
        )
        .json()
        .init();

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

    let engine = CollectionEngine::open(data_dir)?;
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
    tokio::try_join!(rest, grpc)?;
    Ok(())
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
