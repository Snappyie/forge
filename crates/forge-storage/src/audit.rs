use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::{Result, StorageError};
use forge_domain::TenantId;

/// Audit event row (spec 01.17).
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct AuditEventRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub result: String,
    pub source_ip: Option<String>,
    pub request_id: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// A security-relevant action to record.
#[derive(Debug, Clone)]
pub struct NewAuditEvent {
    pub tenant_id: Option<TenantId>,
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    /// `SUCCESS`, `DENIED` or `FAILURE`.
    pub result: String,
    pub source_ip: Option<String>,
    pub request_id: Option<String>,
    /// Must never contain secret material (spec 01.17, invariant 11).
    pub metadata: serde_json::Value,
}

impl NewAuditEvent {
    pub fn new(
        tenant_id: TenantId,
        actor_type: &str,
        actor_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
    ) -> Self {
        Self {
            tenant_id: Some(tenant_id),
            actor_type: actor_type.to_string(),
            actor_id,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            result: "SUCCESS".to_string(),
            source_ip: None,
            request_id: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn denied(mut self) -> Self {
        self.result = "DENIED".to_string();
        self
    }

    pub fn with_request_id(mut self, request_id: Option<String>) -> Self {
        self.request_id = request_id;
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

const AUDIT_COLUMNS: &str = "id, tenant_id, actor_type, actor_id, action, resource_type, \
     resource_id, result, source_ip, request_id, metadata, created_at";

/// Append-only audit log.
///
/// Spec 01.22 invariant 10: audit records are append-only from the
/// application's perspective. This repository deliberately exposes no update or
/// delete; retention is handled by a separate batched cleanup that is itself
/// auditable (spec 08.7).
pub struct AuditRepository<'a> {
    pool: &'a PgPool,
}

/// Filters for `list_audit_events` (spec 05 endpoint 52).
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub actor_id: Option<Uuid>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
}

impl<'a> AuditRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn record(&self, event: NewAuditEvent) -> Result<AuditEventRow> {
        // `TenantId` is a UUID newtype; unwrap its inner value rather than
        // re-parsing the string form.
        let tenant_uuid = event.tenant_id.map(|t| t.into_uuid());
        // An unparseable source IP is dropped rather than failing the audit
        // write: the event matters more than the address on it.
        let source_ip = event
            .source_ip
            .as_deref()
            .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
            .map(|ip| ip.to_string());

        sqlx::query_as::<_, AuditEventRow>(&format!(
            "INSERT INTO audit_events
                 (id, tenant_id, actor_type, actor_id, action, resource_type, resource_id,
                  result, source_ip, request_id, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::inet, $10, $11)
             RETURNING {AUDIT_COLUMNS}"
        ))
        .bind(Uuid::new_v4())
        .bind(tenant_uuid)
        .bind(&event.actor_type)
        .bind(event.actor_id)
        .bind(&event.action)
        .bind(&event.resource_type)
        .bind(event.resource_id)
        .bind(&event.result)
        .bind(source_ip)
        .bind(&event.request_id)
        .bind(&event.metadata)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Lists audit events for a tenant (spec 05 endpoint 52).
    pub async fn list(
        &self,
        tenant_id: TenantId,
        filter: &AuditFilter,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<crate::error::Page<AuditEventRow>> {
        let decoded = match cursor {
            Some(token) => Some(crate::error::Cursor::decode(token)?),
            None => None,
        };

        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {AUDIT_COLUMNS} FROM audit_events WHERE tenant_id = "
        ));
        qb.push_bind(tenant_id.into_uuid());
        if let Some(actor) = filter.actor_id {
            qb.push(" AND actor_id = ").push_bind(actor);
        }
        if let Some(action) = &filter.action {
            qb.push(" AND action = ").push_bind(action);
        }
        if let Some(resource) = &filter.resource_type {
            qb.push(" AND resource_type = ").push_bind(resource);
        }
        if let Some(after) = filter.created_after {
            qb.push(" AND created_at >= ").push_bind(after);
        }
        if let Some(before) = filter.created_before {
            qb.push(" AND created_at < ").push_bind(before);
        }
        if let Some(c) = decoded.as_ref() {
            qb.push(" AND (created_at, id) < (")
                .push_bind(&c.sort_value)
                .push("::timestamptz, ")
                .push_bind(c.last_id)
                .push(")");
        }
        qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind((limit + 1) as i64);

        let rows = qb
            .build_query_as::<AuditEventRow>()
            .fetch_all(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?;

        Ok(crate::error::Page::from_overfetch(rows, limit, |row| {
            crate::error::Cursor::new(row.created_at.to_rfc3339(), row.id)
        }))
    }

    /// Batched retention cleanup (spec 08.7). `LIMIT` keeps each pass short so
    /// the table is never locked for long.
    pub async fn delete_before(&self, cutoff: DateTime<Utc>, batch: i64) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM audit_events WHERE id IN (
                 SELECT id FROM audit_events WHERE created_at < $1 LIMIT $2
             )",
        )
        .bind(cutoff)
        .bind(batch)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(result.rows_affected())
    }
}

/// Outcome of reserving an idempotency key.
#[derive(Debug, Clone)]
pub enum IdempotencyOutcome {
    /// The key is new; the caller should perform the operation and store the
    /// response via `complete`.
    Fresh,
    /// The same key was replayed with the same request; `body` is the stored
    /// response.
    Replay { status: i32, body: serde_json::Value },
    /// The same key was reused with a different request (spec 02.15).
    Conflict,
}

pub struct IdempotencyRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> IdempotencyRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Reserves a key, returning whether the operation should run.
    ///
    /// Spec 08.4 requires the idempotency record and the operation reservation
    /// to commit together, so this INSERT is the reservation point: the unique
    /// constraint on `(tenant_id, endpoint, idempotency_key)` means a
    /// concurrent duplicate cannot both win.
    pub async fn reserve(
        &self,
        tenant_id: TenantId,
        key: &str,
        endpoint: &str,
        fingerprint: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<IdempotencyOutcome> {
        let inserted = sqlx::query(
            "INSERT INTO idempotency_keys
                 (id, tenant_id, idempotency_key, endpoint, request_fingerprint, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (tenant_id, endpoint, idempotency_key) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id.into_uuid())
        .bind(key)
        .bind(endpoint)
        .bind(fingerprint)
        .bind(expires_at)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;

        if inserted.rows_affected() == 1 {
            return Ok(IdempotencyOutcome::Fresh);
        }

        // The key already exists: replay or conflict depends on whether the
        // request is semantically identical.
        let existing: Option<(String, Option<i32>, Option<serde_json::Value>)> = sqlx::query_as(
            "SELECT request_fingerprint, response_status, response_body
             FROM idempotency_keys
             WHERE tenant_id = $1 AND endpoint = $2 AND idempotency_key = $3",
        )
        .bind(tenant_id.into_uuid())
        .bind(endpoint)
        .bind(key)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;

        match existing {
            None => Ok(IdempotencyOutcome::Fresh),
            Some((stored_fingerprint, status, body)) => {
                if stored_fingerprint != fingerprint {
                    return Ok(IdempotencyOutcome::Conflict);
                }
                match (status, body) {
                    (Some(status), Some(body)) => Ok(IdempotencyOutcome::Replay { status, body }),
                    // The original request is still in flight; treating this as
                    // a conflict avoids handing out a half-finished result.
                    _ => Ok(IdempotencyOutcome::Conflict),
                }
            }
        }
    }

    /// Stores the response for a reserved key so a later replay returns it.
    pub async fn complete(
        &self,
        tenant_id: TenantId,
        key: &str,
        endpoint: &str,
        status: i32,
        body: &serde_json::Value,
        resource_id: Option<Uuid>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE idempotency_keys
             SET response_status = $4, response_body = $5, resource_id = $6
             WHERE tenant_id = $1 AND endpoint = $2 AND idempotency_key = $3",
        )
        .bind(tenant_id.into_uuid())
        .bind(endpoint)
        .bind(key)
        .bind(status)
        .bind(body)
        .bind(resource_id)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(())
    }

    /// Batched expiry cleanup (spec 02.15 retention).
    pub async fn purge_expired(&self, batch: i64) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM idempotency_keys WHERE id IN (
                 SELECT id FROM idempotency_keys WHERE expires_at < NOW() LIMIT $1
             )",
        )
        .bind(batch)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(result.rows_affected())
    }
}

/// Outbox event (spec 04.8).
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct OutboxEventRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub attempt_count: i32,
    pub last_error: Option<String>,
}

pub struct OutboxRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> OutboxRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Enqueues an event. Callers write this in the same transaction as the
    /// state change it describes, so an event cannot be lost by a crash
    /// (spec 04.8).
    pub async fn enqueue(
        &self,
        tenant_id: Option<TenantId>,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        payload: serde_json::Value,
    ) -> Result<OutboxEventRow> {
        sqlx::query_as::<_, OutboxEventRow>(
            "INSERT INTO outbox_events
                 (id, tenant_id, event_type, aggregate_type, aggregate_id, payload)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id, tenant_id, event_type, aggregate_type, aggregate_id, payload,
                       created_at, published_at, attempt_count, last_error",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id.map(|t| t.into_uuid()))
        .bind(event_type)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(payload)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Claims unpublished events for the publisher.
    ///
    /// `SKIP LOCKED` lets several publishers run concurrently without
    /// delivering the same event twice (AT-REC-005).
    pub async fn claim_unpublished(&self, batch: i64) -> Result<Vec<OutboxEventRow>> {
        sqlx::query_as::<_, OutboxEventRow>(
            "UPDATE outbox_events SET attempt_count = attempt_count + 1
             WHERE id IN (
                 SELECT id FROM outbox_events
                 WHERE published_at IS NULL
                 ORDER BY created_at ASC
                 FOR UPDATE SKIP LOCKED
                 LIMIT $1
             )
             RETURNING id, tenant_id, event_type, aggregate_type, aggregate_id, payload,
                       created_at, published_at, attempt_count, last_error",
        )
        .bind(batch)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    pub async fn mark_published(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE outbox_events SET published_at = NOW(), last_error = NULL WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?;
        Ok(())
    }

    pub async fn mark_failed(&self, id: Uuid, error: &str) -> Result<()> {
        sqlx::query("UPDATE outbox_events SET last_error = $2 WHERE id = $1")
            .bind(id)
            .bind(error)
            .execute(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?;
        Ok(())
    }
}