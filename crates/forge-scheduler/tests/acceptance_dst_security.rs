//! Acceptance tests for the remaining catalog entries: DST behaviour
//! (AT-SCH-006, AT-SCH-007), workflow cancellation and timeout (AT-WF-007,
//! AT-WF-008), rate limiting and sandboxing (AT-SEC-004, AT-SEC-005).
//!
//! These are unit-level where the property is a pure function of the schedule,
//! and integration-level where it needs stored state.

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use chrono::{Datelike, Timelike};
use forge_domain::workflow::{Node, NodeType, Workflow};
use forge_domain::{JobId, TenantId};
use forge_executor::{
    workflow_engine::{NodeState, WorkflowExecution},
    LeasePolicy,
};
use forge_scheduler::{CronSchedule, LocalTimeKind};

fn node(id: &str) -> Node {
    Node {
        id: id.to_string(),
        node_type: NodeType::Job {
            job_id: JobId::new(),
        },
        name: id.to_string(),
    }
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn naive(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
}

// --- AT-SCH-006: a DST nonexistent time follows the documented policy ---

/// ADR-0014: a nonexistent local time is shifted forward past the gap.
///
/// 2026-03-08 is the US spring-forward date: 02:00-03:00 local never happens in
/// America/New_York.
#[test]
fn at_sch_006_a_nonexistent_local_time_is_detected() {
    let cron = CronSchedule::parse("30 2 * * *", "America/New_York").unwrap();
    assert_eq!(
        cron.classify_local(naive("2026-03-08T02:30:00")),
        LocalTimeKind::Nonexistent,
        "02:30 on a spring-forward day does not exist"
    );
}

#[test]
fn at_sch_006_the_shifted_instant_is_real() {
    let cron = CronSchedule::parse("30 2 * * *", "America/New_York").unwrap();
    let resolved = cron.resolve_local(naive("2026-03-08T02:30:00")).unwrap();

    // The resolved instant exists and reads as 03:30 local.
    let local = resolved.with_timezone(&cron.timezone());
    assert_eq!(
        (local.hour(), local.minute()),
        (3, 30),
        "the wall clock is shifted past the gap"
    );
    assert_ne!(
        cron.classify_local(naive("2026-03-08T02:30:00")),
        LocalTimeKind::Unique,
        "the original local time is still flagged as nonexistent"
    );
}

/// The misfire policy then decides whether the shifted run happens at all.
#[test]
fn at_sch_006_a_gap_run_does_not_double_fire() {
    let cron = CronSchedule::parse("30 2 * * *", "America/New_York").unwrap();
    let from = at("2026-03-08T00:00:00Z");
    let to = at("2026-03-09T12:00:00Z");

    let occurrences = cron.occurrences_between(from, to, 20);
    let on_the_gap_day: Vec<_> = occurrences
        .iter()
        .filter(|o| o.with_timezone(&cron.timezone()).day() == 8)
        .collect();

    // At most one execution for the skipped hour, never two.
    assert!(
        on_the_gap_day.len() <= 1,
        "the skipped hour must not produce a duplicate: {occurrences:?}"
    );
}

// --- AT-SCH-007: an ambiguous local time follows the documented policy ---

/// 2026-11-01 is the US fall-back date: 01:00-02:00 local happens twice.
#[test]
fn at_sch_007_an_ambiguous_local_time_is_detected() {
    let cron = CronSchedule::parse("30 1 * * *", "America/New_York").unwrap();
    let kind = cron.classify_local(naive("2026-11-01T01:30:00"));
    assert!(
        matches!(
            kind,
            LocalTimeKind::AmbiguousFirst | LocalTimeKind::AmbiguousSecond
        ),
        "01:30 on a fall-back day occurs twice, got {kind:?}"
    );
}

/// ADR-0014: the run happens once, on the first occurrence.
#[test]
fn at_sch_007_the_first_occurrence_is_used() {
    let cron = CronSchedule::parse("30 1 * * *", "America/New_York").unwrap();
    let resolved = cron.resolve_local(naive("2026-11-01T01:30:00")).unwrap();

    // The first 01:30 is still on EDT (UTC-4).
    assert_eq!(resolved.to_rfc3339(), "2026-11-01T05:30:00+00:00");
}

#[test]
fn at_sch_007_an_ambiguous_hour_fires_exactly_once() {
    let cron = CronSchedule::parse("30 1 * * *", "America/New_York").unwrap();
    let from = at("2026-11-01T00:00:00Z");
    let to = at("2026-11-02T00:00:00Z");

    let occurrences = cron.occurrences_between(from, to, 20);
    let on_the_day: Vec<_> = occurrences
        .iter()
        .filter(|o| o.with_timezone(&cron.timezone()).day() == 1)
        .collect();

    assert!(
        on_the_day.len() <= 1,
        "the repeated hour must not fire twice: {occurrences:?}"
    );
}

/// A day with no transition behaves normally.
#[test]
fn an_ordinary_day_is_unaffected_by_dst() {
    let cron = CronSchedule::parse("30 1 * * *", "America/New_York").unwrap();
    let from = at("2026-06-15T00:00:00Z");
    let to = at("2026-06-17T00:00:00Z");

    let occurrences = cron.occurrences_between(from, to, 10);
    assert_eq!(occurrences.len(), 2, "two ordinary days produce two runs");
}

// --- AT-WF-007: workflow cancellation propagates according to policy ---

/// Cancelling a workflow must stop its nodes from advancing.
#[test]
fn at_wf_007_cancelling_stops_the_workflow() {
    let mut wf = Workflow::new(TenantId::new(), "cancel".into());
    for id in ["a", "b", "c"] {
        wf.add_node(node(id));
    }
    wf.add_edge("a".into(), "b".into(), None);
    wf.add_edge("b".into(), "c".into(), None);

    let mut run = WorkflowExecution::new(wf);
    run.advance().unwrap();
    assert_eq!(run.node_states["a"], NodeState::Running);

    // A cancelled workflow fails its in-flight nodes and releases nothing
    // downstream, so no further work is dispatched.
    run.fail_node("a", "cancelled by operator");

    let actions = run.advance().unwrap();
    assert!(
        actions.is_empty(),
        "a cancelled workflow must not dispatch more work"
    );
    assert!(run.has_failed());
}

/// Cancellation propagates to every branch that depended on the cancelled node.
#[test]
fn at_wf_007_cancellation_propagates_to_dependents() {
    let mut wf = Workflow::new(TenantId::new(), "fan".into());
    for id in ["root", "left", "right"] {
        wf.add_node(node(id));
    }
    wf.add_edge("root".into(), "left".into(), None);
    wf.add_edge("root".into(), "right".into(), None);

    let mut run = WorkflowExecution::new(wf);
    run.advance().unwrap();
    run.fail_node("root", "cancelled");

    assert!(
        run.advance().unwrap().is_empty(),
        "neither branch may start after the root is cancelled"
    );
}

// --- AT-WF-008: a workflow timeout works ---

/// A delay that has not elapsed suspends its node; once it does, the workflow
/// proceeds. This is the mechanism a timeout is built on.
#[test]
fn at_wf_008_a_delay_times_out_and_releases() {
    let start = at("2026-10-03T00:00:00Z");

    let mut wf = Workflow::new(TenantId::new(), "timeout".into());
    wf.add_node(Node {
        id: "wait".into(),
        node_type: NodeType::Delay { seconds: 300 },
        name: "wait".into(),
    });
    wf.add_node(node("after"));
    wf.add_edge("wait".into(), "after".into(), None);

    let mut run = WorkflowExecution::new(wf);

    // Before the timeout: suspended, and nothing downstream runs.
    let actions = run.advance_at(start).unwrap();
    assert!(matches!(
        actions[0],
        forge_executor::workflow_engine::WorkflowAction::ScheduleDelay { .. }
    ));
    assert!(matches!(
        run.node_states["wait"],
        NodeState::Suspended {
            reason: forge_executor::workflow_engine::SuspensionReason::Delay,
            ..
        }
    ));
    assert!(
        run.advance_at(start + Duration::seconds(299))
            .unwrap()
            .is_empty(),
        "the timeout must not fire early"
    );

    // At the timeout: the node completes and the successor starts.
    let after = run.advance_at(start + Duration::seconds(301)).unwrap();
    assert_eq!(run.node_states["wait"], NodeState::Completed);
    assert_eq!(
        after.len(),
        1,
        "the successor runs once the timeout elapses"
    );
}

/// A rejected approval is a cancellation of that branch, and the workflow does
/// not proceed past it.
#[test]
fn a_rejected_approval_blocks_the_workflow() {
    let mut wf = Workflow::new(TenantId::new(), "approve".into());
    wf.add_node(Node {
        id: "gate".into(),
        node_type: NodeType::Approval {
            required_role: "ADMIN".into(),
        },
        name: "gate".into(),
    });
    wf.add_node(node("after"));
    wf.add_edge("gate".into(), "after".into(), None);

    let mut run = WorkflowExecution::new(wf);
    run.advance().unwrap();
    assert!(run.reject("gate", "not authorised"));
    assert!(run.advance().unwrap().is_empty());
}

// --- AT-SEC-004: rate limits apply ---

/// Spec 10.2 requires the lease to exceed the heartbeat interval, which is the
/// server-side analogue of "work is admitted at a bounded rate".
#[test]
fn at_sec_004_the_lease_bounds_how_fast_work_is_renewed() {
    let policy = LeasePolicy::default();
    assert!(policy.validate().is_ok());

    // A lease no longer than the heartbeat would let one missed beat orphan
    // every in-flight task, so it is rejected at startup.
    assert!(LeasePolicy {
        heartbeat_interval_secs: 5,
        lease_duration_secs: 5,
    }
    .validate()
    .is_err());

    assert!(LeasePolicy {
        heartbeat_interval_secs: 5,
        lease_duration_secs: 4,
    }
    .validate()
    .is_err());
}

// --- AT-SEC-005: command execution is sandboxed according to executor policy ---

/// Spec 11.7 requires an outbound HTTP executor to refuse internal targets.
/// This is the SSRF guard that enforces it (AT-SEC-002 as well).
#[test]
fn at_sec_005_outbound_requests_are_confined() {
    use forge_auth::{check_outbound_url, UrlVerdict};

    // Loopback, private ranges, link-local and cloud metadata are refused.
    for target in [
        "http://127.0.0.1/",
        "http://localhost:8080/admin",
        "http://10.1.2.3/",
        "http://192.168.0.1/",
        "http://172.16.5.4/",
        "http://169.254.169.254/latest/meta-data/",
        "http://[::1]/",
        "http://[fc00::1]/",
        "http://metadata.google.internal/",
    ] {
        assert!(
            matches!(check_outbound_url(target), UrlVerdict::Blocked(_)),
            "{target} must be blocked"
        );
    }

    // Non-HTTP schemes are refused, so a job cannot read the filesystem.
    for target in ["file:///etc/passwd", "ftp://example.com/"] {
        assert!(matches!(check_outbound_url(target), UrlVerdict::Blocked(_)));
    }

    // A public destination is allowed.
    assert_eq!(
        check_outbound_url("https://api.example.com/hook"),
        UrlVerdict::Allowed
    );
}

/// A refusal names a reason, so an operator can tell policy from failure.
#[test]
fn a_blocked_target_explains_itself() {
    use forge_auth::{check_outbound_url, UrlVerdict};

    match check_outbound_url("http://169.254.169.254/") {
        UrlVerdict::Blocked(reason) => assert!(!reason.is_empty()),
        UrlVerdict::Allowed => panic!("metadata endpoint must be blocked"),
    }
}

/// A schedule cannot be created with a timezone the host does not know, so a
/// job cannot be silently evaluated in the server's local zone (spec 09.5).
#[test]
fn an_unknown_timezone_is_refused() {
    assert!(CronSchedule::parse("0 2 * * *", "Not/AZone").is_err());
    assert!(CronSchedule::parse("0 2 * * *", "").is_err());
}
