use crate::error::DomainError;
use crate::id::{JobId, JobVersionId, QueueId, TenantId};
use crate::policy::{ConcurrencyPolicy, Priority, RetryPolicy};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    #[default]
    Draft,
    Active,
    Archived,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Draft => "DRAFT",
            JobStatus::Active => "ACTIVE",
            JobStatus::Archived => "ARCHIVED",
        }
    }
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for JobStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "DRAFT" => Ok(JobStatus::Draft),
            "ACTIVE" => Ok(JobStatus::Active),
            "ARCHIVED" => Ok(JobStatus::Archived),
            other => Err(DomainError::ValidationError(format!(
                "unknown job status: {other}"
            ))),
        }
    }
}

/// How a version's work is actually carried out (spec 01.4).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionType {
    HttpRequest,
    ContainerCommand,
    WorkerTask,
}

impl ExecutionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionType::HttpRequest => "HTTP_REQUEST",
            ExecutionType::ContainerCommand => "CONTAINER_COMMAND",
            ExecutionType::WorkerTask => "WORKER_TASK",
        }
    }
}

impl std::str::FromStr for ExecutionType {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "HTTP_REQUEST" => Ok(ExecutionType::HttpRequest),
            "CONTAINER_COMMAND" => Ok(ExecutionType::ContainerCommand),
            "WORKER_TASK" => Ok(ExecutionType::WorkerTask),
            other => Err(DomainError::ValidationError(format!(
                "unknown execution type: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ResourceRequirements {
    pub cpu_units: Option<u32>,
    pub memory_mb: Option<u32>,
    pub worker_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub tenant_id: TenantId,
    /// Stable human-readable identifier, unique within the tenant (spec 02.2).
    pub key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub status: JobStatus,
    pub current_version_id: Option<JobVersionId>,
    pub default_queue_id: Option<QueueId>,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobVersion {
    pub id: JobVersionId,
    pub job_id: JobId,
    pub version_number: u32,
    pub execution_type: ExecutionType,
    pub retry_policy: RetryPolicy,
    pub concurrency_policy: ConcurrencyPolicy,
    pub resource_requirements: ResourceRequirements,
    pub timeout_seconds: u32,
    pub created_at: DateTime<Utc>,
    /// Set once published. A published version is immutable (spec 02.16,
    /// invariant 4).
    pub published_at: Option<DateTime<Utc>>,
}

impl JobVersion {
    pub fn is_published(&self) -> bool {
        self.published_at.is_some()
    }

    /// Rejects any mutation of an already-published version.
    pub fn ensure_mutable(&self) -> Result<(), DomainError> {
        if self.is_published() {
            return Err(DomainError::ValidationError(format!(
                "job version {} is published and immutable",
                self.id
            )));
        }
        Ok(())
    }
}

impl Job {
    pub fn new(tenant_id: TenantId, name: String) -> Self {
        let now = Utc::now();
        Self {
            id: JobId::new(),
            tenant_id,
            key: None,
            name,
            description: None,
            status: JobStatus::Draft,
            current_version_id: None,
            default_queue_id: None,
            priority: Priority::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Transitions with an explicit clock so tests stay deterministic.
    pub fn transition_to(&mut self, new_status: JobStatus) -> Result<(), DomainError> {
        self.transition_to_at(new_status, Utc::now())
    }

    pub fn transition_to_at(
        &mut self,
        new_status: JobStatus,
        now: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        match (&self.status, &new_status) {
            (JobStatus::Draft, JobStatus::Active) => {
                if self.current_version_id.is_none() {
                    return Err(DomainError::ValidationError(
                        "Cannot activate job without a current version".to_string(),
                    ));
                }
                self.status = new_status;
            }
            (JobStatus::Active, JobStatus::Archived) => self.status = new_status,
            (JobStatus::Draft, JobStatus::Archived) => self.status = new_status,
            (from, to) => {
                return Err(DomainError::InvalidStateTransition {
                    from: format!("{:?}", from),
                    to: format!("{:?}", to),
                });
            }
        }
        self.updated_at = now;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_transitions() {
        let tenant = TenantId::new();
        let mut job = Job::new(tenant, "Report Job".to_string());

        // Initial state should be Draft
        assert_eq!(job.status, JobStatus::Draft);

        let err = job.transition_to(JobStatus::Active);
        assert!(err.is_err());

        // Add version
        job.current_version_id = Some(JobVersionId::new());
        job.transition_to(JobStatus::Active).unwrap();
        assert_eq!(job.status, JobStatus::Active);

        // Active -> Archived
        job.transition_to(JobStatus::Archived).unwrap();
        assert_eq!(job.status, JobStatus::Archived);
    }
}
