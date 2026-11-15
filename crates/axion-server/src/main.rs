use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use axion_api::{ApiConfig, router};
use axion_query::CollectionEngine;
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
    let api_key = env::var("AXION_API_KEY")
        .ok()
        .filter(|value| !value.is_empty());
    let allow_any_origin =
        env::var("AXION_CORS_ALLOW_ANY").is_ok_and(|value| value.eq_ignore_ascii_case("true"));

    let engine = CollectionEngine::open(data_dir)?;
    let app = router(
        engine,
        ApiConfig {
            api_key,
            allow_any_origin,
            ..ApiConfig::default()
        },
    );
    let address = SocketAddr::new(host, port);
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "Axion REST server listening");
    axum::serve(listener, app)
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
    tracing::info!("graceful shutdown requested");
}
