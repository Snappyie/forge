use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum SchedulerError {
    #[error("Invalid cron expression: {0}")]
    InvalidCronExpression(String),
}

pub struct CronSchedule {
    schedule: Schedule,
}

impl CronSchedule {
    pub fn parse(expression: &str) -> Result<Self, SchedulerError> {
        let schedule = Schedule::from_str(expression)
            .map_err(|e| SchedulerError::InvalidCronExpression(e.to_string()))?;
        Ok(Self { schedule })
    }

    pub fn next_run_at(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.schedule.after(&after).next()
    }
}
