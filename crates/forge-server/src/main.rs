use forge_storage::db::Database;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::net::SocketAddr;
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize production tracing (structured logging)
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,forge_server=debug,forge_api=debug"));
    
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Forge platform...");

    // 2. Load environment variables
    let _ = dotenvy::dotenv(); // Ignore error if .env doesn't exist (e.g. in prod container)

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".to_string());
    
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("PORT must be a valid u16 integer");

    // 3. Connect to the database and run migrations
    info!("Connecting to the database and ensuring migrations are up-to-date...");
    let _db = match Database::new(&db_url).await {
        Ok(db) => {
            info!("Database migrations applied successfully!");
            db
        }
        Err(e) => {
            error!("Failed to connect to the database or apply migrations: {}", e);
            std::process::exit(1);
        }
    };

    // 4. Initialize HTTP API router
    let app = forge_api::create_router(_db.pool().clone());

    // 5. Bind server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Listening on {}", addr);

    // 6. Serve with Graceful Shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Forge platform stopped gracefully.");
    Ok(())
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
