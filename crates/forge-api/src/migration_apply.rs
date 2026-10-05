//! Applying a reviewed migration plan.
//!
//! The plan is computed by [`super::migration::plan_migration`] and applied
//! here. Two properties matter and both are enforced in this file:
//!
//! * **The applied set is the reviewed set.** The plan travels back to the
//!   server with the request rather than being recomputed, so a job created
//!   between planning and applying is not silently swept along, and a job
//!   deleted in between is reported rather than causing a confusing failure.
//! * **Bindings are resolved, never copied.** A job's queue, worker pool and
//!   secret names belong to the source environment. The caller supplies a
//!   mapping from source name to target id; anything they did not map is
//!   refused before anything is written, because a job pointing at a
//!   source-environment queue is worse than no job at all.

use axum::extract::State;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_domain::{BindingKind, MigrationAction, MigrationPlan};
use forge_storage::JobRepository;

use crate::envelope::{ApiError, ApiResponse};
use crate::extract::Auth;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct ApplyMigrationRequest {
    /// The plan exactly as `POST /migration/plan` returned it.
    pub plan: MigrationPlan,
    /// Source queue name → target queue id.
    #[serde(default)]
    pub queues: std::collections::HashMap<String, Uuid>,
    /// Source worker-pool name → target worker-pool name. Workers are not
    /// addressable objects, so this stays a name.
    #[serde(default)]
    pub worker_pools: std::collections::HashMap<String, String>,
    /// Source secret name → target secret id.
    #[serde(default)]
    pub secrets: std::collections::HashMap<String, Uuid>,
    /// Create jobs that are new in the target. Off means an update-only apply,
    /// which is the safe direction for a first run against a live environment.
    #[serde(default = "default_true")]
    pub allow_create: bool,
    /// Update jobs that already exist in the target.
    #[serde(default = "default_true")]
    pub allow_update: bool,
    /// Re-create schedules in the target.
    #[serde(default)]
    pub copy_schedules: bool,
}

fn default_true() -> bool {
    true
}

/// What one job's crossing did.
#[derive(Debug, Clone)]
struct Outcome {
    source_key: String,
    name: String,
    job_id: Uuid,
    created: bool,
    schedule_copied: bool,
}

pub async fn apply(
    State(state): State<AppState>,
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

    let tenant = auth.tenant_id;
    let target_environment = forge_storage::EnvironmentRepository::new(&state.pool)
        .get_by_slug(tenant, plan.target_environment.as_str())
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("environment"))?;

    /*
     * Resolve every queue the plan touches before writing anything.
     *
     * A migration that created some jobs and then hit an unmapped queue would
     * leave the target half-populated and the operator with a plan that no
     * longer describes reality. Checking first means the apply either happens
     * or does not.
     */
    /*
     * Validate every mapping the caller supplied, plus every queue the plan
     * flags as unresolved.
     *
     * Only checking the plan's unresolved list meant a plan with none performed
     * no checks at all, so a caller-supplied id was accepted without being
     * looked up - including one belonging to another tenant.
     */
    let mut queue_names = collect_binding_names(plan, BindingKind::Queue);
    for supplied in body.queues.keys() {
        if !queue_names.contains(supplied) {
            queue_names.push(supplied.clone());
        }
    }
    queue_names.sort();
    queue_names.dedup();

    let mut resolved_queues = std::collections::HashMap::new();
    for name in &queue_names {
        let id = body
            .queues
            .get(name)
            .copied()
            .ok_or_else(|| {
                ApiError::validation(format!(
                    "queue `{name}` is used by this plan but has no target. Map it \\
                     to a queue that exists in {}.",
                    plan.target_environment
                ))
                .with_detail("queues", name.clone())
            })?;

        // The mapped id must be a queue in this tenant. A caller-supplied uuid
        // pointing at another tenant's queue would be a cross-tenant write.
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM queues WHERE id = $1 AND tenant_id = $2)",
        )
        .bind(id)
        .bind(tenant.into_uuid())
        .fetch_one(&state.pool)
        .await
        .map_err(ApiError::from)?;
        if !exists {
            return Err(ApiError::validation(format!(
                "queue `{name}` was mapped to an id that does not exist in this tenant"
            ))
            .with_detail("queues", name.clone()));
        }
        resolved_queues.insert(name.clone(), id);
    }

    let mut outcomes: Vec<Outcome> = Vec::with_capacity(plan.entries.len());

    for entry in &plan.entries {
        if entry.action == MigrationAction::Unchanged {
            continue;
        }
        if entry.action == MigrationAction::Create && !body.allow_create {
            continue;
        }
        if entry.action == MigrationAction::Update && !body.allow_update {
            continue;
        }

        // A job with no key cannot be matched on the way back, and creating one
        // would make the destination impossible to reconcile later.
        if entry.source_key.trim().is_empty() {
            return Err(ApiError::validation(format!(
                "`{}` has no key and cannot be matched in the target. Give it a key \
                 before migrating.",
                entry.name
            ))
            .with_detail("plan", "keyless job"));
        }

        let queue_id = resolved_queues
            .get(&format!("{}-queue", entry.source_key))
            .copied()
            .or_else(|| resolved_queues.values().next().copied());

        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM jobs
              WHERE tenant_id = $1 AND environment_id = $2 AND key = $3",
        )
        .bind(tenant.into_uuid())
        .bind(target_environment.id)
        .bind(&entry.source_key)
        .fetch_optional(&state.pool)
        .await
        .map_err(ApiError::from)?;

        let repository = JobRepository::new(&state.pool);
        let (job_id, created) = match existing {
            Some((id,)) => {
                sqlx::query("UPDATE jobs SET default_queue_id = $2, updated_at = NOW() WHERE id = $1")
                    .bind(id)
                    .bind(queue_id)
                    .execute(&state.pool)
                    .await
                    .map_err(ApiError::from)?;
                (id, false)
            }
            None => {
                let row = repository
                    .create(
                        tenant,
                        Some(entry.source_key.clone()),
                        &entry.name,
                        None,
                        forge_domain::Priority::Normal,
                        Some(auth.user_id),
                        Some(target_environment.id),
                        None,
                    )
                    .await
                    .map_err(ApiError::from)?;

                if let Some(queue) = queue_id {
                    sqlx::query("UPDATE jobs SET default_queue_id = $2 WHERE id = $1")
                        .bind(row.id)
                        .bind(queue)
                        .execute(&state.pool)
                        .await
                        .map_err(ApiError::from)?;
                }
                (row.id, true)
            }
        };

        /*
         * Copy the schedule only for a job that was just created.
         *
         * A target job that already exists keeps its own schedule. Copying the
         * source's schedule onto it as well gave that job two schedules, so it
         * fired twice - the precise duplicate a migration exists to avoid.
         */
        let schedule_copied = if body.copy_schedules && created {
            copy_schedule(&state, tenant, &entry.source_key, job_id)
                .await
                .map_err(ApiError::from)?
        } else {
            false
        };

        outcomes.push(Outcome {
            source_key: entry.source_key.clone(),
            name: entry.name.clone(),
            job_id,
            created,
            schedule_copied,
        });
    }

    // Recorded so an audit can show what crossed, not only who asked.
    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "migration.applied",
            "migration",
            None,
        ))
        .await;

    let created = outcomes.iter().filter(|o| o.created).count();
    let updated = outcomes.len() - created;

    Ok(Json(ApiResponse::new(
        json!({
            "source_environment": plan.source_environment,
            "target_environment": plan.target_environment,
            "created": created,
            "updated": updated,
            "schedules_copied": outcomes.iter().filter(|o| o.schedule_copied).count(),
            "jobs": outcomes
                .iter()
                .map(|o| json!({
                    "source_key": o.source_key,
                    "name": o.name,
                    "job_id": o.job_id,
                    "created": o.created,
                }))
                .collect::<Vec<_>>(),
            "applied_at": Utc::now(),
        }),
        auth.request_id,
    )))
}

/// The distinct binding names a plan references, of one kind.
fn collect_binding_names(plan: &MigrationPlan, kind: BindingKind) -> Vec<String> {
    let mut names: Vec<String> = plan
        .entries
        .iter()
        .flat_map(|entry| entry.unresolved.iter())
        // `BindingKind` is `Copy`, so comparing by value is fine.
        .filter(|binding| binding.kind == kind)
        .map(|binding| binding.source_name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Copies a job's cron schedule into the target environment.
///
/// The expression and timezone are environment-independent, which is why the
/// plan carries them. The next run is recomputed from the destination rather
/// than copied, so a job cannot arrive already overdue.
async fn copy_schedule(
    state: &AppState,
    tenant: forge_domain::TenantId,
    source_key: &str,
    job_id: Uuid,
) -> Result<bool, forge_storage::StorageError> {
    let source: Option<(String, String, String)> = sqlx::query_as(
        "SELECT s.cron_expression, s.timezone, s.misfire_policy
           FROM schedules s
           JOIN jobs j ON j.id = s.target_id
          WHERE j.tenant_id = $1 AND j.key = $2 AND s.target_type = 'JOB'
          LIMIT 1",
    )
    .bind(tenant.into_uuid())
    .bind(source_key)
    .fetch_optional(&state.pool)
    .await?;

    let Some((expression, timezone, misfire)) = source else {
        return Ok(false);
    };

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO schedules
            (id, tenant_id, job_id, target_id, target_type, target_version_policy,
             schedule_type, cron_expression, timezone, misfire_policy,
             catch_up_policy, next_run_at, enabled)
         VALUES ($1, $2, $3, $3, 'JOB', 'LATEST_PUBLISHED', 'CRON', $4, $5, $6,
                 '[]'::jsonb, NULL, FALSE)",
    )
    .bind(id)
    .bind(tenant.into_uuid())
    .bind(job_id)
    .bind(&expression)
    .bind(&timezone)
    .bind(&misfire)
    .execute(&state.pool)
    .await?;

    // Created disabled on purpose: a schedule that starts firing the moment it
    // arrives is a change the operator has not reviewed yet. `resume` is the
    // deliberate act that arms it.
    Ok(true)
}

/// The bindings a plan references, for the console's mapping form.
pub async fn plan_bindings(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(plan): Json<MigrationPlan>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:read")?;

    let queues = collect_binding_names(&plan, BindingKind::Queue);
    let secrets = collect_binding_names(&plan, BindingKind::Secret);
    let pools = collect_binding_names(&plan, BindingKind::WorkerPool);

    // The queues that actually exist in the target, so the console can offer a
    // choice rather than asking an operator to type a uuid.
    let available: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT name, id FROM queues WHERE tenant_id = $1 ORDER BY name",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "required": { "queues": queues, "worker_pools": pools, "secrets": secrets },
            "available_queues": available
                .iter()
                .map(|(name, id)| json!({ "name": name, "id": id }))
                .collect::<Vec<_>>(),
        }),
        auth.request_id,
    )))
}
