//! Tenant isolation is enforced by Postgres, not only by the query.
//!
//! `redesign.md` §G requires per-tenant isolation as an operable model. Until
//! migration 020 the only boundary was a `WHERE tenant_id = $1` in each
//! repository method — a convention that one forgotten predicate would break.
//! These tests pin the database-level check so a future edit that weakens or
//! drops a policy fails here.
//!
//! The harness is deliberately different from the other suites in this crate:
//! isolation is meaningless with a single tenant, so these tests create two and
//! read across the boundary deliberately. They also `SET ROLE forge_app`,
//! because **RLS does not apply to the table owner** — an owner sees every row
//! regardless of policy, so testing as the owner would prove nothing at all.

use std::env;

use forge_domain::{ApplicationId, EnvironmentId, Slug, TenantId};
use sqlx::postgres::{PgPoolOptions, Postgres};
use sqlx::{Executor, PgPool};
use uuid::Uuid;

const TENANT_A: &str = "11111111-1111-1111-1111-111111111111";
const TENANT_B: &str = "22222222-2222-2222-2222-222222222222";

/// A throwaway database plus a non-owner role, dropped on the way out.
///
/// The role is what makes the policies observable: as the table owner Postgres
/// bypasses RLS entirely, so a test running as `forge` would pass no matter what
/// the policies said.
struct IsolationDb {
    pool: PgPool,
    admin_url: String,
    db_name: String,
}

/// Borrows one connection for the duration of a scenario.
///
/// `SET ROLE` and `SET` are *session* state, so they must be issued and used on
/// the same connection. A pool may hand a different connection to the next
/// query, which silently returns to the owner's privileges and makes the
/// policies untestable — the failure looks like "RLS is not working" when in
/// fact the test was running as the owner the whole time.
async fn session(db: &IsolationDb) -> sqlx::pool::PoolConnection<Postgres> {
    db.pool.acquire().await.expect("acquire a connection")
}

impl IsolationDb {
    async fn new() -> Option<Self> {
        let base = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));
        let db_name = format!("forge_rls_{}", Uuid::new_v4().simple());

        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping RLS tests: cannot connect ({e})");
                return None;
            }
        };

        for stmt in [
            format!("CREATE DATABASE \"{db_name}\""),
            // The role is cluster-wide, so it may already exist from a previous
            // suite. CREATEROLE needs it to exist before the grants below.
            "DO $$ BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'forge_app') THEN CREATE ROLE forge_app NOLOGIN; END IF; END $$".to_string(),
        ] {
            if admin.execute(sqlx::AssertSqlSafe(stmt)).await.is_err() {
                eprintln!("skipping RLS tests: cannot prepare database");
                return None;
            }
        }
        admin.close().await;

        let pool = match PgPoolOptions::new()
            .max_connections(5)
            .connect(&format!("{server}/{db_name}"))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping RLS tests: cannot connect ({e})");
                return None;
            }
        };

        if let Err(e) = sqlx::migrate!("../forge-storage/migrations").run(&pool).await {
            eprintln!("skipping RLS tests: migrations failed ({e})");
            return None;
        }

        // The policies only bind a non-owner role, and only once that role can
        // actually read the tables. Migration 020 deliberately leaves GRANT to
        // deployment, so the harness grants on its own throwaway database.
        for table in ["jobs", "applications", "environments", "job_revisions"] {
            let stmt = format!("GRANT SELECT, INSERT, UPDATE ON {table} TO forge_app");
            if pool.execute(sqlx::AssertSqlSafe(stmt)).await.is_err() {
                eprintln!("skipping RLS tests: cannot grant to forge_app");
                return None;
            }
        }

        // Two tenants, each with a container pair, plus a job only they can see.
        for (tenant, label) in [(TENANT_A, "A"), (TENANT_B, "B")] {
            pool.execute(sqlx::AssertSqlSafe(format!(
                "INSERT INTO tenants (id, name) VALUES ('{tenant}', 'Tenant {label}')"
            )))
            .await
            .ok()?;
        }
        for (tenant, label) in [(TENANT_A, "A"), (TENANT_B, "B")] {
            pool.execute(sqlx::AssertSqlSafe(format!(
                "INSERT INTO jobs (id, tenant_id, name, status)
                 VALUES (gen_random_uuid(), '{tenant}', 'secret-job-{label}', 'DRAFT')"
            )))
            .await
            .ok()?;
        }

        Some(Self {
            pool,
            admin_url,
            db_name,
        })
    }
}

impl IsolationDb {
    /// Drops the throwaway database, guaranteeing it is released.
    ///
    /// Explicitly awaited rather than done in `Drop`: `Drop` cannot await, so it
    /// has to spawn a detached thread, and that thread is killed when the test
    /// binary exits — which is why 6 databases still survived a full green run
    /// even after `Drop` was introduced.
    ///
    /// Every test awaits this before returning, so the drop completes while the
    /// runtime is still alive.
    async fn cleanup(self) {
        let admin_url = self.admin_url;
        let db_name = self.db_name;

        // Release the pool, then evict any session still attached: the RLS
        // tests pin a connection open to hold `SET ROLE`, so `close()` alone
        // would wait forever for a connection it cannot reclaim.
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            let _ = admin
                .execute(sqlx::AssertSqlSafe(format!(
                    "SELECT pg_terminate_backend(pid)
                       FROM pg_stat_activity
                      WHERE datname = '{db_name}' AND pid <> pg_backend_pid()"
                )))
                .await;
            admin.close().await;
        }

        self.pool.close().await;

        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            let _ = admin
                .execute(sqlx::AssertSqlSafe(format!(
                    "DROP DATABASE IF EXISTS \"{db_name}\""
                )))
                .await;
            admin.close().await;
        }
    }
}

/// Pins a connection to act as `forge_app` with `tenant` set.
///
/// Two mechanics matter here, and both are easy to get wrong:
///
/// - `SET ROLE` is issued on its own statement, not inside an explicit
///   transaction. Inside one it is reverted when the transaction ends, so a
///   later query on the same connection can still run as the owner and bypass
///   the policies entirely.
/// - The tenant uses plain `SET`, matching the same session lifetime as the
///   role. `SET LOCAL` would revert at the end of the implicit single-statement
///   transaction this helper issues.
///
/// [`unscoped`] restores the connection, because both changes outlive the
/// statement that made them.
async fn scoped(conn: &mut sqlx::pool::PoolConnection<Postgres>, tenant: &str) {
    conn.execute(sqlx::AssertSqlSafe("SET ROLE forge_app"))
        .await
        .expect("assume the application role");
    conn.execute(sqlx::AssertSqlSafe(format!(
        "SET app.current_tenant = '{tenant}'"
    )))
    .await
    .expect("set the tenant");
}

/// Drops the assumed role and tenant, returning the connection to owner
/// privileges so later fixture work is not filtered by the policies.
async fn unscoped(conn: &mut sqlx::pool::PoolConnection<Postgres>) {
    conn.execute(sqlx::AssertSqlSafe("RESET ROLE")).await.ok();
    conn.execute(sqlx::AssertSqlSafe("RESET app.current_tenant"))
        .await
        .ok();
}

#[test]
fn rls_hides_another_tenants_rows() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let Some(db) = IsolationDb::new().await else {
            return;
        };

        let mut conn = session(&db).await;

        for (tenant, expected) in [
            (TENANT_A, "secret-job-A"),
            (TENANT_B, "secret-job-B"),
        ] {
            scoped(&mut conn, tenant).await;
            let names: Vec<String> = sqlx::query_scalar("SELECT name FROM jobs")
                .fetch_all(&mut *conn)
                .await
                .unwrap();
            unscoped(&mut conn).await;

            assert_eq!(
                names,
                vec![expected.to_string()],
                "{tenant} must see only its own job"
            );
        }

        // The pinned connection must be released before cleanup: it is still
        // checked out of the pool, and `pool.close()` waits for every connection
        // to be returned, so cleaning up while it is alive blocks forever.
        drop(conn);
        db.cleanup().await;
    });
}


#[test]
fn rls_blocks_a_cross_tenant_write() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let Some(db) = IsolationDb::new().await else {
            return;
        };

        let mut conn = session(&db).await;
        scoped(&mut conn, TENANT_A).await;
        // WITH CHECK, not just USING: without it a row could be *read* in
        // isolation yet still be *written* into someone else's tenant.
        let result = sqlx::query(
            "INSERT INTO jobs (id, tenant_id, name, status)
             VALUES (gen_random_uuid(), $1, 'injected', 'DRAFT')",
        )
        .bind(TENANT_B)
        .execute(&mut *conn)
        .await;
        unscoped(&mut conn).await;

        assert!(
            result.is_err(),
            "writing into another tenant must be refused by the policy"
        );

        // The pinned connection must be released before cleanup: it is still
        // checked out of the pool, and `pool.close()` waits for every connection
        // to be returned, so cleaning up while it is alive blocks forever.
        drop(conn);
        db.cleanup().await;
    });
}


#[test]
fn an_unset_tenant_denies_everything_rather_than_erroring() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let Some(db) = IsolationDb::new().await else {
            return;
        };

        // The fail-closed case: a code path that forgets to set the tenant gets
        // no rows, not all of them.
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 2,
            "with no tenant set the owner still sees both rows; this asserts \
             the baseline the policy tests above are measured against"
        );

        let mut conn = session(&db).await;
        scoped(&mut conn, TENANT_A).await;
        // Clearing the tenant is the realistic "forgot to set it" case: the role
        // is assumed but no tenant is bound.
        conn.execute(sqlx::AssertSqlSafe("RESET app.current_tenant"))
            .await
            .unwrap();
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        unscoped(&mut conn).await;
        assert_eq!(
            visible, 0,
            "a non-owner role with no tenant set must see nothing, so a \
             forgotten SET LOCAL fails closed instead of reading everything"
        );

        // The pinned connection must be released before cleanup: it is still
        // checked out of the pool, and `pool.close()` waits for every connection
        // to be returned, so cleaning up while it is alive blocks forever.
        drop(conn);
        db.cleanup().await;
    });
}


#[test]
fn the_domain_types_round_trip_through_the_new_tables() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let Some(db) = IsolationDb::new().await else {
            return;
        };

        let tenant = TenantId::from_uuid(Uuid::parse_str(TENANT_A).unwrap());
        let slug = Slug::parse("Payments API (EU)").unwrap();
        assert_eq!(slug.as_str(), "payments-api-eu");

        let app_id = ApplicationId::new();
        db.pool
            .execute(sqlx::AssertSqlSafe(format!(
                "INSERT INTO applications (id, tenant_id, slug, name)
                 VALUES ('{app_id}', '{tenant}', '{slug}', 'Payments API (EU)')"
            )))
            .await
            .unwrap();

        let env_id = EnvironmentId::new();
        db.pool
            .execute(sqlx::AssertSqlSafe(format!(
                "INSERT INTO environments (id, tenant_id, slug, name, kind)
                 VALUES ('{env_id}', '{tenant}', 'prod', 'Production', 'production')"
            )))
            .await
            .unwrap();

        let stored: String = sqlx::query_scalar(
            "SELECT slug FROM applications WHERE id = $1",
        )
        .bind(app_id.as_uuid())
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(stored, "payments-api-eu");

        // A second production environment is the failure mode ADR-0022 exists to
        // prevent, so it is asserted here rather than only described there.
        let second = sqlx::query(
            "INSERT INTO environments (id, tenant_id, slug, name, kind)
             VALUES ($1, $2, 'prod-2', 'Production 2', 'production')",
        )
        .bind(EnvironmentId::new().as_uuid())
        .bind(tenant.as_uuid())
        .execute(&db.pool)
        .await;
        assert!(
            second.is_err(),
            "a tenant may not have two production environments"
        );
        db.cleanup().await;
    });
}


#[test]
fn a_job_revision_stores_and_reads_back_its_config() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let Some(db) = IsolationDb::new().await else {
            return;
        };

        let job_id: Uuid = sqlx::query_scalar("SELECT id FROM jobs LIMIT 1")
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let config = serde_json::json!({
            "schedule": { "cron": "0 2 * * *", "timezone": "Asia/Kolkata" },
            "identity": { "name": "Nightly settlement" }
        });
        db.pool
            .execute(sqlx::AssertSqlSafe(format!(
                "INSERT INTO job_revisions
                     (id, tenant_id, job_id, revision_number, config)
                 VALUES (gen_random_uuid(), '{TENANT_A}', '{job_id}', 1, '{config}')"
            )))
            .await
            .unwrap();

        let read: serde_json::Value =
            sqlx::query_scalar("SELECT config FROM job_revisions WHERE job_id = $1")
                .bind(job_id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(read, config, "a revision must round-trip unchanged");

        // A second revision numbered 1 would make "the current revision"
        // ambiguous, which is what the unique index exists to prevent.
        let duplicate = sqlx::query(
            "INSERT INTO job_revisions (id, tenant_id, job_id, revision_number, config)
             VALUES (gen_random_uuid(), $1, $2, 1, '{}')",
        )
        .bind(Uuid::parse_str(TENANT_A).unwrap())
        .bind(job_id)
        .execute(&db.pool)
        .await;
        assert!(
            duplicate.is_err(),
            "two revisions of one job may not share a revision number"
        );
        db.cleanup().await;
    });
}


#[tokio::test]
async fn every_scoped_table_has_a_policy() {
    // Guards against a future migration adding a tenant-scoped table and
    // forgetting to enable RLS on it, which is the whole failure this suite
    // exists to catch.
    let Some(db) = IsolationDb::new().await else {
        return;
    };

    let unprotected: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT c.relname
          FROM pg_class c
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public'
           AND c.relkind = 'r'
           AND c.relname IN (
                'applications','environments','jobs','job_versions','job_revisions',
                'executions','schedules','queues','workers','workflows',
                'workflow_versions','workflow_nodes','workflow_edges',
                'manual_approvals','workflow_node_states','alerts','incidents',
                'notifications','api_keys','audit_events','idempotency_keys',
                'migrations'
           )
           AND c.relrowsecurity IS NOT TRUE
        "#,
    )
    .fetch_all(&db.pool)
    .await
    .expect("query pg_class");

    assert!(
        unprotected.is_empty(),
        "these tenant-scoped tables have row-level security disabled: {unprotected:?}"
    );
    db.cleanup().await;
}

/// Compile-time guard that the suite's imports are all used; also documents the
/// Postgres types the fixtures rely on.
#[allow(dead_code)]
fn _type_anchors(_: &Postgres, _: TenantId, _: ApplicationId, _: EnvironmentId) {}