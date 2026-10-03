use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackoffStrategy {
    Fixed { delay: Duration },
    Linear { initial_delay: Duration, increment: Duration },
    Exponential { initial_delay: Duration, multiplier: u32, max_delay: Duration },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff: BackoffStrategy,
    pub retryable_error_classes: Vec<String>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential {
                initial_delay: Duration::from_secs(1),
                multiplier: 2,
                max_delay: Duration::from_secs(3600),
            },
            retryable_error_classes: vec!["*".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConcurrencyLimit {
    Unlimited,
    Bounded(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConcurrencyPolicy {
    pub max_concurrent_executions: ConcurrencyLimit,
    pub queue_id: Option<String>,
}

impl Default for ConcurrencyPolicy {
    fn default() -> Self {
        Self {
            max_concurrent_executions: ConcurrencyLimit::Unlimited,
            queue_id: None,
        }
    }
}
