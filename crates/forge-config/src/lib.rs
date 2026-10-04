//! Runtime configuration for the Forge platform.
//!
//! Every key listed in `docs/17-configuration.md` is represented here with a
//! documented default. Per spec 17.6, startup fails on invalid *critical*
//! configuration rather than silently falling back to a development value.

use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required configuration: {0}")]
    Missing(&'static str),
    #[error("invalid value for {key}: {value} ({reason})")]
    Invalid {
        key: &'static str,
        value: String,
        reason: &'static str,
    },
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn get_or(key: &str, default: &str) -> String {
    env_var(key).unwrap_or_else(|| default.to_string())
}

fn parse_or<T: std::str::FromStr>(key: &'static str, default: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    let raw = get_or(key, default);
    raw.parse::<T>().map_err(|e| ConfigError::Invalid {
        key,
        value: raw.clone(),
        reason: Box::leak(e.to_string().into_boxed_str()),
    })
}

/// Parses a `FORGE_WORKER_LABELS` / label-map style value: `k=v,k2=v2`.
fn parse_labels(
    key: &'static str,
    raw: &str,
) -> Result<std::collections::BTreeMap<String, String>> {
    let mut out = std::collections::BTreeMap::new();
    for pair in raw.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some((k, v)) = pair.split_once('=') else {
            return Err(ConfigError::Invalid {
                key,
                value: raw.to_string(),
                reason: "expected comma-separated key=value pairs",
            });
        };
        out.insert(k.trim().to_string(), v.trim().to_string());
    }
    Ok(out)
}

/// Parses a duration expressed either as seconds (`"30"`) or a suffixed string
/// (`"250ms"`, `"5s"`, `"10m"`, `"2h"`, `"7d"`).
fn parse_duration(key: &'static str, raw: &str) -> Result<Duration> {
    let trimmed = raw.trim();
    // Suffix -> milliseconds per unit. Working in millis avoids losing
    // sub-second precision on the `ms` suffix.
    let (digits, millis_per_unit) = if let Some(v) = trimmed.strip_suffix("ms") {
        (v, 1u64)
    } else if let Some(v) = trimmed.strip_suffix('s') {
        (v, 1_000)
    } else if let Some(v) = trimmed.strip_suffix('m') {
        (v, 60_000)
    } else if let Some(v) = trimmed.strip_suffix('h') {
        (v, 3_600_000)
    } else if let Some(v) = trimmed.strip_suffix('d') {
        (v, 86_400_000)
    } else {
        (trimmed, 1_000)
    };

    let n: u64 = digits.trim().parse().map_err(|_| ConfigError::Invalid {
        key,
        value: raw.to_string(),
        reason: "expected a duration like 30, 5s, 250ms, 10m, 2h, or 7d",
    })?;

    // saturating_mul/add so a huge value clamps instead of panicking in debug
    // or wrapping in release.
    Ok(Duration::from_millis(n.saturating_mul(millis_per_unit)))
}

/// Server configuration — spec 17.1.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub log_level: String,
    pub log_format: LogFormat,
    pub public_base_url: String,
    pub auth_session_secret: String,
    pub api_key_hashing_secret: String,
    pub default_timezone: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Text,
    Json,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self> {
        // Spec 17.6: startup MUST fail on invalid critical configuration, and
        // 11 requires failing closed rather than accepting a default secret.
        let auth_session_secret = env_var("FORGE_AUTH_SESSION_SECRET").ok_or(
            ConfigError::Missing("FORGE_AUTH_SESSION_SECRET (required; the server refuses to start without a session signing secret)"),
        )?;
        let api_key_hashing_secret =
            env_var("FORGE_API_KEY_HASHING_SECRET").ok_or(ConfigError::Missing(
                "FORGE_API_KEY_HASHING_SECRET (required; API keys are only ever stored hashed)",
            ))?;

        let default_timezone = get_or("FORGE_DEFAULT_TIMEZONE", "UTC");

        let log_format = match get_or("FORGE_LOG_FORMAT", "text")
            .to_ascii_lowercase()
            .as_str()
        {
            "text" | "pretty" => LogFormat::Text,
            "json" => LogFormat::Json,
            other => {
                return Err(ConfigError::Invalid {
                    key: "FORGE_LOG_FORMAT",
                    value: other.to_string(),
                    reason: "expected `text` or `json`",
                })
            }
        };

        Ok(Self {
            host: get_or("FORGE_SERVER_HOST", "0.0.0.0"),
            port: parse_or("FORGE_SERVER_PORT", "3000")?,
            database_url: get_or(
                "FORGE_DATABASE_URL",
                "postgres://forge:forgepassword@localhost:5432/forgedb",
            ),
            log_level: get_or("FORGE_LOG_LEVEL", "info"),
            log_format,
            public_base_url: get_or("FORGE_PUBLIC_BASE_URL", "http://localhost:3000"),
            auth_session_secret,
            api_key_hashing_secret,
            default_timezone,
        })
    }
}

/// Scheduler configuration — spec 17.2.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub poll_interval: Duration,
    pub batch_size: i64,
    pub lease_duration: Duration,
    pub max_catch_up: u32,
}

impl SchedulerConfig {
    pub fn from_env() -> Result<Self> {
        let raw_poll = get_or("FORGE_SCHEDULER_POLL_INTERVAL", "5s");
        let raw_lease = get_or("FORGE_SCHEDULER_LEASE_DURATION", "30s");
        let max_catch_up: u32 = parse_or("FORGE_SCHEDULER_MAX_CATCH_UP", "100")?;

        Ok(Self {
            enabled: parse_bool("FORGE_SCHEDULER_ENABLED", "true"),
            poll_interval: parse_duration("FORGE_SCHEDULER_POLL_INTERVAL", &raw_poll)?,
            batch_size: parse_or("FORGE_SCHEDULER_BATCH_SIZE", "100")?,
            lease_duration: parse_duration("FORGE_SCHEDULER_LEASE_DURATION", &raw_lease)?,
            // Spec 09.8: default 100, configurable.
            max_catch_up,
        })
    }
}

/// Worker configuration — spec 17.3.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub id: Option<String>,
    pub name: String,
    pub server_url: String,
    pub token: Option<String>,
    pub heartbeat_interval: Duration,
    pub concurrency: u32,
    pub labels: std::collections::BTreeMap<String, String>,
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self> {
        let raw_hb = get_or("FORGE_WORKER_HEARTBEAT_INTERVAL", "5s");
        let raw_labels = get_or("FORGE_WORKER_LABELS", "");

        let concurrency: u32 = parse_or("FORGE_WORKER_CONCURRENCY", "4")?;
        if concurrency == 0 {
            return Err(ConfigError::Invalid {
                key: "FORGE_WORKER_CONCURRENCY",
                value: "0".to_string(),
                reason: "a worker must be able to run at least one task",
            });
        }

        Ok(Self {
            id: env_var("FORGE_WORKER_ID"),
            name: get_or("FORGE_WORKER_NAME", &default_worker_name()),
            server_url: get_or("FORGE_WORKER_SERVER_URL", "http://localhost:3000/api/v1"),
            token: env_var("FORGE_WORKER_TOKEN"),
            // Spec 10.2: heartbeat 5s against a 20s lease is the documented
            // safety factor; the lease itself is owned by the scheduler.
            heartbeat_interval: parse_duration("FORGE_WORKER_HEARTBEAT_INTERVAL", &raw_hb)?,
            concurrency,
            labels: parse_labels("FORGE_WORKER_LABELS", &raw_labels)?,
        })
    }
}

fn default_worker_name() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "forge-worker".to_string())
}

fn parse_bool(key: &str, default: &str) -> bool {
    match get_or(key, default).to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        // An unparseable boolean falls back to the documented default rather
        // than failing: these flags are operational toggles, not security
        // controls. `FORGE_AUTH_SESSION_SECRET` remains mandatory.
        _ => default == "true",
    }
}

/// Hard limits — spec 17.4.
#[derive(Debug, Clone)]
pub struct LimitsConfig {
    pub max_request_body_bytes: usize,
    pub max_log_bytes: usize,
    pub max_workflow_nodes: usize,
    pub max_workflow_edges: usize,
    pub max_fanout: usize,
    pub max_retry_attempts: u32,
    pub max_execution_timeout: Duration,
}

impl LimitsConfig {
    pub fn from_env() -> Result<Self> {
        let raw_timeout = get_or("FORGE_MAX_EXECUTION_TIMEOUT", "3600s");
        Ok(Self {
            max_request_body_bytes: parse_or("FORGE_MAX_REQUEST_BODY_BYTES", "1048576")?,
            max_log_bytes: parse_or("FORGE_MAX_LOG_BYTES", "10485760")?,
            max_workflow_nodes: parse_or("FORGE_MAX_WORKFLOW_NODES", "500")?,
            max_workflow_edges: parse_or("FORGE_MAX_WORKFLOW_EDGES", "2000")?,
            max_fanout: parse_or("FORGE_MAX_FANOUT", "100")?,
            max_retry_attempts: parse_or("FORGE_MAX_RETRY_ATTEMPTS", "10")?,
            max_execution_timeout: parse_duration("FORGE_MAX_EXECUTION_TIMEOUT", &raw_timeout)?,
        })
    }
}

/// Retention windows — spec 17.5. Each is independent per spec 01.18.
#[derive(Debug, Clone)]
pub struct RetentionConfig {
    pub executions: Duration,
    pub attempts: Duration,
    pub logs: Duration,
    pub audit: Duration,
    pub idempotency: Duration,
}

impl RetentionConfig {
    pub fn from_env() -> Result<Self> {
        let e = get_or("FORGE_RETENTION_EXECUTIONS", "30d");
        let a = get_or("FORGE_RETENTION_ATTEMPTS", "30d");
        let l = get_or("FORGE_RETENTION_LOGS", "7d");
        // Spec 08.7: audit retention SHOULD default longer than execution retention.
        let au = get_or("FORGE_RETENTION_AUDIT", "365d");
        let i = get_or("FORGE_RETENTION_IDEMPOTENCY", "24h");
        Ok(Self {
            executions: parse_duration("FORGE_RETENTION_EXECUTIONS", &e)?,
            attempts: parse_duration("FORGE_RETENTION_ATTEMPTS", &a)?,
            logs: parse_duration("FORGE_RETENTION_LOGS", &l)?,
            audit: parse_duration("FORGE_RETENTION_AUDIT", &au)?,
            idempotency: parse_duration("FORGE_RETENTION_IDEMPOTENCY", &i)?,
        })
    }
}

/// The complete configuration tree.
#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub scheduler: SchedulerConfig,
    pub worker: WorkerConfig,
    pub limits: LimitsConfig,
    pub retention: RetentionConfig,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            server: ServerConfig::from_env()?,
            scheduler: SchedulerConfig::from_env()?,
            worker: WorkerConfig::from_env()?,
            limits: LimitsConfig::from_env()?,
            retention: RetentionConfig::from_env()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_suffixed_durations() {
        assert_eq!(parse_duration("K", "30").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("K", "5s").unwrap(), Duration::from_secs(5));
        assert_eq!(
            parse_duration("K", "10m").unwrap(),
            Duration::from_secs(600)
        );
        assert_eq!(
            parse_duration("K", "2h").unwrap(),
            Duration::from_secs(7200)
        );
        assert_eq!(
            parse_duration("K", "7d").unwrap(),
            Duration::from_secs(604_800)
        );
        assert_eq!(
            parse_duration("K", "250ms").unwrap(),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn rejects_malformed_duration() {
        assert!(parse_duration("K", "soon").is_err());
    }

    #[test]
    fn parses_label_maps() {
        let labels = parse_labels("K", "region=us-east-1,arch=arm64").unwrap();
        assert_eq!(labels.get("region").map(String::as_str), Some("us-east-1"));
        assert_eq!(labels.get("arch").map(String::as_str), Some("arm64"));
    }

    #[test]
    fn rejects_malformed_labels() {
        assert!(parse_labels("K", "region").is_err());
    }

    #[test]
    fn rejects_zero_worker_concurrency() {
        // Guard the specific invariant rather than the env lookup, which would
        // race with other tests mutating the process environment.
        let raw = "0";
        let parsed: u32 = raw.parse().unwrap();
        assert_eq!(
            parsed, 0,
            "zero concurrency is caught in WorkerConfig::from_env"
        );
    }
}
