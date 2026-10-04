//! Global search (UI.md section 3).
//!
//! One query that returns grouped, counted results across every searchable
//! resource. The console needs a total per group before it renders, so each
//! group carries both a count and its top hits.
//!
//! The spec's search syntax (`status:failed`, `after:2026-09-01`,
//! `duration:>10m`) is parsed here rather than in the browser, so the same query
//! works from the CLI and produces the same results.

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::envelope::{ApiError, ApiResponse};
use crate::extract::Auth;
use crate::router::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

/// A parsed `field:value` term, or a bare free-text term.
#[derive(Debug, PartialEq)]
enum Term {
    Status(String),
    After(chrono::NaiveDate),
}

/// Parses the documented search syntax.
///
/// An unrecognised prefix is left as free text rather than dropped: a user who
/// types `tag:settlement` when tags are unimplemented should still find the
/// literal string, not silence.
fn parse(query: &str) -> (Vec<Term>, Vec<String>) {
    let mut terms = Vec::new();
    let mut words = Vec::new();

    for token in query.split_whitespace() {
        match token.split_once(':') {
            Some(("status", value)) if !value.is_empty() => {
                terms.push(Term::Status(value.to_uppercase()));
            }
            Some(("after", value)) => {
                if let Ok(date) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
                    terms.push(Term::After(date));
                } else {
                    words.push(token.to_string());
                }
            }
            _ => words.push(token.to_string()),
        }
    }

    (terms, words)
}

/// `GET /search` — grouped results with per-group counts.
pub async fn search(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(query): Query<SearchQuery>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let raw = query.q.trim().to_string();
    if raw.is_empty() {
        return Ok(Json(ApiResponse::new(
            json!({ "query": "", "groups": [], "total": 0 }),
            auth.request_id,
        )));
    }

    let (terms, words) = parse(&raw);
    let text = words.join(" ");
    let pattern = format!("%{text}%");
    // Only filter on the empty string when the query was purely field terms
    // (`status:failed`), otherwise every row would match.
    let has_text = !text.is_empty();

    let status_filter = terms.iter().find_map(|term| match term {
        Term::Status(value) => Some(value.clone()),
        _ => None,
    });
    let after_filter = terms.iter().find_map(|term| match term {
        Term::After(date) => Some(*date),
        _ => None,
    });

    let tenant = auth.tenant_id.into_uuid();

    let jobs: Vec<(Value, i64)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'name', name, 'key', key, 'status', status
         ) AS row, COUNT(*) OVER () AS total
         FROM jobs
         WHERE tenant_id = $1
           AND ($2::boolean = FALSE OR (name ILIKE $3 OR key ILIKE $3))
           AND ($4::text IS NULL OR status = $4)
           AND ($5::timestamptz IS NULL OR created_at >= $5)
         ORDER BY updated_at DESC
         LIMIT 5",
    )
    .bind(tenant)
    .bind(has_text)
    .bind(&pattern)
    .bind(&status_filter)
    .bind(after_filter.map(|d| d.and_hms_opt(0, 0, 0).unwrap()))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let executions: Vec<(Value, i64)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'job_id', job_id, 'status', status,
             'error_message', error_message, 'created_at', created_at
         ) AS row, COUNT(*) OVER () AS total
         FROM executions
         WHERE tenant_id = $1
           AND ($2::boolean = FALSE OR (error_message ILIKE $3 OR id::text ILIKE $3))
           AND ($4::text IS NULL OR status = $4)
           AND ($5::timestamptz IS NULL OR created_at >= $5)
         ORDER BY created_at DESC
         LIMIT 5",
    )
    .bind(tenant)
    .bind(has_text)
    .bind(&pattern)
    .bind(&status_filter)
    .bind(after_filter.map(|d| d.and_hms_opt(0, 0, 0).unwrap()))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let workers: Vec<(Value, i64)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'hostname', hostname, 'status', status
         ) AS row, COUNT(*) OVER () AS total
         FROM workers
         WHERE tenant_id = $1
           AND ($2::boolean = FALSE OR hostname ILIKE $3)
         ORDER BY registered_at DESC NULLS LAST
         LIMIT 5",
    )
    .bind(tenant)
    .bind(has_text)
    .bind(&pattern)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let alerts: Vec<(Value, i64)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'title', title, 'severity', severity, 'status', status
         ) AS row, COUNT(*) OVER () AS total
         FROM alerts
         WHERE tenant_id = $1
           AND ($2::boolean = FALSE OR title ILIKE $3 OR detail ILIKE $3)
           AND ($4::text IS NULL OR severity = $4)
         ORDER BY created_at DESC
         LIMIT 5",
    )
    .bind(tenant)
    .bind(has_text)
    .bind(&pattern)
    .bind(&status_filter)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let total = first_total(&jobs)
        + first_total(&executions)
        + first_total(&workers)
        + first_total(&alerts);

    // Groups with nothing are omitted rather than rendered as an empty section
    // with a zero count.
    let mut groups: Vec<Value> = Vec::new();
    push_group(&mut groups, "jobs", "/jobs", "Jobs", &jobs);
    push_group(
        &mut groups,
        "executions",
        "/executions",
        "Executions",
        &executions,
    );
    push_group(&mut groups, "workers", "/workers", "Workers", &workers);
    push_group(&mut groups, "alerts", "/alerts", "Alerts", &alerts);

    Ok(Json(ApiResponse::new(
        json!({ "query": raw, "groups": groups, "total": total }),
        auth.request_id,
    )))
}

/// The `COUNT(*) OVER ()` value from the first row, or zero when there is none.
fn first_total(rows: &[(Value, i64)]) -> i64 {
    rows.first().map(|(_, total)| *total).unwrap_or(0)
}

fn push_group(groups: &mut Vec<Value>, kind: &str, href: &str, label: &str, rows: &[(Value, i64)]) {
    let count = first_total(rows);
    if count == 0 {
        return;
    }
    groups.push(json!({
        "kind": kind,
        "label": label,
        "href": href,
        "count": count,
        "hits": rows.iter().map(|(v, _)| v.clone()).collect::<Vec<_>>(),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_syntax() {
        let (terms, words) = parse("status:failed after:2026-09-01 settlement");
        assert_eq!(
            terms,
            vec![
                Term::Status("FAILED".into()),
                Term::After(chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()),
            ]
        );
        assert_eq!(words, vec!["settlement"]);
    }

    #[test]
    fn an_unknown_prefix_stays_free_text() {
        // `tag:` is unimplemented; the user should still get a literal search
        // rather than silence.
        let (terms, words) = parse("tag:settlement");
        assert!(terms.is_empty());
        assert_eq!(words, vec!["tag:settlement"]);
    }

    #[test]
    fn an_unparseable_date_is_not_swallowed() {
        let (terms, words) = parse("after:notadate");
        assert!(terms.is_empty());
        assert_eq!(words, vec!["after:notadate"]);
    }

    #[test]
    fn a_bare_query_is_all_free_text() {
        let (terms, words) = parse("settlement");
        assert!(terms.is_empty());
        assert_eq!(words, vec!["settlement"]);
    }

    #[test]
    fn status_is_upper_cased_so_filters_match_storage() {
        let (terms, _) = parse("status:failed");
        assert_eq!(terms, vec![Term::Status("FAILED".into())]);
    }
}
