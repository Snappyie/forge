//! The Forge server binary.
//!
//! Starts the HTTP API and the background runtime (scheduler, lease reaper,
//! outbox publisher, retention, heartbeat monitor), then shuts both down
//! cleanly on SIGINT or SIGTERM.

mod runtime;

use std::net::SocketAddr;
use std::sync::Arc;

use forge_storage::db::Database;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A `.env` is a development convenience; its absence is not an error.
    let _ = dotenvy::dotenv();

    // Spec 17.6: startup MUST fail on invalid critical configuration, so this
    // runs before logging is configured and reports to stderr.
    let config = match forge_config::Config::from_env() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("configuration error: {e}");
            eprintln!(
                "hint: FORGE_AUTH_SESSION_SECRET and FORGE_API_KEY_HASHING_SECRET are required; \
                 see docs/17-configuration.md for every supported key and its default."
            );
            std::process::exit(1);
        }
    };

    init_tracing(&config);
    info!(
        version = env!("CARGO_PKG_VERSION"),
        host = %config.server.host,
        port = config.server.port,
        log_format = ?config.server.log_format,
        "starting Forge"
    );

    // Migrations run at startup; a failure is fatal because the API would
    // otherwise serve against an unknown schema.
    let db = match Database::new(&config.server.database_url).await {
        Ok(db) => {
            info!("database migrations applied");
            db
        }
        Err(e) => {
            error!(error = %e, "could not connect to the database or apply migrations");
            std::process::exit(1);
        }
    };
    let pool = db.pool().clone();

    // A single shutdown signal fans out to the API and every background loop, so
    // one Ctrl-C stops the whole platform rather than half of it. `broadcast`
    // rather than `watch` because several independent waiters each need their own
    // receiver.
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    let background = tokio::spawn({
        let config = Arc::clone(&config);
        let pool = pool.clone();
        let rx = shutdown_tx.subscribe();
        async move {
            runtime::Runtime::new(pool, config, rx).run().await;
        }
    });

    let app = forge_api::create_router(
        pool.clone(),
        &config.server.auth_session_secret,
        &config.server.public_base_url,
    );

    let addr = SocketAddr::from((parse_host(&config.server.host), config.server.port));
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!(%addr, "API listening");

    // `into_make_service_with_connect_info` supplies the peer address the rate
    // limiter needs to key limits per client IP.
    let mut api = tokio::spawn({
        let signal_tx = shutdown_tx.clone();
        async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                shutdown_signal().await;
                // Tell the background loops to stop too.
                let _ = signal_tx.send(());
            })
            .await
        }
    });

    let mut background = background;

    // Whichever finishes first begins the shutdown; the other is awaited so the
    // process does not exit with work still in flight.
    tokio::select! {
        result = &mut api => {
            match result {
                Ok(Ok(())) => info!("API stopped"),
                Ok(Err(e)) => error!(error = %e, "API failed"),
                Err(e) => error!(error = %e, "API task panicked"),
            }
        }
        result = &mut background => {
            if let Err(e) = result {
                error!(error = %e, "background runtime panicked");
            }
            warn!("background runtime exited; stopping the API");
        }
    }

    // Both sides observe the same signal.
    let _ = shutdown_tx.send(());

    // Drain whatever is still running.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), api).await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), background).await;

    info!("Forge stopped");
    Ok(())
}

fn init_tracing(config: &forge_config::Config) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.server.log_level));

    match config.server.log_format {
        forge_config::LogFormat::Json => tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init(),
        forge_config::LogFormat::Text => tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init(),
    }
}

/// Resolves `FORGE_SERVER_HOST` to an address to bind.
///
/// Accepts an IPv4 literal, an IPv6 literal, or a hostname. An unresolvable
/// value falls back to binding every interface rather than refusing to start.
fn parse_host(host: &str) -> std::net::IpAddr {
    host.parse()
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

/// Resolves once SIGINT or SIGTERM arrives.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("could not install a Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("could not install a SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("received SIGINT, shutting down"),
        _ = terminate => info!("received SIGTERM, shutting down"),
    }
}
