use forge_storage::db::Database;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::net::SocketAddr;
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize production tracing (structured logging)
    // Config is loaded first so FORGE_LOG_LEVEL / FORGE_LOG_FORMAT take effect
    // from the very first line of output.
    let _ = dotenvy::dotenv(); // Ignore error if .env doesn't exist (e.g. in prod container)

    let config = match forge_config::Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            // Spec 17.6: startup MUST fail on invalid critical configuration.
            // Formatting goes to stderr because tracing may not be up yet.
            eprintln!("configuration error: {e}");
            eprintln!(
                "hint: FORGE_AUTH_SESSION_SECRET and FORGE_API_KEY_HASHING_SECRET are required; \
                 see docs/17-configuration.md for every supported key and its default."
            );
            std::process::exit(1);
        }
    };

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.server.log_level));

    match config.server.log_format {
        forge_config::LogFormat::Json => tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init(),
        forge_config::LogFormat::Text => tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init(),
    }

    info!(
        host = %config.server.host,
        port = config.server.port,
        log_format = ?config.server.log_format,
        "Starting Forge platform..."
    );

    // 2. Connect to the database and run migrations
    info!("Connecting to the database and ensuring migrations are up-to-date...");
    let _db = match Database::new(&config.server.database_url).await {
        Ok(db) => {
            info!("Database migrations applied successfully!");
            db
        }
        Err(e) => {
            error!("Failed to connect to the database or apply migrations: {}", e);
            std::process::exit(1);
        }
    };

    // 3. Initialize HTTP API router
    let app = forge_api::create_router(_db.pool().clone(), &config.server.auth_session_secret);

    // 4. Bind server
    let addr = SocketAddr::from((
        parse_host(&config.server.host),
        config.server.port,
    ));
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Listening on {}", addr);

    // 5. Serve with Graceful Shutdown.
    // `into_make_service_with_connect_info` inserts the peer `SocketAddr` into
    // each request's extensions, which the API's rate limiter needs in order to
    // key limits per client IP.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    info!("Forge platform stopped gracefully.");
    Ok(())
}

/// Resolves `FORGE_SERVER_HOST` to an address to bind.
///
/// Accepts an IPv4 literal (`0.0.0.0`, `127.0.0.1`), an IPv6 literal
/// (`::`, `::1`), or a hostname such as `localhost`. An unresolvable value
/// falls back to binding all interfaces rather than refusing to start.
fn parse_host(host: &str) -> std::net::IpAddr {
    host.parse().unwrap_or(std::net::IpAddr::V4(
        std::net::Ipv4Addr::UNSPECIFIED,
    ))
}

/// Helper function to listen for OS signals (SIGINT, SIGTERM) to initiate graceful shutdown
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down gracefully...");
        },
        _ = terminate => {
            info!("Received SIGTERM, shutting down gracefully...");
        },
    }
}