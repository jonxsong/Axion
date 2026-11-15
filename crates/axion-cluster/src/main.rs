use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use axion_cluster::{ClusterConfig, router};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("axion=info")),
        )
        .json()
        .init();
    let nodes = env::var("AXION_SHARD_NODES")?;
    let node_count = nodes
        .split(',')
        .filter(|entry| !entry.trim().is_empty())
        .count();
    let minimum = env::var("AXION_MIN_SEARCH_SHARDS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(node_count);
    let mut config = ClusterConfig::from_csv(&nodes, minimum)?;
    config.coordinator_api_key = env::var("AXION_CLUSTER_API_KEY")
        .ok()
        .filter(|value| !value.is_empty());
    config.shard_api_key = env::var("AXION_SHARD_API_KEY")
        .ok()
        .filter(|value| !value.is_empty());
    config.request_timeout = Duration::from_millis(
        env::var("AXION_SHARD_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5_000),
    );
    let host = env::var("AXION_CLUSTER_HOST")
        .ok()
        .and_then(|value| value.parse::<IpAddr>().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let port = env::var("AXION_CLUSTER_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8_090);
    let address = SocketAddr::new(host, port);
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, shards = config.nodes.len(), minimum, "Axion cluster coordinator listening");
    axum::serve(listener, router(config)?)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
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
}
