//! Domain events and the outbox publisher (spec 04.8, spec 06).
//!
//! Events are written to the outbox **in the same transaction** as the state
//! change they describe. A crash between the two would otherwise either lose
//! the event or announce a change that never happened.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

use forge_domain::{ErrorClass, ExecutionStatus, TenantId, TriggerSource};

/// A domain event, as written to the outbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainEvent {
    pub id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
    pub payload: serde_json::Value,
    /// Ties this event to the request or execution that caused it
    /// (spec 01.6, AT-OBS-001).
    pub correlation_id: Option<String>,
}

impl DomainEvent {
    pub fn new(
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        tenant_id: Option<TenantId>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type: event_type.to_string(),
            aggregate_type: aggregate_type.to_string(),
            aggregate_id,
            tenant_id: tenant_id.map(|t| t.into_uuid()),
            occurred_at: Utc::now(),
            payload: serde_json::json!({}),
            correlation_id: None,
        }
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: Option<String>) -> Self {
        self.correlation_id = correlation_id;
        self
    }

    /// Wire form, using the `cloudevents`-style envelope field names.
    pub fn to_envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "type": self.event_type,
            "source": format!("forge/{}", self.aggregate_type),
            "subject": self.aggregate_id.to_string(),
            "time": self.occurred_at.to_rfc3339(),
            "tenant_id": self.tenant_id,
            "correlation_id": self.correlation_id,
            "data": self.payload,
        })
    }
}

/// Event type names, kept in one place so producers and consumers agree.
pub mod event_type {
    pub const EXECUTION_CREATED: &str = "execution.created";
    pub const EXECUTION_STARTED: &str = "execution.started";
    pub const EXECUTION_SUCCEEDED: &str = "execution.succeeded";
    pub const EXECUTION_FAILED: &str = "execution.failed";
    pub const EXECUTION_ABANDONED: &str = "execution.abandoned";
    pub const EXECUTION_CANCELLED: &str = "execution.cancelled";
    pub const EXECUTION_DEAD_LETTERED: &str = "execution.dead_lettered";
    pub const WORKER_REGISTERED: &str = "worker.registered";
    pub const WORKER_OFFLINE: &str = "worker.offline";
    pub const WORKER_REVOKED: &str = "worker.revoked";
    pub const SCHEDULE_FIRED: &str = "schedule.fired";
    pub const SCHEDULE_MISFIRED: &str = "schedule.misfired";
    pub const LEASE_EXPIRED: &str = "lease.expired";
}

/// Builds the event for an execution reaching a state.
pub fn execution_event(
    tenant_id: TenantId,
    execution_id: Uuid,
    status: ExecutionStatus,
    attempt: u32,
    error_class: Option<ErrorClass>,
    correlation_id: Option<String>,
) -> DomainEvent {
    let event_type = match status {
        ExecutionStatus::Queued | ExecutionStatus::Scheduled => event_type::EXECUTION_CREATED,
        ExecutionStatus::Running => event_type::EXECUTION_STARTED,
        ExecutionStatus::Succeeded => event_type::EXECUTION_SUCCEEDED,
        ExecutionStatus::Failed => event_type::EXECUTION_FAILED,
        ExecutionStatus::Abandoned => event_type::EXECUTION_ABANDONED,
        ExecutionStatus::Cancelled | ExecutionStatus::CancelRequested => {
            event_type::EXECUTION_CANCELLED
        }
        ExecutionStatus::DeadLettered => event_type::EXECUTION_DEAD_LETTERED,
        // Dispatched, RetryScheduled and TimedOut are internal progress, not
        // externally meaningful state changes.
        ExecutionStatus::Dispatched | ExecutionStatus::RetryScheduled | ExecutionStatus::TimedOut => {
            event_type::EXECUTION_STARTED
        }
    };

    DomainEvent::new(event_type, "execution", execution_id, Some(tenant_id))
        .with_payload(serde_json::json!({
            "status": status.as_str(),
            "attempt": attempt,
            // AT-OBS-004: a failure carries a safe classification, never a
            // raw internal error string.
            "error_class": error_class.map(|c| c.as_str()),
        }))
        .with_correlation_id(correlation_id)
}

/// Builds the event for a manual trigger, distinguishing it from a scheduled
/// run (spec 09.10).
pub fn trigger_event(
    tenant_id: TenantId,
    execution_id: Uuid,
    source: TriggerSource,
    actor_id: Option<Uuid>,
) -> DomainEvent {
    DomainEvent::new(
        event_type::EXECUTION_CREATED,
        "execution",
        execution_id,
        Some(tenant_id),
    )
    .with_payload(serde_json::json!({
        "trigger_source": source.as_str(),
        "actor_id": actor_id,
    }))
}

/// Publishes outbox rows to a sink.
///
/// AT-REC-005: a publisher that restarts must not lose events, so claims are
/// released back to the queue when publication fails rather than being marked
/// published optimistically.
#[async_trait::async_trait]
pub trait EventSink: Send + Sync {
    /// Delivers one event. Returning an error causes a retry.
    async fn publish(&self, event: &DomainEvent) -> Result<(), SinkError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("transient transport failure: {0}")]
    Transient(String),
    #[error("permanent delivery failure: {0}")]
    Permanent(String),
}

impl SinkError {
    /// Whether the event should be retried.
    pub fn is_retryable(&self) -> bool {
        matches!(self, SinkError::Transient(_))
    }
}

/// A sink that records events in memory, for tests and local development.
#[derive(Debug, Default)]
pub struct InMemorySink {
    events: std::sync::Mutex<Vec<DomainEvent>>,
    /// When set, every publish fails with this error.
    fail_with: std::sync::Mutex<Option<SinkError>>,
}

impl InMemorySink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn published(&self) -> Vec<DomainEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.events.lock().map(|e| e.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Makes subsequent publishes fail, to exercise the retry path.
    pub fn fail_with(&self, error: Option<SinkError>) {
        if let Ok(mut slot) = self.fail_with.lock() {
            *slot = error;
        }
    }
}

#[async_trait::async_trait]
impl EventSink for InMemorySink {
    async fn publish(&self, event: &DomainEvent) -> Result<(), SinkError> {
        if let Ok(slot) = self.fail_with.lock() {
            if let Some(err) = slot.as_ref() {
                return Err(SinkError::Transient(err.to_string()));
            }
        }
        if let Ok(mut events) = self.events.lock() {
            events.push(event.clone());
        }
        Ok(())
    }
}

/// Reads outbox rows and hands them to a sink.
pub struct OutboxPublisher<'a> {
    pool: &'a sqlx::PgPool,
    batch_size: i64,
    /// Attempts before an event is considered undeliverable.
    max_attempts: i32,
}

impl<'a> OutboxPublisher<'a> {
    pub fn new(pool: &'a sqlx::PgPool, batch_size: i64) -> Self {
        Self {
            pool,
            batch_size,
            max_attempts: 10,
        }
    }

    pub fn with_max_attempts(mut self, max_attempts: i32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// Publishes one batch.
    ///
    /// Each row is marked published only after the sink confirms it, so a crash
    /// mid-batch leaves the un-published rows claimable by the next run.
    pub async fn publish_batch(&self, sink: &dyn EventSink) -> PublishStats {
        let outbox = forge_storage::OutboxRepository::new(self.pool);

        let rows = match outbox.claim_unpublished(self.batch_size).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "outbox publisher failed to claim events");
                return PublishStats::default();
            }
        };

        let mut stats = PublishStats {
            claimed: rows.len() as u64,
            ..Default::default()
        };

        for row in rows {
            let event = DomainEvent {
                id: row.id,
                event_type: row.event_type.clone(),
                aggregate_type: row.aggregate_type.clone(),
                aggregate_id: row.aggregate_id,
                tenant_id: row.tenant_id,
                occurred_at: row.created_at,
                payload: row.payload.clone(),
                correlation_id: row
                    .payload
                    .get("correlation_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            };

            // A permanently failing event is abandoned rather than retried
            // forever, so it cannot block the queue indefinitely.
            if row.attempt_count >= self.max_attempts {
                let _ = outbox
                    .mark_failed(row.id, "max publish attempts exceeded")
                    .await;
                stats.abandoned += 1;
                continue;
            }

            match sink.publish(&event).await {
                Ok(()) => {
                    if outbox.mark_published(row.id).await.is_err() {
                        stats.failed += 1;
                    } else {
                        stats.published += 1;
                    }
                }
                Err(e) => {
                    let _ = outbox.mark_failed(row.id, &e.to_string()).await;
                    if e.is_retryable() {
                        stats.failed += 1;
                    } else {
                        stats.abandoned += 1;
                    }
                    stats.last_error = Some(e.to_string());
                }
            }
        }

        if stats.published > 0 || stats.failed > 0 {
            tracing::info!(
                claimed = stats.claimed,
                published = stats.published,
                failed = stats.failed,
                abandoned = stats.abandoned,
                "outbox publish batch complete"
            );
        }

        stats
    }

    /// Runs the publisher on an interval until `shutdown` resolves.
    pub async fn run(
        &self,
        sink: Arc<dyn EventSink>,
        interval: std::time::Duration,
        shutdown: impl std::future::Future<Output = ()> + Send,
    ) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.publish_batch(sink.as_ref()).await;
                }
                _ = &mut shutdown => {
                    tracing::info!("outbox publisher stopping");
                    return;
                }
            }
        }
    }
}

/// What one publish pass did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PublishStats {
    pub claimed: u64,
    pub published: u64,
    pub failed: u64,
    /// Events given up on after exhausting their attempts.
    pub abandoned: u64,
    pub last_error: Option<String>,
}

/// A sink that logs events, for running without an external broker.
#[derive(Debug, Default)]
pub struct LogSink;

#[async_trait::async_trait]
impl EventSink for LogSink {
    async fn publish(&self, event: &DomainEvent) -> Result<(), SinkError> {
        tracing::info!(
            event_type = %event.event_type,
            aggregate_id = %event.aggregate_id,
            correlation_id = ?event.correlation_id,
            "domain event"
        );
        Ok(())
    }
}

/// Metadata attached to an event for routing and filtering.
pub type EventMetadata = BTreeMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;

    fn tenant() -> TenantId {
        TenantId::from_uuid(Uuid::new_v4())
    }

    #[test]
    fn events_carry_a_typed_envelope() {
        let event = DomainEvent::new(
            event_type::EXECUTION_SUCCEEDED,
            "execution",
            Uuid::new_v4(),
            Some(tenant()),
        )
        .with_payload(serde_json::json!({"attempt": 1}))
        .with_correlation_id(Some("corr-1".into()));

        let envelope = event.to_envelope();
        assert_eq!(envelope["type"], "execution.succeeded");
        assert_eq!(envelope["data"]["attempt"], 1);
        assert_eq!(envelope["correlation_id"], "corr-1");
    }

    /// AT-OBS-001: an execution carries a correlation id.
    #[test]
    fn execution_events_propagate_the_correlation_id() {
        let event = execution_event(
            tenant(),
            Uuid::new_v4(),
            ExecutionStatus::Succeeded,
            2,
            None,
            Some("corr-42".into()),
        );
        assert_eq!(event.correlation_id.as_deref(), Some("corr-42"));
        assert_eq!(event.event_type, "execution.succeeded");
        assert_eq!(event.payload["attempt"], 2);
    }

    /// AT-OBS-004: a failure carries a safe classification.
    #[test]
    fn failure_events_carry_a_class_not_a_raw_message() {
        let event = execution_event(
            tenant(),
            Uuid::new_v4(),
            ExecutionStatus::Failed,
            1,
            Some(ErrorClass::Timeout),
            None,
        );
        assert_eq!(event.payload["error_class"], "TIMEOUT");
        assert!(
            !event.to_envelope().to_string().contains("stack"),
            "no internal detail may leak into the event"
        );
    }

    #[test]
    fn every_execution_status_maps_to_an_event_type() {
        let statuses = [
            ExecutionStatus::Scheduled,
            ExecutionStatus::Queued,
            ExecutionStatus::Dispatched,
            ExecutionStatus::Running,
            ExecutionStatus::Succeeded,
            ExecutionStatus::Failed,
            ExecutionStatus::TimedOut,
            ExecutionStatus::CancelRequested,
            ExecutionStatus::Cancelled,
            ExecutionStatus::RetryScheduled,
            ExecutionStatus::DeadLettered,
            ExecutionStatus::Abandoned,
        ];
        for status in statuses {
            let event = execution_event(tenant(), Uuid::new_v4(), status, 1, None, None);
            assert!(
                !event.event_type.is_empty() && event.event_type.starts_with("execution."),
                "{status} produced `{}`",
                event.event_type
            );
        }
    }

    #[test]
    fn a_manual_trigger_is_distinguishable_from_a_scheduled_one() {
        let manual = trigger_event(
            tenant(),
            Uuid::new_v4(),
            TriggerSource::Manual,
            Some(Uuid::new_v4()),
        );
        assert_eq!(manual.payload["trigger_source"], "MANUAL");

        let scheduled = trigger_event(
            tenant(),
            Uuid::new_v4(),
            TriggerSource::Schedule,
            None,
        );
        assert_eq!(scheduled.payload["trigger_source"], "SCHEDULE");
    }

    #[tokio::test]
    async fn the_in_memory_sink_records_published_events() {
        let sink = InMemorySink::new();
        assert!(sink.is_empty());

        let event = DomainEvent::new("test.event", "execution", Uuid::new_v4(), None);
        sink.publish(&event).await.unwrap();
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.published()[0].event_type, "test.event");
    }

    #[tokio::test]
    async fn a_failing_sink_reports_a_retryable_error() {
        let sink = InMemorySink::new();
        sink.fail_with(Some(SinkError::Transient("broker down".into())));

        let result = sink
            .publish(&DomainEvent::new("e", "a", Uuid::new_v4(), None))
            .await;
        let err = result.unwrap_err();
        assert!(err.is_retryable());
        assert!(sink.is_empty(), "nothing is recorded when delivery fails");
    }

    #[tokio::test]
    async fn a_permanent_failure_is_not_retried() {
        let err = SinkError::Permanent("bad schema".into());
        assert!(!err.is_retryable());
    }

    #[test]
    fn publish_stats_start_at_zero() {
        let stats = PublishStats::default();
        assert_eq!(stats.claimed, 0);
        assert_eq!(stats.published, 0);
        assert!(stats.last_error.is_none());
    }
}