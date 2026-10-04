//! Integration tests for the outbox publisher (spec 04.8, AT-REC-005).

use std::sync::Arc;

use forge_events::{DomainEvent, InMemorySink, LogSink, OutboxPublisher, PublishStats, SinkError};
use forge_storage::OutboxRepository;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use uuid::Uuid;

struct TestDb {
    pool: PgPool,
    admin_url: String,
    db_name: String,
}

impl TestDb {
    async fn cleanup(self) {
        self.pool.close().await;
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            let _ = admin
                .execute(sqlx::AssertSqlSafe(format!(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
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
    }

    async fn new() -> Option<TestDb> {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let db_name = format!("forge_evts_{}", Uuid::new_v4().simple());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping events integration tests: cannot connect ({e})");
                return None;
            }
        };
        if admin
            .execute(sqlx::AssertSqlSafe(format!(
                r#"CREATE DATABASE "{db_name}""#
            )))
            .await
            .is_err()
        {
            eprintln!("skipping events integration tests: cannot create database");
            return None;
        }
        admin.close().await;

        let pool = match PgPoolOptions::new()
            .max_connections(10)
            .connect(&format!("{server}/{db_name}"))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping events integration tests: cannot connect ({e})");
                return None;
            }
        };
        if let Err(e) = sqlx::migrate!("../forge-storage/migrations")
            .run(&pool)
            .await
        {
            eprintln!("skipping events integration tests: migrations failed ({e})");
            return None;
        }

        Some(TestDb {
            pool,
            admin_url,
            db_name,
        })
    }
}

macro_rules! with_db {
    ($body:expr) => {
        async move {
            match TestDb::new().await {
                Some(db) => {
                    let db = Arc::new(db);
                    $body(db.pool.clone()).await;
                    match Arc::try_unwrap(db) {
                        Ok(db) => db.cleanup().await,
                        Err(_) => eprintln!("warning: test db handle still shared"),
                    }
                }
                None => eprintln!("(skipped: no database available)"),
            }
        }
    };
}

/// Enqueues `count` events and returns the repository.
async fn enqueue(pool: &PgPool, count: usize) -> OutboxRepository<'_> {
    let repo = OutboxRepository::new(pool);
    for i in 0..count {
        repo.enqueue(
            None,
            "execution.succeeded",
            "execution",
            Uuid::new_v4(),
            serde_json::json!({ "index": i }),
        )
        .await
        .expect("enqueue");
    }
    repo
}

#[tokio::test]
async fn the_publisher_delivers_pending_events() {
    with_db!(|pool: PgPool| async move {
        let repo = enqueue(&pool, 3).await;
        let sink = InMemorySink::new();
        let publisher = OutboxPublisher::new(&pool, 100);

        let stats = publisher.publish_batch(&sink).await;

        assert_eq!(stats.claimed, 3);
        assert_eq!(stats.published, 3);
        assert_eq!(stats.failed, 0);
        assert_eq!(sink.len(), 3);

        // Nothing is left to publish.
        assert!(repo.claim_unpublished(100).await.unwrap().is_empty());
    })
    .await;
}

#[tokio::test]
async fn a_second_pass_has_nothing_to_do() {
    with_db!(|pool: PgPool| async move {
        enqueue(&pool, 2).await;
        let sink = InMemorySink::new();
        let publisher = OutboxPublisher::new(&pool, 100);

        publisher.publish_batch(&sink).await;
        let second = publisher.publish_batch(&sink).await;

        assert_eq!(second.claimed, 0);
        assert_eq!(sink.len(), 2, "events are delivered exactly once");
    })
    .await;
}

// AT-REC-005: an outbox publisher restart does not lose events.
#[tokio::test]
async fn a_publisher_restart_resumes_unpublished_events() {
    with_db!(|pool: PgPool| async move {
        let repo = enqueue(&pool, 3).await;

        // First publisher claims but fails before marking anything published.
        let failing = InMemorySink::new();
        failing.fail_with(Some(SinkError::Transient("broker unreachable".into())));
        let stats = OutboxPublisher::new(&pool, 100)
            .publish_batch(&failing)
            .await;
        assert_eq!(stats.failed, 3);
        assert!(failing.is_empty());

        // The rows are still claimable, so a restarted publisher picks them up.
        let recovered = InMemorySink::new();
        let stats = OutboxPublisher::new(&pool, 100)
            .publish_batch(&recovered)
            .await;
        assert_eq!(stats.published, 3, "no event is lost across the restart");
        assert_eq!(recovered.len(), 3);

        assert!(repo.claim_unpublished(100).await.unwrap().is_empty());
    })
    .await;
}

#[tokio::test]
async fn a_failed_event_records_the_error_for_diagnosis() {
    with_db!(|pool: PgPool| async move {
        enqueue(&pool, 1).await;

        let failing = InMemorySink::new();
        failing.fail_with(Some(SinkError::Transient("timeout".into())));
        OutboxPublisher::new(&pool, 100)
            .publish_batch(&failing)
            .await;

        let pending = OutboxRepository::new(&pool)
            .claim_unpublished(10)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
        let error = pending[0].last_error.as_deref().unwrap_or("");
        assert!(
            error.contains("timeout"),
            "the failure reason must be visible to an operator: {error}"
        );
        assert!(
            pending[0].attempt_count >= 2,
            "retries are counted, not silently repeated"
        );
    })
    .await;
}

#[tokio::test]
async fn an_event_is_abandoned_after_exhausting_its_attempts() {
    with_db!(|pool: PgPool| async move {
        enqueue(&pool, 1).await;

        // Push the attempt counter past the ceiling.
        sqlx::query("UPDATE outbox_events SET attempt_count = 99")
            .execute(&pool)
            .await
            .unwrap();

        let sink = InMemorySink::new();
        let stats = OutboxPublisher::new(&pool, 100)
            .with_max_attempts(10)
            .publish_batch(&sink)
            .await;

        assert_eq!(stats.abandoned, 1);
        assert_eq!(stats.published, 0);
        assert!(sink.is_empty(), "an abandoned event is not delivered");
        // The repository is exercised through the publisher.
    })
    .await;
}

#[tokio::test]
async fn the_batch_size_bounds_a_publish_pass() {
    with_db!(|pool: PgPool| async move {
        enqueue(&pool, 5).await;
        let sink = InMemorySink::new();

        let first = OutboxPublisher::new(&pool, 2).publish_batch(&sink).await;
        assert_eq!(first.claimed, 2);
        assert_eq!(sink.len(), 2);

        let second = OutboxPublisher::new(&pool, 100).publish_batch(&sink).await;
        assert_eq!(second.published, 3);
        assert_eq!(sink.len(), 5);
    })
    .await;
}

#[tokio::test]
async fn the_log_sink_accepts_everything() {
    with_db!(|pool: PgPool| async move {
        enqueue(&pool, 1).await;
        let stats = OutboxPublisher::new(&pool, 10)
            .publish_batch(&LogSink)
            .await;
        assert_eq!(stats.published, 1);
    })
    .await;
}

#[tokio::test]
async fn the_run_loop_stops_when_asked() {
    with_db!(|pool: PgPool| async move {
        let sink = Arc::new(InMemorySink::new());
        // The publisher borrows the pool, so it is created inside the spawned
        // task rather than moved into it.
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let publisher = OutboxPublisher::new(&pool, 10);
            publisher
                .run(sink, std::time::Duration::from_millis(10), async move {
                    let _ = rx.await;
                })
                .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        tx.send(()).ok();
        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("the publisher loop must stop promptly")
            .expect("the task must not panic");
    })
    .await;
}

#[tokio::test]
async fn publishing_nothing_is_a_no_op() {
    with_db!(|pool: PgPool| async move {
        let sink = InMemorySink::new();
        let stats: PublishStats = OutboxPublisher::new(&pool, 100).publish_batch(&sink).await;
        assert_eq!(stats.claimed, 0);
        assert_eq!(stats.published, 0);
        assert!(sink.is_empty());
    })
    .await;
}

#[tokio::test]
async fn the_payload_survives_the_round_trip() {
    with_db!(|pool: PgPool| async move {
        let repo = OutboxRepository::new(&pool);
        repo.enqueue(
            None,
            "execution.failed",
            "execution",
            Uuid::new_v4(),
            serde_json::json!({ "error_class": "TIMEOUT", "attempt": 3 }),
        )
        .await
        .unwrap();

        let claimed = repo.claim_unpublished(10).await.unwrap();
        assert_eq!(claimed[0].payload["error_class"], "TIMEOUT");

        // The same fields reconstruct a domain event for delivery.
        let event = DomainEvent {
            id: claimed[0].id,
            event_type: claimed[0].event_type.clone(),
            aggregate_type: claimed[0].aggregate_type.clone(),
            aggregate_id: claimed[0].aggregate_id,
            tenant_id: claimed[0].tenant_id,
            occurred_at: claimed[0].created_at,
            payload: claimed[0].payload.clone(),
            correlation_id: None,
        };
        assert_eq!(event.to_envelope()["data"]["attempt"], 3);
    })
    .await;
}
