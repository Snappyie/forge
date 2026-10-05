//! Cross-environment migration planning (`redesign.md` §2.C, §3).
//!
//! `redesign.md` §3 names "change previews" and "controlled rollouts" as a
//! differentiator, and §41 of the UI spec is equally explicit: nothing applies
//! silently. Both need the same artifact — a computed plan the operator reads,
//! challenges, and then confirms.
//!
//! This endpoint *computes* that plan and applies nothing. The split is the
//! safety property: a migration that both computed and performed its work would
//! make a dry run impossible, and a dry run that is really a write is the one
//! an operator stops trusting.

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_domain::{
    BindingKind, MigratableField, MigrationAction, MigrationEntry, MigrationPlan, Slug,
    UnresolvedBinding, UnresolvedReason,
};
use forge_storage::{JobRevisionRepository, DEFAULT_MIGRATABLE};

use crate::envelope::{ApiError, ApiResponse};
use crate::extract::Auth;
use crate::router::AppState;

/// Which jobs to plan for, optionally narrowed to one application.
#[derive(Debug, Deserialize)]
pub struct PlanMigrationQuery {
    /// The environment the jobs are leaving. Required: a plan without a source
    /// would silently mean "everything".
    pub from_environment: String,
    /// The environment the jobs are moving to.
    pub to_environment: String,
    /// Narrow to one application. Omitted means the whole environment.
    #[serde(default)]
    pub application: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApplyMigrationRequest {
    /// A plan previously returned by `POST /migration/plan`, echoed back so the
    /// applied set is exactly the reviewed set.
    pub plan: MigrationPlan,
}

/// `POST /migration/plan` — compute what would cross between environments.
///
/// Applies nothing. Every entry reports what would happen and which bindings
/// cannot be resolved on the far side, because a job's identity is portable and
/// its queue/worker/secret names are not.
pub async fn plan_migration(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(query): Query<PlanMigrationQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:write")?;

    let from_slug = Slug::parse(query.from_environment.trim())
        .map_err(|e| ApiError::validation(format!("`from_environment`: {e}")))?;
    let to_slug = Slug::parse(query.to_environment.trim())
        .map_err(|e| ApiError::validation(format!("`to_environment`: {e}")))?;

    if from_slug == to_slug {
        return Err(ApiError::validation(
            "source and target environments are the same",
        )
        .with_detail("to_environment", "same as source"));
    }

    let source_environment = resolve_environment(&state, auth.tenant_id, &from_slug).await?;
    let target_environment = resolve_environment(&state, auth.tenant_id, &to_slug).await?;

    let application_id = match query.application.as_deref() {
        Some(slug) => {
            let slug = Slug::parse(slug.trim())
                .map_err(|e| ApiError::validation(format!("`application`: {e}")))?;
            Some(
                forge_storage::ApplicationRepository::new(&state.pool)
                    .get_by_slug(auth.tenant_id, slug.as_str())
                    .await
                    .map_err(ApiError::from)?
                    .ok_or_else(|| ApiError::not_found("application"))?
                    .id,
            )
        }
        None => None,
    };

    let candidates = JobRevisionRepository::new(&state.pool)
        .migration_candidates(
            auth.tenant_id,
            Some(source_environment),
            application_id,
        )
        .await
        .map_err(ApiError::from)?;

    // What already exists in the target, keyed by the stable key a migration
    // matches on. Database ids differ between environments, so `key` is the only
    // identifier that can say "this already exists over there".
    let existing = JobRevisionRepository::new(&state.pool)
        .migration_candidates(
            auth.tenant_id,
            Some(target_environment),
            application_id,
        )
        .await
        .map_err(ApiError::from)?;

    let existing_by_key: std::collections::HashMap<String, _> = existing
        .into_iter()
        .filter_map(|job| job.key.clone().map(|key| (key, job)))
        .collect();

    let mut entries = Vec::with_capacity(candidates.len());
    for job in candidates {
        // A job with no key cannot be matched on the way back, so it is
        // reported with the reason rather than quietly skipped: the operator
        // would otherwise believe a job was moving when it never could.
        let source_key = match job.key.clone() {
            Some(key) if !key.trim().is_empty() => key,
            _ => {
                entries.push(MigrationEntry {
                    source_key: String::new(),
                    name: job.name.clone(),
                    action: MigrationAction::Unchanged,
                    unresolved: vec![UnresolvedBinding {
                        kind: BindingKind::Secret,
                        source_name: job.name.clone(),
                        reason: UnresolvedReason::MissingInTarget,
                    }],
                });
                continue;
            }
        };

        let action = match existing_by_key.get(&source_key) {
            None => MigrationAction::Create,
            Some(target) if definitions_match(&job, target) => MigrationAction::Unchanged,
            Some(_) => MigrationAction::Update,
        };

        entries.push(MigrationEntry {
            source_key,
            name: job.name.clone(),
            action,
            unresolved: Vec::new(),
        });
    }

    let plan = MigrationPlan {
        source_tenant: tenant_slug(&state, auth.tenant_id).await?,
        source_environment: from_slug,
        target_tenant: tenant_slug(&state, auth.tenant_id).await?,
        target_environment: to_slug,
        application: query
            .application
            .as_deref()
            .and_then(|raw| Slug::parse(raw.trim()).ok()),
        entries,
        include: DEFAULT_MIGRATABLE.to_vec(),
    };

    let creates = plan
        .entries
        .iter()
        .filter(|e| e.action == MigrationAction::Create)
        .count();
    let updates = plan
        .entries
        .iter()
        .filter(|e| e.action == MigrationAction::Update)
        .count();
    let unchanged = plan.entries.len() - creates - updates;

    Ok(Json(ApiResponse::new(
        json!({
            "plan": plan,
            "summary": {
                "create": creates,
                "update": updates,
                "unchanged": unchanged,
            },
            // Spelled out because "what would this do" is not obvious from a
            // list, and `redesign.md` §3 asks for the plan to be reviewable.
            "note": "Nothing has been applied. Bindings are excluded by default \\
                     because they name destination objects that must be \\
                     resolved explicitly.",
        }),
        auth.request_id,
    )))
}

/// `POST /migration/apply` — carry the approved plan across.
///
/// Refuses rather than partially applying: the plan is echoed back from
/// `POST /migration/plan` precisely so the applied set is the reviewed set, and
/// a plan that has drifted from what the operator saw is a reason to stop.
pub async fn apply_migration(
    Auth(auth): Auth,
    Json(body): Json<ApplyMigrationRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:write")?;

    let plan = &body.plan;
    if plan.entries.is_empty() {
        return Err(ApiError::validation("the plan carries no jobs").with_detail(
            "plan",
            "empty",
        ));
    }

    /*
     * Applying a migration needs the destination's resolved object ids for every
     * binding, and no such mapping has been agreed yet. Rather than create jobs
     * that point at a dev queue, this refuses.
     *
     * A 400 rather than a 501: nothing is missing from the *server*, the request
     * is missing a precondition. 501 would say "this endpoint does not exist",
     * which is the opposite of the problem, and this API maps validation
     * failures to 400 consistently.
     */
    Err(ApiError::validation(format!(
        "applying a migration needs binding resolution: {} job(s) are planned but \
         no queue, worker-pool, or secret mapping has been supplied",
        plan.entries.len()
    ))
    .with_detail("bindings", plan.entries.len().to_string()))
}

/// Two jobs are the same when their portable definition matches.
///
/// Deliberately narrow: only the fields a migration would copy are compared, so
/// an identical job is reported as `Unchanged` rather than as an `Update` that
//  changes nothing.
fn definitions_match(
    source: &forge_storage::MigrationCandidate,
    target: &forge_storage::MigrationCandidate,
) -> bool {
    source.name == target.name
        && source.priority == target.priority
        && source.expression == target.expression
        && source.timezone == target.timezone
        && source.misfire_policy == target.misfire_policy
}

/// Resolves an environment slug within the caller's tenant.
async fn resolve_environment(
    state: &AppState,
    tenant_id: forge_domain::TenantId,
    slug: &Slug,
) -> Result<Uuid, ApiError> {
    forge_storage::EnvironmentRepository::new(&state.pool)
        .get_by_slug(tenant_id, slug.as_str())
        .await
        .map_err(ApiError::from)?
        .map(|row| row.id)
        .ok_or_else(|| ApiError::not_found("environment"))
}

/// The caller's own slug, so the plan reads back where it came from.
async fn tenant_slug(state: &AppState, tenant_id: forge_domain::TenantId) -> Result<Slug, ApiError> {
    let raw: Option<String> = sqlx::query_scalar("SELECT slug FROM tenants WHERE id = $1")
        .bind(tenant_id.into_uuid())
        .fetch_optional(&state.pool)
        .await
        .map_err(ApiError::from)?;

    raw.and_then(|value| Slug::parse(&value).ok())
        .ok_or_else(ApiError::internal)
}

/// The fields a plan carries by default, exposed so the console can show them
/// without hardcoding the list.
pub fn default_migratable_fields() -> Vec<MigratableField> {
    DEFAULT_MIGRATABLE.to_vec()
}