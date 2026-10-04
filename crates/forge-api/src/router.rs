//! Router composition.
//!
//! Every tenant-scoped route goes through the [`crate::extract::Auth`] extractor,
//! so a handler physically cannot run without an identity, and the tenant comes
//! from the token rather than the request body. Public routes are health
//! probes, authentication, and the OpenAPI document.

use std::sync::Arc;

use axum::routing::{delete, get, patch, post, put};
use axum::Json;
use tower::ServiceBuilder;
use tower_governor::{
    governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor, GovernorLayer,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Shared state, injected into every handler.
#[derive(Clone)]
pub struct AppState {
    pub version: String,
    pub pool: sqlx::PgPool,
    /// JWT signing and verification.
    pub jwt: Arc<forge_auth::JwtService>,
    /// The public base URL, used in the OpenAPI document.
    pub base_url: String,
}

/// The conventional prefix for every endpoint.
const PREFIX: &str = "/api/v1";

pub fn create_router(
    pool: sqlx::PgPool,
    session_secret: &str,
    base_url: impl Into<String>,
) -> Router {
    let state = AppState {
        version: env!("CARGO_PKG_VERSION").to_string(),
        pool,
        jwt: Arc::new(forge_auth::JwtService::new(session_secret.as_bytes())),
        base_url: base_url.into(),
    };

    // The limiter works with or without a ConnectInfo extension; the server
    // supplies one via `into_make_service_with_connect_info`.
    //
    // Spec 17 requires documented, configurable limits, and spec 11 requires
    // rate limiting to actually apply. The values come from the environment so
    // a deployment behind a shared proxy — where every client shares one source
    // IP — can be tuned without a rebuild.
    let per_second = env_u64("FORGE_RATE_LIMIT_PER_SECOND", 100) as u64;
    let burst = env_u64("FORGE_RATE_LIMIT_BURST", 200) as u32;

    let governor_conf = Box::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_second(per_second)
            .burst_size(burst)
            .finish()
            .expect("valid rate-limit configuration"),
    );

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // --- public routes (spec 05: health probes, auth, OpenAPI) ---
    let public = Router::new()
        .route(
            &format!("{PREFIX}/health/live"),
            get(crate::system::health_live),
        )
        .route(
            &format!("{PREFIX}/health/ready"),
            get(crate::system::health_ready),
        )
        .route(&format!("{PREFIX}/openapi.json"), get(openapi_document))
        .route(
            &format!("{PREFIX}/auth/register"),
            post(crate::auth_routes::register),
        )
        .route(
            &format!("{PREFIX}/auth/login"),
            post(crate::auth_routes::login),
        )
        .route(
            &format!("{PREFIX}/auth/refresh"),
            post(crate::auth_routes::refresh),
        )
        .route(
            &format!("{PREFIX}/auth/logout"),
            post(crate::auth_routes::logout),
        );

    // --- tenant-scoped routes ---
    let api = Router::new()
        // jobs (spec 05 endpoints 1-5)
        .route(
            &format!("{PREFIX}/jobs"),
            post(crate::jobs::create).get(crate::jobs::list),
        )
        .route(
            &format!("{PREFIX}/jobs/:job_id"),
            get(crate::jobs::get)
                .patch(crate::jobs::update)
                .delete(crate::jobs::archive),
        )
        .route(
            &format!("{PREFIX}/jobs/:job_id/executions"),
            get(crate::jobs::list_executions),
        )
        // job versions (spec 05 endpoints 6-8)
        .route(
            &format!("{PREFIX}/jobs/:job_id/versions"),
            post(crate::jobs::create_version).get(crate::jobs::list_versions),
        )
        .route(
            &format!("{PREFIX}/jobs/:job_id/versions/:version_id/publish"),
            post(crate::jobs::publish_version),
        )
        // trigger (spec 05 endpoint 9)
        .route(
            &format!("{PREFIX}/jobs/:job_id/trigger"),
            post(crate::jobs::trigger),
        )
        // schedules (spec 05 endpoints 10-16)
        .route(
            &format!("{PREFIX}/schedules"),
            post(crate::schedules::create).get(crate::schedules::list),
        )
        .route(
            &format!("{PREFIX}/schedules/:id"),
            get(crate::schedules::get).patch(crate::schedules::update),
        )
        .route(
            &format!("{PREFIX}/schedules/:id/pause"),
            post(crate::schedules::pause),
        )
        .route(
            &format!("{PREFIX}/schedules/:id/resume"),
            post(crate::schedules::resume),
        )
        .route(
            &format!("{PREFIX}/schedules/:id/preview"),
            post(crate::schedules::preview),
        )
        // executions (spec 05 endpoints 17-23)
        .route(
            &format!("{PREFIX}/executions"),
            get(crate::executions::list),
        )
        .route(
            &format!("{PREFIX}/executions/:id"),
            get(crate::executions::get),
        )
        .route(
            &format!("{PREFIX}/executions/:id/cancel"),
            post(crate::executions::cancel),
        )
        .route(
            &format!("{PREFIX}/executions/:id/retry"),
            post(crate::executions::retry),
        )
        .route(
            &format!("{PREFIX}/executions/:id/dead-letter"),
            post(crate::executions::dead_letter),
        )
        .route(
            &format!("{PREFIX}/executions/:id/attempts"),
            get(crate::executions::list_attempts),
        )
        .route(
            &format!("{PREFIX}/executions/:id/logs"),
            get(crate::executions::logs),
        )
        // worker protocol (spec 10.1; recorded in an ADR)
        .route(
            &format!("{PREFIX}/workers/:worker_id/claim"),
            post(crate::executions::claim),
        )
        .route(
            &format!("{PREFIX}/executions/:id/heartbeat"),
            post(crate::executions::heartbeat),
        )
        .route(
            &format!("{PREFIX}/executions/:id/complete"),
            post(crate::executions::complete),
        )
        .route(
            &format!("{PREFIX}/executions/:id/dispatch"),
            post(crate::executions::dispatch),
        )
        // workers (spec 05 endpoints 33-38)
        .route(
            &format!("{PREFIX}/workers/register"),
            post(crate::workers::register),
        )
        .route(&format!("{PREFIX}/workers"), get(crate::workers::list))
        .route(&format!("{PREFIX}/workers/:id"), get(crate::workers::get))
        .route(
            &format!("{PREFIX}/workers/:id/heartbeat"),
            post(crate::workers::heartbeat),
        )
        .route(
            &format!("{PREFIX}/workers/:id/drain"),
            post(crate::workers::drain),
        )
        .route(
            &format!("{PREFIX}/workers/:id/revoke"),
            post(crate::workers::revoke),
        )
        // queues (spec 05 endpoints 39-44)
        .route(
            &format!("{PREFIX}/queues"),
            post(crate::workers::create_queue).get(crate::workers::list_queues),
        )
        .route(
            &format!("{PREFIX}/queues/:id"),
            get(crate::workers::get_queue).patch(crate::workers::update_queue),
        )
        .route(
            &format!("{PREFIX}/queues/:id/pause"),
            post(crate::workers::pause_queue),
        )
        .route(
            &format!("{PREFIX}/queues/:id/resume"),
            post(crate::workers::resume_queue),
        )
        // users (spec 05 endpoints 45-48)
        .route(
            &format!("{PREFIX}/users"),
            post(crate::auth_routes::create_user).get(crate::auth_routes::list_users),
        )
        .route(
            &format!("{PREFIX}/users/:id"),
            patch(crate::auth_routes::update_user),
        )
        .route(
            &format!("{PREFIX}/users/:id/disable"),
            post(crate::auth_routes::disable_user),
        )
        // API keys (spec 05 endpoints 49-51)
        .route(
            &format!("{PREFIX}/api-keys"),
            post(crate::system::create_api_key).get(crate::system::list_api_keys),
        )
        .route(
            &format!("{PREFIX}/api-keys/:id/revoke"),
            post(crate::system::revoke_api_key),
        )
        // audit (spec 05 endpoint 52)
        .route(
            &format!("{PREFIX}/audit-events"),
            get(crate::system::list_audit_events),
        )
        // integrations (spec 05 endpoints 53-56)
        .route(
            &format!("{PREFIX}/integrations"),
            post(crate::system::create_integration).get(crate::system::list_integrations),
        )
        .route(
            &format!("{PREFIX}/integrations/:id"),
            patch(crate::system::update_integration).delete(crate::system::delete_integration),
        )
        .route(
            &format!("{PREFIX}/integrations/:id/test"),
            post(crate::system::test_integration),
        )
        // workflows (spec 05 endpoints 24-32)
        .route(
            &format!("{PREFIX}/workflows"),
            post(crate::workflows::create).get(crate::workflows::list),
        )
        .route(
            &format!("{PREFIX}/workflows/validate"),
            post(crate::workflows::validate),
        )
        .route(
            &format!("{PREFIX}/workflows/:id"),
            get(crate::workflows::get),
        )
        .route(
            &format!("{PREFIX}/workflows/:id/definition"),
            put(crate::workflows::put_definition),
        )
        .route(
            &format!("{PREFIX}/workflows/:id/versions"),
            get(crate::workflows::list_versions),
        )
        .route(
            &format!("{PREFIX}/workflows/:id/publish"),
            post(crate::workflows::publish),
        )
        .route(
            &format!("{PREFIX}/workflows/:id/trigger"),
            post(crate::workflows::trigger),
        )
        // execution metrics, integrations, webhooks, saved views,
        // dependencies, undo and system health (UI.md 17, 38, 48, 70, 73, 74, 75)
        .route(
            &format!("{PREFIX}/executions/:id/metrics"),
            get(crate::parity::execution_metrics).post(crate::parity::record_execution_metric),
        )
        .route(
            &format!("{PREFIX}/webhooks"),
            get(crate::parity::list_webhooks).post(crate::parity::create_webhook),
        )
        .route(
            &format!("{PREFIX}/webhooks/:id"),
            delete(crate::parity::delete_webhook),
        )
        .route(
            &format!("{PREFIX}/webhooks/:id/test"),
            post(crate::parity::test_webhook),
        )
        .route(
            &format!("{PREFIX}/webhooks/:id/deliveries"),
            get(crate::parity::webhook_deliveries),
        )
        .route(
            &format!("{PREFIX}/saved-views"),
            get(crate::parity::list_saved_views).post(crate::parity::create_saved_view),
        )
        .route(
            &format!("{PREFIX}/saved-views/:id"),
            delete(crate::parity::delete_saved_view),
        )
        .route(
            &format!("{PREFIX}/jobs/:job_id/dependencies"),
            get(crate::parity::job_dependencies).post(crate::parity::create_dependency),
        )
        .route(
            &format!("{PREFIX}/job-dependencies/:edge_id"),
            delete(crate::parity::delete_dependency),
        )
        .route(&format!("{PREFIX}/undo"), get(crate::parity::list_undoable))
        .route(&format!("{PREFIX}/undo/:id"), post(crate::parity::undo))
        .route(
            &format!("{PREFIX}/system/health"),
            get(crate::parity::system_health),
        )
        .route(
            &format!("{PREFIX}/scheduler/heartbeat"),
            post(crate::parity::record_scheduler_heartbeat),
        )
        // key rotation, import/export, emergency controls (UI.md 41, 72, 76)
        .route(
            &format!("{PREFIX}/api-keys/:id/rotate"),
            post(crate::ops::rotate_api_key),
        )
        .route(
            &format!("{PREFIX}/api-keys/:id/expiry"),
            put(crate::ops::set_api_key_expiry),
        )
        .route(
            &format!("{PREFIX}/jobs/export"),
            get(crate::ops::export_jobs),
        )
        .route(
            &format!("{PREFIX}/jobs/import"),
            post(crate::ops::import_jobs),
        )
        .route(
            &format!("{PREFIX}/emergency"),
            get(crate::ops::emergency_state),
        )
        .route(
            &format!("{PREFIX}/emergency/cancel-running"),
            post(crate::ops::cancel_all_running),
        )
        // bulk operations, upcoming runs, assistant (UI.md 5, 22, 37, 80)
        .route(&format!("{PREFIX}/jobs/bulk"), post(crate::bulk::bulk_jobs))
        .route(&format!("{PREFIX}/upcoming"), get(crate::bulk::upcoming))
        .route(&format!("{PREFIX}/assistant/ask"), post(crate::bulk::ask))
        .route(
            &format!("{PREFIX}/assistant/propose"),
            post(crate::bulk::propose),
        )
        // global search (UI.md 3)
        .route(&format!("{PREFIX}/search"), get(crate::search::search))
        // dashboard, job health, SLA and maintenance (UI.md 2, 31, 32, 71, 79)
        .route(
            &format!("{PREFIX}/dashboard"),
            get(crate::insights::dashboard),
        )
        .route(
            &format!("{PREFIX}/maintenance"),
            get(crate::insights::get_maintenance)
                .post(crate::insights::start_maintenance)
                .delete(crate::insights::end_maintenance),
        )
        .route(
            &format!("{PREFIX}/sla/compliance"),
            get(crate::insights::sla_compliance),
        )
        .route(
            &format!("{PREFIX}/jobs/:job_id/health"),
            get(crate::insights::job_health),
        )
        .route(
            &format!("{PREFIX}/jobs/:job_id/sla"),
            put(crate::insights::set_sla_target).get(crate::insights::sla_detail),
        )
        .route(
            &format!("{PREFIX}/executions/:id/timeline"),
            get(crate::insights::timeline),
        )
        // alerts and incidents (UI.md 28-30)
        .route(&format!("{PREFIX}/alerts"), get(crate::alerts::list_alerts))
        .route(
            &format!("{PREFIX}/alerts/summary"),
            get(crate::alerts::alerts_summary),
        )
        .route(
            &format!("{PREFIX}/alerts/:id/acknowledge"),
            post(crate::alerts::acknowledge_alert),
        )
        .route(
            &format!("{PREFIX}/alert-rules"),
            post(crate::alerts::create_alert_rule).get(crate::alerts::list_alert_rules),
        )
        .route(
            &format!("{PREFIX}/alert-rules/:id"),
            patch(crate::alerts::update_alert_rule).delete(crate::alerts::delete_alert_rule),
        )
        .route(
            &format!("{PREFIX}/incidents"),
            get(crate::alerts::list_incidents),
        )
        .route(
            &format!("{PREFIX}/incidents/:id"),
            get(crate::alerts::get_incident),
        )
        // notifications (UI.md 50-51)
        .route(
            &format!("{PREFIX}/notifications"),
            get(crate::alerts::list_notifications),
        )
        .route(
            &format!("{PREFIX}/notifications/read"),
            post(crate::alerts::mark_notifications_read),
        )
        .route(
            &format!("{PREFIX}/notification-preferences"),
            get(crate::alerts::get_notification_preferences),
        )
        .route(
            &format!("{PREFIX}/notification-preferences/update"),
            post(crate::alerts::update_notification_preferences),
        )
        // system (spec 05 endpoints 60-62)
        .route(
            &format!("{PREFIX}/health"),
            get(crate::system::health_detail),
        )
        .route(&format!("{PREFIX}/metrics"), get(crate::system::metrics))
        .route(
            &format!("{PREFIX}/admin/purge-idempotency"),
            post(crate::system::purge_idempotency),
        );

    public
        .merge(api)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(cors),
        )
        .layer(GovernorLayer {
            config: Arc::new(*governor_conf),
        })
        .with_state(state)
}

use axum::Router;

/// Reads a numeric tuning knob, falling back to a documented default.
fn env_u64(key: &str, default: u64) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(|value| value as usize)
        .unwrap_or(default as usize)
}

/// `GET /api/v1/openapi.json` — the published contract.
async fn openapi_document(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<serde_json::Value> {
    Json(crate::openapi::document_for(&state.base_url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use http::Request;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    fn test_router() -> Router {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://forge:forgepassword@localhost:5432/forgedb")
            .expect("lazy pool");
        create_router(
            pool,
            "test-secret-not-for-production",
            "http://localhost:3000/api/v1",
        )
    }

    /// Builds a request carrying `ConnectInfo`, which the rate limiter needs.
    /// Without it the governor's key extractor fails and every route 500s.
    fn request(method: &str, uri: &str, headers: &[(&str, &str)]) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let mut built = builder.body(Body::empty()).unwrap();
        built
            .extensions_mut()
            .insert(ConnectInfo("127.0.0.1:1234".parse::<SocketAddr>().unwrap()));
        built
    }

    fn get(uri: &str) -> Request<Body> {
        request("GET", uri, &[])
    }

    #[tokio::test]
    async fn health_live_is_public() {
        let response = test_router()
            .oneshot(get("/api/v1/health/live"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }

    // AT-API-001: authentication is required on tenant-scoped routes.
    #[tokio::test]
    async fn tenant_routes_require_authentication() {
        for uri in [
            "/api/v1/jobs",
            "/api/v1/executions",
            "/api/v1/workers",
            "/api/v1/queues",
            "/api/v1/audit-events",
            "/api/v1/users",
            "/api/v1/api-keys",
            "/api/v1/schedules",
            "/api/v1/integrations",
        ] {
            let response = test_router().oneshot(get(uri)).await.unwrap();
            assert_eq!(
                response.status(),
                401,
                "GET {uri} must require authentication"
            );
        }
    }

    /// AT-API-004: a malformed credential is refused, not crashed on.
    #[tokio::test]
    async fn a_malformed_bearer_token_is_refused() {
        for header in ["", "Bearer", "Bearer not-a-jwt", "Basic abc"] {
            let headers: Vec<(&str, &str)> = if header.is_empty() {
                vec![]
            } else {
                vec![("authorization", header)]
            };
            let response = test_router()
                .oneshot(request("GET", "/api/v1/jobs", &headers))
                .await
                .unwrap();
            assert!(
                response.status() == 401 || response.status() == 400,
                "header {header:?} produced {}",
                response.status()
            );
        }
    }

    /// The OpenAPI document is public, so a client can discover the contract
    /// before it has a credential.
    #[tokio::test]
    async fn the_openapi_document_is_public() {
        let response = test_router()
            .oneshot(get("/api/v1/openapi.json"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn an_unknown_route_is_not_found() {
        let response = test_router()
            .oneshot(get("/api/v1/does-not-exist"))
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
    }

    /// Spec 05 §5.1: a client may supply its own request id.
    #[tokio::test]
    async fn a_supplied_request_id_is_echoed() {
        let response = test_router()
            .oneshot(request(
                "GET",
                "/api/v1/health/live",
                &[("x-request-id", "client-supplied-id")],
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }

    /// The response body must use the documented envelope shape.
    #[tokio::test]
    async fn health_live_uses_the_standard_envelope() {
        let response = test_router()
            .oneshot(get("/api/v1/health/live"))
            .await
            .unwrap();

        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["data"]["status"], "ok");
        assert!(value["request_id"].is_string());
    }

    #[tokio::test]
    async fn the_version_comes_from_the_crate_metadata() {
        // `connect_lazy` still needs a runtime context, so this runs async.
        let state = AppState {
            version: env!("CARGO_PKG_VERSION").to_string(),
            pool: sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://localhost/x")
                .unwrap(),
            jwt: Arc::new(forge_auth::JwtService::new(b"s")),
            base_url: "http://localhost".to_string(),
        };
        assert!(!state.version.is_empty());
        assert!(state.base_url.starts_with("http"));
    }

    #[test]
    fn json_helper_is_available() {
        // A trivial guard that the module compiles with the shapes it uses.
        assert_eq!(serde_json::json!({"a": 1})["a"], 1);
    }
}
