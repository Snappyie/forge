pub mod alerts;
pub mod applications;
pub mod auth_routes;
pub mod bulk;
pub mod envelope;
pub mod executions;
pub mod extract;
pub mod idempotency;
pub mod insights;
pub mod jobs;
pub mod middleware;
pub mod migration;
pub mod openapi;
pub mod ops;
pub mod parity;
pub mod router;
pub mod rerun;
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

/// Guards the RBAC matrix against the API surface drifting away from it.
///
/// A handler that requires a permission no role grants is effectively
/// owner-only: the console renders the action, the role model says the role
/// should be able to perform it, and the request fails with 403 in production.
/// That is a silent authorization bug, so the strings are read back out of this
/// crate's own source and every one of them must be grantable by a non-owner
/// role.
#[cfg(test)]
mod permission_coverage {
    use forge_auth::Role;
    use std::collections::BTreeSet;
    use std::path::Path;

    /// Every role a tenant can grant to a human, strongest first.
    const NON_OWNER_ROLES: [Role; 5] = [
        Role::Admin,
        Role::Operator,
        Role::Developer,
        Role::Auditor,
        Role::Viewer,
    ];

    /// Every permission string the handlers in this crate require.
    fn required_permissions() -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).expect("src is readable") {
                let path = entry.expect("directory entry is readable").path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                // This file contains the pattern in its own literal, so it
                // would match itself.
                if path.file_name().and_then(|n| n.to_str()) == Some("lib.rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).expect("source is readable");
                for line in source.lines() {
                    let Some(at) = line.find("require(\"") else {
                        continue;
                    };
                    let after = &line[at + "require(\"".len()..];
                    let Some(end) = after.find('"') else {
                        continue;
                    };
                    found.insert(after[..end].to_string());
                }
            }
        }
        found
    }

    #[test]
    fn every_required_permission_is_grantable() {
        let required = required_permissions();
        assert!(
            required.len() > 20,
            "the scan found suspiciously few permissions: {required:?}"
        );

        let ungrantable: Vec<&String> = required
            .iter()
            .filter(|permission| !NON_OWNER_ROLES.iter().any(|role| role.allows(permission)))
            .collect();

        assert!(
            ungrantable.is_empty(),
            "these permissions are required by a handler but granted by no role, \
             so the endpoints are owner-only by accident: {ungrantable:?}"
        );
    }

    /// The matrix is meant to be least-privilege: a read-only role must not be
    /// able to satisfy a mutating permission.
    #[test]
    fn read_only_roles_cannot_satisfy_write_permissions() {
        for permission in required_permissions() {
            let Some((_, action)) = permission.split_once(':') else {
                continue;
            };
            if !matches!(action, "write" | "admin" | "cancel" | "trigger" | "retry") {
                continue;
            }
            assert!(
                !Role::Viewer.allows(&permission),
                "VIEWER must not grant `{permission}`"
            );
            assert!(
                !Role::Auditor.allows(&permission),
                "AUDITOR reads history; `{permission}` would let it mutate"
            );
        }
    }
}
