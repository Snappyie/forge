use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use forge_domain::{ExecutionId, JobId, TenantId, WorkerId};

/// Worker lifecycle states (spec 02.8).
///
/// The pre-existing `Online`/`Draining`/`Offline` triple is not enough: a
/// worker must be distinguishable while it is still starting up, while it is
/// busy, and after an operator has revoked it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkerStatus {
    /// Handshake in progress; not yet eligible for work.
    #[default]
    Registering,
    /// Idle and eligible for work.
    Ready,
    /// Holding at least one execution.
    Busy,
    /// Finishing held work, accepting nothing new.
    Draining,
    /// Not connected.
    Offline,
    /// Revoked by an operator; must never receive work again.
    Revoked,
}

impl WorkerStatus {
    /// Whether the worker may be handed a new execution (spec 10.10).
    pub fn accepts_work(&self) -> bool {
        matches!(self, WorkerStatus::Ready | WorkerStatus::Busy)
    }

    /// Whether the worker's reported status is trustworthy.
    pub fn is_healthy(&self) -> bool {
        !matches!(self, WorkerStatus::Offline | WorkerStatus::Revoked)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            WorkerStatus::Registering => "REGISTERING",
            WorkerStatus::Ready => "READY",
            WorkerStatus::Busy => "BUSY",
            WorkerStatus::Draining => "DRAINING",
            WorkerStatus::Offline => "OFFLINE",
            WorkerStatus::Revoked => "REVOKED",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "REGISTERING" => Some(WorkerStatus::Registering),
            "READY" => Some(WorkerStatus::Ready),
            "BUSY" => Some(WorkerStatus::Busy),
            "DRAINING" => Some(WorkerStatus::Draining),
            "OFFLINE" => Some(WorkerStatus::Offline),
            "REVOKED" => Some(WorkerStatus::Revoked),
            _ => None,
        }
    }
}

impl std::fmt::Display for WorkerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub id: WorkerId,
    pub tenant_id: TenantId,
    pub name: String,
    pub hostname: String,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    pub labels: std::collections::BTreeMap<String, String>,
    pub status: WorkerStatus,
    pub last_heartbeat_at: DateTime<Utc>,
    pub registered_at: DateTime<Utc>,
}

impl Worker {
    pub fn new(tenant_id: TenantId, hostname: String, capabilities: Vec<String>) -> Self {
        let now = Utc::now();
        Self {
            id: WorkerId::new(),
            tenant_id,
            name: hostname.clone(),
            hostname,
            version: None,
            capabilities,
            labels: Default::default(),
            status: WorkerStatus::Ready,
            last_heartbeat_at: now,
            registered_at: now,
        }
    }

    pub fn heartbeat(&mut self) {
        self.last_heartbeat_at = Utc::now();
    }

    /// Whether a job's required capabilities are satisfied (spec 10.13).
    ///
    /// A job with no requirements matches any worker. Otherwise every required
    /// capability must be present.
    pub fn satisfies(&self, required: &[String]) -> bool {
        required.iter().all(|r| self.capabilities.contains(r))
    }

    /// Whether the heartbeat is recent enough to be considered alive.
    pub fn is_alive_at(&self, now: DateTime<Utc>, stale_after_secs: i64) -> bool {
        (now - self.last_heartbeat_at).num_seconds() < stale_after_secs
    }
}

/// An exclusive claim on an execution (spec 02.9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub id: uuid::Uuid,
    pub execution_id: ExecutionId,
    pub worker_id: WorkerId,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub renewed_at: Option<DateTime<Utc>>,
    pub released_at: Option<DateTime<Utc>>,
}

impl Lease {
    pub fn new(execution_id: ExecutionId, worker_id: WorkerId, duration_secs: i64) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4(),
            execution_id,
            worker_id,
            acquired_at: now,
            expires_at: now + chrono::Duration::seconds(duration_secs),
            renewed_at: Some(now),
            released_at: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.is_expired_at(Utc::now())
    }

    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    pub fn renew(&mut self, additional_secs: i64) {
        let now = Utc::now();
        self.expires_at = now + chrono::Duration::seconds(additional_secs);
        self.renewed_at = Some(now);
    }

    /// Whether `worker_id` is still entitled to act on this lease.
    pub fn is_held_by(&self, worker_id: WorkerId) -> bool {
        self.worker_id == worker_id && self.released_at.is_none() && !self.is_expired()
    }
}

/// Lease policy: spec 10.2 requires the lease to exceed the heartbeat interval
/// by a documented safety factor. The defaults are a 5s heartbeat against a 20s
/// lease — a factor of four, so two missed heartbeats do not orphan work.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LeasePolicy {
    pub heartbeat_interval_secs: i64,
    pub lease_duration_secs: i64,
}

impl Default for LeasePolicy {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: 5,
            lease_duration_secs: 20,
        }
    }
}

impl LeasePolicy {
    /// The documented safety factor.
    pub fn safety_factor(&self) -> f64 {
        self.lease_duration_secs as f64 / self.heartbeat_interval_secs as f64
    }

    /// Rejects a lease shorter than the heartbeat, which would orphan work on
    /// every missed beat.
    pub fn validate(&self) -> Result<(), forge_domain::DomainError> {
        if self.heartbeat_interval_secs <= 0 {
            return Err(forge_domain::DomainError::ValidationError(
                "heartbeat interval must be positive".to_string(),
            ));
        }
        if self.lease_duration_secs <= self.heartbeat_interval_secs {
            return Err(forge_domain::DomainError::ValidationError(format!(
                "lease duration ({}) must exceed the heartbeat interval ({})",
                self.lease_duration_secs, self.heartbeat_interval_secs
            )));
        }
        Ok(())
    }
}

/// The result a worker reports when a task finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionReport {
    pub execution_id: ExecutionId,
    pub attempt_id: Option<uuid::Uuid>,
    /// The lease the work ran under. A completion carrying a stale lease must
    /// be rejected (spec 10.5).
    pub lease_id: uuid::Uuid,
    pub worker_id: WorkerId,
    pub outcome: TaskOutcome,
    pub duration_ms: i64,
    pub output: serde_json::Value,
}

/// How a task finished.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskOutcome {
    Succeeded {
        exit_code: i32,
    },
    Failed {
        class: forge_domain::ErrorClass,
        message: Option<String>,
        exit_code: Option<i32>,
    },
    TimedOut,
    /// The worker observed a cancellation request and stopped.
    Cancelled,
}

/// Work handed to a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignment {
    pub execution_id: ExecutionId,
    pub lease_id: uuid::Uuid,
    pub job_id: JobId,
    pub execution_type: forge_domain::ExecutionType,
    pub input: serde_json::Value,
    pub timeout_secs: i64,
    pub required_capabilities: Vec<String>,
    pub attempt_number: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    // AT-WKR-004: a draining worker receives no new work.
    #[test]
    fn draining_and_revoked_workers_do_not_accept_work() {
        assert!(WorkerStatus::Ready.accepts_work());
        assert!(WorkerStatus::Busy.accepts_work());
        assert!(!WorkerStatus::Draining.accepts_work());
        assert!(!WorkerStatus::Revoked.accepts_work());
        assert!(!WorkerStatus::Offline.accepts_work());
        assert!(!WorkerStatus::Registering.accepts_work());
    }

    // AT-WKR-005: a revoked worker cannot receive work.
    #[test]
    fn revoked_workers_are_not_healthy() {
        assert!(WorkerStatus::Ready.is_healthy());
        assert!(WorkerStatus::Busy.is_healthy());
        assert!(!WorkerStatus::Revoked.is_healthy());
        assert!(!WorkerStatus::Offline.is_healthy());
    }

    #[test]
    fn worker_status_round_trips_through_its_string_form() {
        for status in [
            WorkerStatus::Registering,
            WorkerStatus::Ready,
            WorkerStatus::Busy,
            WorkerStatus::Draining,
            WorkerStatus::Offline,
            WorkerStatus::Revoked,
        ] {
            assert_eq!(WorkerStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(WorkerStatus::parse("NOPE"), None);
    }

    /// Spec 10.13: capability matching.
    #[test]
    fn capability_matching_requires_every_needed_capability() {
        let mut worker = Worker::new(
            TenantId::new(),
            "h".into(),
            vec!["linux".into(), "docker".into(), "arm64".into()],
        );

        assert!(worker.satisfies(&[]), "no requirement matches anything");
        assert!(worker.satisfies(&["linux".to_string()]));
        assert!(worker.satisfies(&["linux".to_string(), "arm64".to_string()]));
        assert!(
            !worker.satisfies(&["windows".to_string()]),
            "an unmet capability must not match"
        );
        assert!(
            !worker.satisfies(&["linux".to_string(), "gpu".to_string()]),
            "every requirement must be met"
        );

        worker.capabilities.clear();
        assert!(!worker.satisfies(&["linux".to_string()]));
    }

    #[test]
    fn lease_expiry_is_evaluated_against_an_explicit_instant() {
        let lease = Lease::new(ExecutionId::new(), WorkerId::new(), 20);
        assert!(!lease.is_expired_at(lease.expires_at - chrono::Duration::seconds(1)));
        assert!(lease.is_expired_at(lease.expires_at));
        assert!(lease.is_expired_at(lease.expires_at + chrono::Duration::seconds(1)));
    }

    #[test]
    fn lease_renewal_extends_and_records_the_renewal() {
        let mut lease = Lease::new(ExecutionId::new(), WorkerId::new(), 20);
        let before = lease.expires_at;
        lease.renew(60);
        assert!(lease.expires_at > before);
        assert!(lease.renewed_at.is_some());
    }

    /// Invariant 8: only the lease holder may act on it.
    #[test]
    fn lease_ownership_is_checked() {
        let holder = WorkerId::new();
        let other = WorkerId::new();
        let lease = Lease::new(ExecutionId::new(), holder, 20);

        assert!(lease.is_held_by(holder));
        assert!(!lease.is_held_by(other));

        let mut released = lease.clone();
        released.released_at = Some(Utc::now());
        assert!(!released.is_held_by(holder), "a released lease is not held");

        let mut expired = lease;
        expired.expires_at = Utc::now() - chrono::Duration::seconds(1);
        assert!(!expired.is_held_by(holder), "an expired lease is not held");
    }

    /// Spec 10.2: the lease must exceed the heartbeat by a safety factor.
    #[test]
    fn lease_policy_defaults_are_documented_and_valid() {
        let policy = LeasePolicy::default();
        assert_eq!(policy.heartbeat_interval_secs, 5);
        assert_eq!(policy.lease_duration_secs, 20);
        assert_eq!(policy.safety_factor(), 4.0);
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn a_lease_shorter_than_the_heartbeat_is_rejected() {
        // Such a lease would orphan work on every missed beat.
        let bad = LeasePolicy {
            heartbeat_interval_secs: 10,
            lease_duration_secs: 5,
        };
        assert!(bad.validate().is_err());

        let equal = LeasePolicy {
            heartbeat_interval_secs: 10,
            lease_duration_secs: 10,
        };
        assert!(equal.validate().is_err());
    }

    #[test]
    fn worker_liveness_uses_an_explicit_instant() {
        let now = Utc::now();
        let mut worker = Worker::new(TenantId::new(), "h".into(), vec![]);
        worker.last_heartbeat_at = now;

        assert!(worker.is_alive_at(now + chrono::Duration::seconds(5), 60));
        assert!(!worker.is_alive_at(now + chrono::Duration::seconds(120), 60));
    }

    #[test]
    fn completion_report_carries_the_lease_it_ran_under() {
        let report = CompletionReport {
            execution_id: ExecutionId::new(),
            attempt_id: None,
            lease_id: uuid::Uuid::new_v4(),
            worker_id: WorkerId::new(),
            outcome: TaskOutcome::Succeeded { exit_code: 0 },
            duration_ms: 120,
            output: serde_json::json!({}),
        };
        assert_eq!(report.outcome, TaskOutcome::Succeeded { exit_code: 0 });
        assert!(!report.lease_id.is_nil());
    }
}
