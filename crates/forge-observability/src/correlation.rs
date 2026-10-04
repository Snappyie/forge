//! Correlation identifiers (spec 05 §5.1, AT-OBS-001).
//!
//! A client may supply `X-Request-ID`; when it does not, the server mints one.
//! The identifier is carried through the request, into execution rows, and onto
//! every domain event, so one user action can be traced end to end.

use uuid::Uuid;

pub const REQUEST_ID_HEADER: &str = "x-request-id";
pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";

/// Extracts a caller-supplied request id, or mints one.
///
/// A caller-supplied value is sanitised: a request id ends up in logs and in the
/// database, so an unbounded or control-character-bearing value is rejected
/// rather than echoed.
pub fn resolve_request_id(supplied: Option<&str>) -> String {
    match supplied {
        Some(value) if is_acceptable(value) => value.to_string(),
        _ => Uuid::new_v4().to_string(),
    }
}

/// Whether a supplied identifier is safe to store and log.
fn is_acceptable(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && value.chars().all(|c| c.is_ascii_graphic())
}

/// Extracts a correlation id from a set of headers, falling back to the request
/// id so every request has at least one stable identifier.
pub fn resolve_correlation_id(correlation: Option<&str>, request_id: &str) -> String {
    match correlation {
        Some(value) if is_acceptable(value) => value.to_string(),
        _ => request_id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_supplied_request_id_is_preserved() {
        let id = resolve_request_id(Some("client-abc-123"));
        assert_eq!(id, "client-abc-123");
    }

    #[test]
    fn a_missing_request_id_is_minted() {
        let first = resolve_request_id(None);
        let second = resolve_request_id(None);
        assert!(!first.is_empty());
        assert_ne!(first, second, "each request gets a distinct id");
    }

    /// A hostile request id must not reach logs or the database verbatim.
    #[test]
    fn an_unacceptable_request_id_is_replaced() {
        for hostile in [
            "",
            "has space",
            "has\nnewline",
            "has\ttab",
            &"x".repeat(500),
        ] {
            let resolved = resolve_request_id(Some(hostile));
            assert_ne!(
                resolved, hostile,
                "a hostile id must be replaced, not echoed"
            );
            assert!(resolved.chars().all(|c| c.is_ascii_graphic()));
        }
    }

    #[test]
    fn correlation_falls_back_to_the_request_id() {
        assert_eq!(resolve_correlation_id(None, "req-1"), "req-1");
        assert_eq!(
            resolve_correlation_id(Some("corr-9"), "req-1"),
            "corr-9",
            "an explicit correlation id wins"
        );
        assert_eq!(
            resolve_correlation_id(Some("bad\nid"), "req-1"),
            "req-1",
            "an unusable correlation id falls back rather than propagating"
        );
    }
}
