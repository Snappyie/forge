//! Scheduler leader election.
//!
//! `redesign.md` §A asks to "prevent duplicate schedule generation across
//! scheduler replicas" and to "support high availability and scheduler leader
//! failover". The first half is already solved at the row level: `claim_due`
//! writes a per-schedule lease inside its transaction, and a unique index on
//! `(schedule_id, scheduled_for)` means two schedulers racing the same
//! occurrence cannot both produce an execution. That is the property that
//! actually matters, and it holds with any number of replicas.
//!
//! What is missing is coordination. Without it, N replicas all wake on the same
//! tick, all run the same `claim_due` query, and all but one come back empty —
//! N replicas' worth of database work for one replica's worth of progress. It
//! also makes failover invisible: nothing records which instance is currently
//! responsible, so an operator cannot tell a paused scheduler from a scheduler
//! whose leader is unreachable.
//!
//! The lock is a Postgres advisory lock, chosen deliberately over a
//! lease-row-plus-heartbeat:
//!
//! * it needs no table, so there is no migration and no state to migrate;
//! * it is released automatically when the connection drops, so a crashed leader
//!   is replaced as soon as its connection is reaped — with no expiry window
//!   during which nobody leads;
//! * it is scoped to one key, so an unrelated advisory lock elsewhere in the
//!   database cannot collide with it.

use sqlx::PgPool;
use uuid::Uuid;

/// The advisory-lock key for the scheduler.
///
/// Arbitrary but fixed. Postgres keys are two `int4` values or one `int8`; the
/// pair is namespaced against other users of advisory locks in this database so
/// a future feature taking one cannot collide by accident.
const LEADER_LOCK_CLASS: i32 = 0x4650; // "FP" — Forge Platform
const LEADER_LOCK_KEY: i32 = 0x0001;

/// Whether this instance is currently the leader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leadership {
    /// This instance holds the lock and should run the evaluation loop.
    Held,
    /// Another instance holds it. This one should idle.
    HeldElsewhere,
}

impl Leadership {
    pub fn is_leader(self) -> bool {
        matches!(self, Leadership::Held)
    }
}

/// A held leadership lock.
///
/// Dropping the connection releases the lock, so leadership does not survive the
/// process — which is the point. A dead leader's work is picked up by a live one
/// rather than being stranded until a lease expires.
pub struct LeaderGuard {
    /// Kept alive for as long as leadership is held. Dropping it drops the
    /// connection and releases the lock.
    _connection: sqlx::pool::PoolConnection<sqlx::Postgres>,
    instance_id: Uuid,
}

impl LeaderGuard {
    pub fn instance_id(&self) -> Uuid {
        self.instance_id
    }
}

/// Attempts to become the leader.
///
/// Returns `None` when another instance already holds the lock. `pg_try_advisory_lock`
/// is non-blocking by design: a scheduler that blocked waiting for leadership
/// would hold a request open for the length of a leader's whole tenure, and the
/// failure mode of a scheduler is to keep polling, not to queue.
pub async fn try_acquire(pool: &PgPool, instance_id: Uuid) -> Result<Option<LeaderGuard>, sqlx::Error> {
    let mut connection = pool.acquire().await?;

    let acquired: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_lock($1, $2)",
    )
    .bind(LEADER_LOCK_CLASS)
    .bind(LEADER_LOCK_KEY)
    .fetch_one(&mut *connection)
    .await?;

    if !acquired {
        // Release the connection immediately; it holds no lock.
        connection.close().await?;
        return Ok(None);
    }

    Ok(Some(LeaderGuard {
        _connection: connection,
        instance_id,
    }))
}

/// Releases leadership explicitly.
///
/// Dropping the guard does the same thing. This exists so a caller that is
/// shutting down can log the handover at a known point rather than during
/// teardown.
pub async fn release(guard: LeaderGuard) -> Result<(), sqlx::Error> {
    let mut connection = guard._connection;
    sqlx::query("SELECT pg_advisory_unlock($1, $2)")
        .bind(LEADER_LOCK_CLASS)
        .bind(LEADER_LOCK_KEY)
        .execute(&mut *connection)
        .await?;
    connection.close().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_key_is_namespaced_and_fixed() {
        // A stable, non-zero, obviously-namespaced pair. Changing either value
        // silently splits the deployment into two independent leader sets, so
        // this is pinned rather than left to a reader's judgement.
        assert_ne!(LEADER_LOCK_CLASS, 0);
        assert_ne!(LEADER_LOCK_KEY, 0);
        assert_eq!(LEADER_LOCK_CLASS, 0x4650);
        assert_eq!(LEADER_LOCK_KEY, 1);
    }

    #[test]
    fn held_leadership_reports_itself_as_leader() {
        assert!(Leadership::Held.is_leader());
        assert!(!Leadership::HeldElsewhere.is_leader());
    }
}