//! Scheduler leader election (`redesign.md` §A: high availability and leader
//! failover).
//!
//! These drive real advisory locks against a real database, because the
//! property being asserted — that two connections cannot both hold the lock — is
//! a property of Postgres, not of this code.

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use uuid::Uuid;

struct TestDb {
    pool: PgPool,
    admin_url: String,
    db_name: String,
}

impl TestDb {
    async fn new() -> Option<Self> {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));
        let db_name = format!("forge_leader_{}", Uuid::new_v4().simple());

        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
            .ok()?;
        if admin
            .execute(sqlx::AssertSqlSafe(format!(
                r#"CREATE DATABASE "{db_name}""#
            )))
            .await
            .is_err()
        {
            eprintln!("skipping: cannot create database");
            return None;
        }
        admin.close().await;

        let pool = PgPoolOptions::new()
            // Deliberately more than one connection: leadership is per
            // connection, so a pool that only ever hands out one would make the
            // test pass for the wrong reason.
            .max_connections(10)
            .connect(&format!("{server}/{db_name}"))
            .await
            .ok()?;

        Some(Self {
            pool,
            admin_url,
            db_name,
        })
    }

    async fn cleanup(self) {
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            let _ = admin
                .execute(sqlx::AssertSqlSafe(format!(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
                     WHERE datname = '{}' AND pid <> pg_backend_pid()",
                    self.db_name
                )))
                .await;
            let _ = admin
                .execute(sqlx::AssertSqlSafe(format!(
                    r#"DROP DATABASE IF EXISTS "{}""#,
                    self.db_name
                )))
                .await;
            admin.close().await;
        }
        self.pool.close().await;
    }
}

macro_rules! with_db {
    ($body:expr) => {
        async move {
            match TestDb::new().await {
                Some(db) => {
                    let db = Arc::new(db);
                    let outcome = tokio::spawn(std::panic::AssertUnwindSafe(
                        $body(db.pool.clone()),
                    ))
                    .await;
                    match Arc::try_unwrap(db) {
                        Ok(db) => db.cleanup().await,
                        Err(_) => eprintln!("warning: test db handle still shared"),
                    }
                    if let Err(join_err) = outcome {
                        if join_err.is_panic() {
                            std::panic::resume_unwind(join_err.into_panic());
                        }
                        eprintln!("test body did not complete: {join_err}");
                    }
                }
                None => eprintln!("(skipped: no database available)"),
            }
        }
    };
}

#[tokio::test]
async fn only_one_instance_holds_leadership() {
    with_db!(|pool: PgPool| async move {
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();

        let first = forge_scheduler::leader::try_acquire(&pool, first_id)
            .await
            .unwrap();
        assert!(first.is_some(), "the first instance must take leadership");
        let first = first.unwrap();
        assert_eq!(first.instance_id(), first_id);

        // The second must be refused while the first holds it. This is the
        // property that stops N replicas all running the same claim query.
        let second = forge_scheduler::leader::try_acquire(&pool, second_id)
            .await
            .unwrap();
        assert!(
            second.is_none(),
            "a second instance must not take leadership while one is held"
        );

        // Releasing hands it over.
        forge_scheduler::leader::release(first).await.unwrap();
        let second = forge_scheduler::leader::try_acquire(&pool, second_id)
            .await
            .unwrap();
        assert!(
            second.is_some(),
            "leadership must be acquirable once released"
        );
    })
    .await;
}

#[tokio::test]
async fn a_crashed_leader_is_replaced_without_waiting_for_an_expiry() {
    with_db!(|pool: PgPool| async move {
        let crashed = Uuid::new_v4();
        let survivor = Uuid::new_v4();

        {
            let crashed_leadership = forge_scheduler::leader::try_acquire(&pool, crashed)
                .await
                .unwrap()
                .expect("the first instance leads");
            let _keep_alive = crashed_leadership;
            assert!(forge_scheduler::leader::try_acquire(&pool, survivor)
                .await
                .unwrap()
                .is_none());
            // The guard drops at the end of this block without an explicit
            // release, which is exactly what a crashed leader leaves behind.
        }

        // A crashed leader is replaced as soon as its connection is reaped.
        // With a lease-based design there would be an expiry window here during
        // which nobody leads; the advisory lock has none.
        let mut acquired = None;
        for _ in 0..50 {
            if let Some(guard) = forge_scheduler::leader::try_acquire(&pool, survivor)
                .await
                .unwrap()
            {
                acquired = Some(guard);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            acquired.is_some(),
            "a surviving replica must be able to take over after a leader's \
             connection goes away"
        );
    })
    .await;
}

#[tokio::test]
async fn only_one_of_many_simultaneous_attempts_wins() {
    with_db!(|pool: PgPool| async move {
        // Ten replicas racing for the lock at the same instant. Exactly one may
        // win; a scheduler that could be led by two would double-generate.
        let mut handles = Vec::new();
        for _ in 0..10 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                forge_scheduler::leader::try_acquire(&pool, Uuid::new_v4())
                    .await
                    .unwrap()
                    .is_some()
            }));
        }

        let mut winners = 0;
        for handle in handles {
            if handle.await.unwrap() {
                winners += 1;
            }
        }
        assert_eq!(winners, 1, "exactly one instance may hold leadership");
    })
    .await;
}

#[tokio::test]
async fn leadership_does_not_block_other_advisory_locks() {
    with_db!(|pool: PgPool| async move {
        // The scheduler's key is namespaced, so a future feature taking its own
        // advisory lock must not be blocked by ours.
        let guard = forge_scheduler::leader::try_acquire(&pool, Uuid::new_v4())
            .await
            .unwrap()
            .expect("leadership acquired");

        let other: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(987_654_321_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(other, "an unrelated advisory lock must remain available");

        // And a different class within our own namespace too.
        let other_class: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
            .bind(0x4650_i32)
            .bind(0x0002_i32)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(other_class, "another key in the same class must be free");

        drop(guard);
    })
    .await;
}