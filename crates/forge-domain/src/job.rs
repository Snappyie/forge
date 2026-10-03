use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::id::{JobId, JobVersionId, TenantId};
use crate::policy::{RetryPolicy, ConcurrencyPolicy};
use crate::error::DomainError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatus {
    Draft,
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionType {
    HttpRequest,
    ContainerCommand,
    WorkerTask,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceRequirements {
    pub cpu_units: Option<u32>,
    pub memory_mb: Option<u32>,
    pub worker_capabilities: Vec<String>,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            cpu_units: None,
            memory_mb: None,
            worker_capabilities: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    pub status: JobStatus,
    pub current_version_id: Option<JobVersionId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobVersion {
    pub id: JobVersionId,
    pub job_id: JobId,
    pub execution_type: ExecutionType,
    pub retry_policy: RetryPolicy,
    pub concurrency_policy: ConcurrencyPolicy,
    pub resource_requirements: ResourceRequirements,
    pub timeout_seconds: u32,
    pub created_at: DateTime<Utc>,
}

impl Job {
    pub fn new(tenant_id: TenantId, name: String) -> Self {
        let now = Utc::now();
        Self {
            id: JobId::new(),
            tenant_id,
            name,
            description: None,
            status: JobStatus::Draft,
            current_version_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn transition_to(&mut self, new_status: JobStatus) -> Result<(), DomainError> {
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
        self.updated_at = Utc::now();
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
