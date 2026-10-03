use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use forge_domain::{TenantId, WorkerId, ExecutionId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerStatus {
    Online,
    Draining,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub id: WorkerId,
    pub tenant_id: TenantId,
    pub hostname: String,
    pub capabilities: Vec<String>,
    pub status: WorkerStatus,
    pub last_heartbeat_at: DateTime<Utc>,
}

impl Worker {
    pub fn new(tenant_id: TenantId, hostname: String, capabilities: Vec<String>) -> Self {
        Self {
            id: WorkerId::new(),
            tenant_id,
            hostname,
            capabilities,
            status: WorkerStatus::Online,
            last_heartbeat_at: Utc::now(),
        }
    }

    pub fn heartbeat(&mut self) {
        self.last_heartbeat_at = Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub execution_id: ExecutionId,
    pub worker_id: WorkerId,
    pub expires_at: DateTime<Utc>,
}

impl Lease {
    pub fn new(execution_id: ExecutionId, worker_id: WorkerId, duration_sec: i64) -> Self {
        Self {
            execution_id,
            worker_id,
            expires_at: Utc::now() + chrono::Duration::seconds(duration_sec),
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn renew(&mut self, additional_sec: i64) {
        self.expires_at = Utc::now() + chrono::Duration::seconds(additional_sec);
    }
}
