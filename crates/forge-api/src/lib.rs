pub mod alerts;
pub mod auth_routes;
pub mod bulk;
pub mod envelope;
pub mod executions;
pub mod extract;
pub mod idempotency;
pub mod insights;
pub mod jobs;
pub mod middleware;
pub mod openapi;
pub mod ops;
pub mod parity;
pub mod router;
pub mod schedules;
pub mod search;
pub mod system;
pub mod workers;
pub mod workflows;

pub use envelope::{
    ApiError, ApiErrorDetail, ApiErrorResponse, ApiResponse, ListResponse, PageInfo,
    PaginationQuery,
};
pub use router::{create_router, AppState};

/// Reads the request id from the headers, or mints one (spec 05 §5.1).
///
/// The value is used for the response envelope so a client can correlate its
/// request with our logs without a separate header round trip.
pub fn extract_request_id(parts: &axum::http::request::Parts) -> String {
    use forge_observability::correlation::REQUEST_ID_HEADER;

    let supplied = parts
        .headers
        .get(REQUEST_ID_HEADER)
        .and_then(|h| h.to_str().ok());

    forge_observability::correlation::resolve_request_id(supplied)
}
