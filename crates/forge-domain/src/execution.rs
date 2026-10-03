use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::id::{ExecutionId, JobId, JobVersionId, TenantId, WorkerId};
use crate::error::DomainError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    Queued,
    Dispatched,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    CancelRequested,
    Cancelled,
    RetryScheduled,
    DeadLettered,
    Abandoned,
}

impl ExecutionStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ExecutionStatus::Succeeded
                | ExecutionStatus::Failed
                | ExecutionStatus::TimedOut
                | ExecutionStatus::Cancelled
                | ExecutionStatus::DeadLettered
                | ExecutionStatus::Abandoned
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    pub id: ExecutionId,
    pub tenant_id: TenantId,
    pub job_id: JobId,
    pub job_version_id: JobVersionId,
    pub status: ExecutionStatus,
    pub worker_id: Option<WorkerId>,
    pub attempt_count: u32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl Execution {
    pub fn new(tenant_id: TenantId, job_id: JobId, job_version_id: JobVersionId) -> Self {
        Self {
            id: ExecutionId::new(),
            tenant_id,
            job_id,
            job_version_id,
            status: ExecutionStatus::Queued,
            worker_id: None,
            attempt_count: 0,
            created_at: Utc::now(),
            started_at: None,
            ended_at: None,
        }
    }

    pub fn transition_to(&mut self, new_status: ExecutionStatus) -> Result<(), DomainError> {
        if self.status.is_terminal() {
            return Err(DomainError::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", new_status),
            });
        }
        
        // Basic state machine validation
        let valid = matches!(
            (&self.status, &new_status),
            (ExecutionStatus::Queued, ExecutionStatus::Dispatched)
                | (ExecutionStatus::Dispatched, ExecutionStatus::Running)
                | (_, ExecutionStatus::CancelRequested)
                | (ExecutionStatus::CancelRequested, ExecutionStatus::Cancelled)
                | (ExecutionStatus::Running, ExecutionStatus::Succeeded)
                | (ExecutionStatus::Running, ExecutionStatus::Failed)
                | (ExecutionStatus::Running, ExecutionStatus::TimedOut)
                | (ExecutionStatus::Failed, ExecutionStatus::RetryScheduled)
                | (ExecutionStatus::Failed, ExecutionStatus::DeadLettered)
                | (ExecutionStatus::RetryScheduled, ExecutionStatus::Queued)
                | (
                    ExecutionStatus::Dispatched | ExecutionStatus::Running,
                    ExecutionStatus::Abandoned
                )
        );

        if !valid {
            return Err(DomainError::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", new_status),
            });
        }

        self.status = new_status;
        
        if new_status.is_terminal() {
            self.ended_at = Some(Utc::now());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::TenantId;
    use crate::id::JobId;
    use crate::id::JobVersionId;

    #[test]
    fn test_execution_transitions() {
        let tenant = TenantId::new();
        let mut exec = Execution::new(tenant, JobId::new(), JobVersionId::new());
        
        assert_eq!(exec.status, ExecutionStatus::Queued);
        
        exec.transition_to(ExecutionStatus::Dispatched).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Dispatched);
        
        exec.transition_to(ExecutionStatus::Running).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Running);
        
        exec.transition_to(ExecutionStatus::Succeeded).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Succeeded);
        
        let err = exec.transition_to(ExecutionStatus::Failed);
        assert!(err.is_err());
    }
}
