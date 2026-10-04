//! Metrics (spec 12, AT-OBS-002/003/005).
//!
//! A small in-process registry rather than a full telemetry stack: the numbers
//! the console and operators need must be inspectable without standing up a
//! metrics backend. Snapshots are also rendered in Prometheus text format so a
//! scrape endpoint can expose them with no extra dependency.

use std::collections::BTreeMap;
use std::sync::RwLock;

use chrono::{DateTime, Utc};

/// A labelled counter or gauge.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub name: &'static str,
    pub value: f64,
    pub labels: BTreeMap<String, String>,
}

/// Point-in-time metrics for the console (spec 7.4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetricsSnapshot {
    /// Executions currently queued (AT-OBS-003).
    pub queue_depth: i64,
    /// Oldest queued age in seconds.
    pub oldest_queued_age_secs: i64,
    pub running: i64,
    pub succeeded_24h: i64,
    pub failed_24h: i64,
    pub dead_lettered_24h: i64,
    /// Success rate over the window, as a fraction.
    pub success_rate: f64,
    /// Mean execution latency in milliseconds over the window.
    pub mean_latency_ms: f64,
    pub workers_ready: i64,
    pub workers_offline: i64,
    /// Per-queue depth, for the queues page.
    pub queues: BTreeMap<String, i64>,
}

impl MetricsSnapshot {
    /// Prometheus text exposition, for a scrape endpoint.
    pub fn to_prometheus(&self) -> String {
        let mut out = String::new();
        let mut push = |name: &str, help: &str, kind: &str, value: f64| {
            out.push_str(&format!("# HELP forge_{name} {help}\n"));
            out.push_str(&format!("# TYPE forge_{name} {kind}\n"));
            out.push_str(&format!("forge_{name} {value}\n"));
        };

        push(
            "queue_depth",
            "Executions awaiting dispatch.",
            "gauge",
            self.queue_depth as f64,
        );
        push(
            "oldest_queued_age_seconds",
            "Age of the oldest queued execution.",
            "gauge",
            self.oldest_queued_age_secs as f64,
        );
        push(
            "executions_running",
            "Executions currently running.",
            "gauge",
            self.running as f64,
        );
        push(
            "executions_succeeded_24h",
            "Executions succeeded in the last 24h.",
            "counter",
            self.succeeded_24h as f64,
        );
        push(
            "executions_failed_24h",
            "Executions failed in the last 24h.",
            "counter",
            self.failed_24h as f64,
        );
        push(
            "executions_dead_lettered_24h",
            "Executions dead-lettered in the last 24h.",
            "counter",
            self.dead_lettered_24h as f64,
        );
        push(
            "execution_success_rate",
            "Success rate over the last 24h.",
            "gauge",
            self.success_rate,
        );
        push(
            "execution_mean_latency_ms",
            "Mean execution latency.",
            "gauge",
            self.mean_latency_ms,
        );
        push(
            "workers_ready",
            "Workers ready to accept work.",
            "gauge",
            self.workers_ready as f64,
        );
        push(
            "workers_offline",
            "Workers currently offline.",
            "gauge",
            self.workers_offline as f64,
        );

        for (queue, depth) in &self.queues {
            out.push_str(&format!(
                "forge_queue_depth{{queue=\"{}\"}} {}\n",
                escape_label(queue),
                depth
            ));
        }
        out
    }
}

fn escape_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// In-process metric registry.
///
/// Counters only move forward; gauges are set from outside.
#[derive(Debug, Default)]
pub struct Metrics {
    counters: RwLock<BTreeMap<(String, String), f64>>,
    gauges: RwLock<BTreeMap<(String, String), f64>>,
    snapshot: RwLock<MetricsSnapshot>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `delta` to a counter.
    pub fn increment(&self, name: &str, labels: &[(&str, &str)], delta: f64) {
        let key = label_key(name, labels);
        if let Ok(mut counters) = self.counters.write() {
            *counters.entry(key).or_insert(0.0) += delta;
        }
    }

    /// Sets a gauge to an absolute value.
    pub fn set_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64) {
        let key = label_key(name, labels);
        if let Ok(mut gauges) = self.gauges.write() {
            gauges.insert(key, value);
        }
    }

    pub fn counter(&self, name: &str, labels: &[(&str, &str)]) -> f64 {
        self.counters
            .read()
            .ok()
            .and_then(|c| c.get(&label_key(name, labels)).copied())
            .unwrap_or(0.0)
    }

    pub fn gauge(&self, name: &str, labels: &[(&str, &str)]) -> f64 {
        self.gauges
            .read()
            .ok()
            .and_then(|g| g.get(&label_key(name, labels)).copied())
            .unwrap_or(0.0)
    }

    /// Records the current console metrics.
    pub fn set_snapshot(&self, snapshot: MetricsSnapshot) {
        if let Ok(mut slot) = self.snapshot.write() {
            *slot = snapshot;
        }
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        self.snapshot.read().map(|s| s.clone()).unwrap_or_default()
    }

    /// Every counter, for a scrape or a debug dump.
    pub fn all_counters(&self) -> Vec<Sample> {
        self.counters
            .read()
            .map(|c| {
                c.iter()
                    .map(|((name, labels), value)| Sample {
                        name: Box::leak(name.clone().into_boxed_str()),
                        value: *value,
                        labels: parse_labels(labels),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every gauge.
    pub fn all_gauges(&self) -> Vec<Sample> {
        self.gauges
            .read()
            .map(|g| {
                g.iter()
                    .map(|((name, labels), value)| Sample {
                        name: Box::leak(name.clone().into_boxed_str()),
                        value: *value,
                        labels: parse_labels(labels),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn label_key(name: &str, labels: &[(&str, &str)]) -> (String, String) {
    if labels.is_empty() {
        return (name.to_string(), String::new());
    }
    let rendered = labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");
    (name.to_string(), rendered)
}

fn parse_labels(rendered: &str) -> BTreeMap<String, String> {
    rendered
        .split(',')
        .filter(|p| !p.is_empty())
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Why an execution is in the state it is in.
///
/// AT-OBS-005: an operator must be able to see *why* work was dispatched where
/// it was, without querying the database.
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchExplanation {
    pub execution_id: String,
    pub job_id: String,
    pub queue: Option<String>,
    pub worker_id: Option<String>,
    pub status: String,
    pub trigger_source: String,
    pub priority: String,
    /// The reason it was eligible: "queued", "recovered", "retry_scheduled", …
    pub eligible_because: String,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}

impl DispatchExplanation {
    /// One-line summary for a log or UI tooltip.
    pub fn to_sentence(&self) -> String {
        let queue = self.queue.as_deref().unwrap_or("unassigned");
        let worker = self.worker_id.as_deref().unwrap_or("unassigned");
        format!(
            "execution {} is {} on queue `{}` for worker `{}` (source: {}, priority: {}): {}",
            self.execution_id,
            self.status,
            queue,
            worker,
            self.trigger_source,
            self.priority,
            self.eligible_because
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AT-OBS-002: worker metrics are recorded.
    #[test]
    fn counters_accumulate_per_label_set() {
        let m = Metrics::new();
        m.increment("executions_dispatched", &[("queue", "default")], 1.0);
        m.increment("executions_dispatched", &[("queue", "default")], 2.0);
        m.increment("executions_dispatched", &[("queue", "bulk")], 5.0);

        assert_eq!(
            m.counter("executions_dispatched", &[("queue", "default")]),
            3.0
        );
        assert_eq!(
            m.counter("executions_dispatched", &[("queue", "bulk")]),
            5.0,
            "label sets are independent"
        );
        assert_eq!(m.counter("never_touched", &[]), 0.0);
    }

    #[test]
    fn gauges_are_absolute_not_accumulated() {
        let m = Metrics::new();
        m.set_gauge("queue_depth", &[("queue", "default")], 7.0);
        m.set_gauge("queue_depth", &[("queue", "default")], 3.0);
        assert_eq!(
            m.gauge("queue_depth", &[("queue", "default")]),
            3.0,
            "a gauge reflects the latest value"
        );
    }

    #[test]
    fn unlabelled_metrics_are_separate_from_labelled_ones() {
        let m = Metrics::new();
        m.increment("events", &[], 1.0);
        assert_eq!(m.counter("events", &[]), 1.0);
        assert_eq!(m.counter("events", &[("queue", "default")]), 0.0);
    }

    #[test]
    fn metrics_can_be_enumerated_for_scrape() {
        let m = Metrics::new();
        m.increment("a", &[("k", "v")], 1.0);
        m.set_gauge("b", &[], 2.0);

        let counters = m.all_counters();
        assert_eq!(counters.len(), 1);
        assert_eq!(counters[0].value, 1.0);
        assert_eq!(counters[0].labels.get("k").map(String::as_str), Some("v"));

        assert_eq!(m.all_gauges().len(), 1);
    }

    #[test]
    fn concurrency_is_safe_for_shared_use() {
        use std::sync::Arc;
        let m = Arc::new(Metrics::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let m = Arc::clone(&m);
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    m.increment("hits", &[], 1.0);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(m.counter("hits", &[]), 800.0, "no increments are lost");
    }

    // AT-OBS-003: queue depth is visible.
    #[test]
    fn a_snapshot_exposes_queue_depth() {
        let m = Metrics::new();
        let mut queues = BTreeMap::new();
        queues.insert("default".to_string(), 12);
        m.set_snapshot(MetricsSnapshot {
            queue_depth: 12,
            oldest_queued_age_secs: 340,
            queues,
            ..Default::default()
        });

        let read = m.snapshot();
        assert_eq!(read.queue_depth, 12);
        assert_eq!(read.oldest_queued_age_secs, 340);
        assert_eq!(read.queues.get("default"), Some(&12));
    }

    #[test]
    fn the_snapshot_renders_prometheus_text() {
        let mut queues = BTreeMap::new();
        queues.insert("bulk".to_string(), 9);
        let snapshot = MetricsSnapshot {
            queue_depth: 4,
            success_rate: 0.97,
            queues,
            ..Default::default()
        };

        let text = snapshot.to_prometheus();
        assert!(text.contains("forge_queue_depth 4"));
        assert!(text.contains("# TYPE forge_execution_success_rate gauge"));
        assert!(text.contains("forge_execution_success_rate 0.97"));
        assert!(text.contains(r#"forge_queue_depth{queue="bulk"} 9"#));
    }

    #[test]
    fn queue_labels_are_escaped() {
        let mut queues = BTreeMap::new();
        queues.insert("we\"ird\\queue".to_string(), 1);
        let snapshot = MetricsSnapshot {
            queues,
            ..Default::default()
        };
        let text = snapshot.to_prometheus();
        assert!(
            text.contains(r#"queue="we\"ird\\queue""#),
            "a quote in a label must be escaped: {text}"
        );
    }

    // AT-OBS-005: a dispatch explanation is available.
    #[test]
    fn a_dispatch_explains_itself() {
        let explanation = DispatchExplanation {
            execution_id: "exec-1".into(),
            job_id: "job-1".into(),
            queue: Some("default".into()),
            worker_id: Some("worker-7".into()),
            status: "DISPATCHED".into(),
            trigger_source: "SCHEDULE".into(),
            priority: "HIGH".into(),
            eligible_because: "priority over age".into(),
            scheduled_for: None,
            created_at: None,
        };

        let sentence = explanation.to_sentence();
        assert!(sentence.contains("exec-1"));
        assert!(sentence.contains("DISPATCHED"));
        assert!(sentence.contains("worker-7"));
        assert!(sentence.contains("priority over age"));
    }

    #[test]
    fn an_unassigned_execution_says_so() {
        let explanation = DispatchExplanation {
            execution_id: "exec-2".into(),
            job_id: "job-2".into(),
            queue: None,
            worker_id: None,
            status: "QUEUED".into(),
            trigger_source: "MANUAL".into(),
            priority: "NORMAL".into(),
            eligible_because: "awaiting a capable worker".into(),
            scheduled_for: None,
            created_at: None,
        };
        let sentence = explanation.to_sentence();
        assert!(sentence.contains("unassigned"));
    }

    #[test]
    fn label_keys_render_deterministically() {
        let labels = [("b", "2"), ("a", "1")];
        // Order follows the input, so two call sites agree.
        assert_eq!(label_key("m", &labels).1, "b=2,a=1");
        let parsed = parse_labels("b=2,a=1");
        assert_eq!(parsed.get("a").map(String::as_str), Some("1"));
        assert_eq!(parsed.len(), 2);
    }
}
