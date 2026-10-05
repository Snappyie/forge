//! The Forge server binary.
//!
//! Starts the HTTP API and the background runtime (scheduler, lease reaper,
//! outbox publisher, retention, heartbeat monitor), then shuts both down
//! cleanly on SIGINT or SIGTERM.

use forge_server::{console, runtime};

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

    // `--migrate-only` applies migrations and exits.
    //
    // The server migrates on startup, which is right for `run_local.sh` and for
    // docker-compose, but wrong for Kubernetes: three replicas starting together
    // race each other on the same DDL, and a `Job` built on that assumption never
    // completes because the process keeps running instead of exiting. Deployments
    // run this as a migration step and then start the replicas.
    if std::env::args().any(|arg| arg == "--migrate-only") {
        return migrate_only().await;
    }

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

    // Typed explicitly: `Runtime::run` returns nothing, and the shutdown match
    // below needs to know that.
    let mut background: tokio::task::JoinHandle<()> = tokio::spawn({
        let config = Arc::clone(&config);
        let pool = pool.clone();
        let rx = shutdown_tx.subscribe();
        async move {
            runtime::Runtime::new(pool, config, rx).run().await;
        }
    });

    // The API, with the console's fallback mounted underneath it.
    //
    // Order matters: the API's own routes are matched first, so `/api/v1/**` can
    // never be shadowed by the console, and the console's fallback answers only
    // what the API did not claim. That is what lets one binary serve both from
    // one origin, with no CORS and no second port.
    let app = forge_api::create_router(
        pool.clone(),
        &config.server.auth_session_secret,
        &config.server.public_base_url,
        config.server.allow_open_registration,
        config.limits.max_request_body_bytes,
        config.limits.max_log_bytes,
        config.server.api_key_hashing_secret.as_bytes(),
    )
    .fallback(console::serve);

    let addr = SocketAddr::from((parse_host(&config.server.host), config.server.port));
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!(%addr, "API listening");

    // `into_make_service_with_connect_info` supplies the peer address the rate
    // limiter needs to key limits per client IP.
    // Typed explicitly so the shutdown match below can name both handle types.
    let mut api: tokio::task::JoinHandle<std::io::Result<()>> = tokio::spawn({
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

    // Whichever finishes first begins the shutdown.
    //
    // `select!` polls by mutable reference, so both handles survive it and the
    // losing one can still be awaited. The `Result` is taken from the winner
    // only, because a completed `JoinHandle` panics if awaited twice.
    enum First {
        Api(std::io::Result<()>),
        Background(Result<(), tokio::task::JoinError>),
    }

    let (first, pending_api) = tokio::select! {
        result = &mut api => (First::Api(result.expect("the API task must not panic")), None),
        result = &mut background => (
            First::Background(result),
            Some(api),
        ),
    };

    match first {
        First::Api(Ok(())) => info!("API stopped"),
        First::Api(Err(e)) => error!(error = %e, "API failed"),
        First::Background(Ok(())) => {
            info!("background runtime stopped");
            warn!("background runtime exited; stopping the API");
        }
        First::Background(Err(e)) => {
            error!(error = %e, "background runtime panicked");
        }
    }

    // Tell anything still running to stop.
    let _ = shutdown_tx.send(());

    // Drain the API if the background task finished first.
    if let Some(handle) = pending_api {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), handle).await;
    }

    info!("Forge stopped");
    Ok(())
}

fn init_tracing(config: &forge_config::Config) {
    // The HTTP trace spans come from `tower_http`, whose target the bare level
    // would otherwise leave off — so request lines, and the request id on them,
    // would never be logged. `RUST_LOG` still wins when set.
    let directives = format!("{},tower_http=debug", config.server.log_level);
    let filter = match std::env::var("RUST_LOG") {
        Ok(explicit) => tracing_subscriber::EnvFilter::new(explicit),
        Err(_) => tracing_subscriber::EnvFilter::new(directives),
    };

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
/// Applies migrations and exits, so a Kubernetes `Job` can complete.
async fn migrate_only() -> Result<(), Box<dyn std::error::Error>> {
    // Configuration is validated exactly as in a normal start, so a bad secret is
    // reported the same way rather than surfacing later as a confusing database
    // error.
    let config = match forge_config::Config::from_env() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(1);
        }
    };
    init_tracing(&config);

    info!("applying database migrations");
    match Database::new(&config.server.database_url).await {
        Ok(_) => {
            info!("database migrations applied");
            Ok(())
        }
        Err(e) => {
            // A non-zero exit is what makes the Job's retry policy kick in, so
            // this must fail the process rather than only logging.
            error!(error = %e, "could not apply migrations");
            std::process::exit(1);
        }
    }
}

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
