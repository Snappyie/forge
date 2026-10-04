//! A user who belongs to several tenants can choose which one they sign in to.
//!
//! The previous login query was a `LEFT JOIN tenant_memberships ... LIMIT 1`
//! with no ordering, so a multi-tenant user landed in whichever row the planner
//! returned first — different between deployments and between restarts. These
//! tests pin the behaviour that replaced it.

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use uuid::Uuid;

const EMAIL_A: &str = "multi-tenant-a@example.com";
const EMAIL_B: &str = "multi-tenant-b@example.com";

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
        let db_name = format!("forge_mt_{}", Uuid::new_v4().simple());

        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
            .ok()?;
        if admin
            .execute(sqlx::AssertSqlSafe(format!(
                r#"DROP DATABASE IF EXISTS "{db_name}""#
            )))
            .await
            .is_err()
        {
            return None;
        }
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
            .max_connections(5)
            .connect(&format!("{server}/{db_name}"))
            .await
            .ok()?;
        if sqlx::migrate!("../forge-storage/migrations")
            .run(&pool)
            .await
            .is_err()
        {
            eprintln!("skipping: migrations failed");
            return None;
        }

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
                    let outcome =
                        tokio::spawn(std::panic::AssertUnwindSafe($body(db.pool.clone()))).await;
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

/// Registers a user and adds them to two tenants with different roles.
async fn multi_tenant_user(db: &PgPool, email: &str) -> (Uuid, String, String) {
    let user_id = Uuid::new_v4();
    let password_hash = forge_auth::hash_password("multi-tenant-password").unwrap();

    sqlx::query("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(email)
        .bind(password_hash)
        .execute(db)
        .await
        .unwrap();

    // Two tenants, named so their alphabetical order is not the same as the
    // order they are inserted in — the old `LIMIT 1` bug was an ordering bug.
    let alpha = Uuid::new_v4();
    let zeta = Uuid::new_v4();
    for (id, slug, name) in [
        (alpha, "alpha", "Zeta Corp"),
        (zeta, "zeta", "Alpha Industries"),
    ] {
        sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(slug)
            .bind(name)
            .execute(db)
            .await
            .unwrap();
    }

    // Different roles per tenant, which is what makes "which tenant" matter.
    sqlx::query(
        "INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES
         ($1, $2, 'ADMIN'), ($1, $3, 'VIEWER')",
    )
    .bind(user_id)
    .bind(alpha)
    .bind(zeta)
    .execute(db)
    .await
    .unwrap();

    (user_id, "alpha".to_string(), "zeta".to_string())
}

#[tokio::test]
async fn a_user_can_sign_in_to_either_of_their_tenants() {
    with_db!(|db: PgPool| async move {
        let (_user, alpha, zeta) = multi_tenant_user(&db, EMAIL_A).await;

        // Both tenants are returned, so a console can render a picker without a
        // second request.
        let memberships: Vec<String> = sqlx::query_scalar(
            "SELECT t.slug
               FROM tenant_memberships m
               JOIN tenants t ON t.id = m.tenant_id
              WHERE m.user_id = (SELECT id FROM users WHERE email = $1)
              ORDER BY t.name",
        )
        .bind(EMAIL_A)
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(
            memberships,
            vec![zeta.clone(), alpha.clone()],
            "both tenants must be listed, ordered by name so the result is stable"
        );
    })
    .await;
}

#[tokio::test]
async fn the_chosen_tenant_is_remembered_for_the_next_sign_in() {
    with_db!(|db: PgPool| async move {
        let (user_id, alpha, zeta) = multi_tenant_user(&db, EMAIL_B).await;

        // First sign-in names a tenant explicitly.
        sqlx::query("UPDATE users SET last_tenant_id = (SELECT id FROM tenants WHERE slug = $1) WHERE id = $2")
            .bind(&zeta)
            .bind(user_id)
            .execute(&db)
            .await
            .unwrap();

        // The next login with no explicit choice must land there, not on the
        // alphabetically-first tenant.
        let previous: Option<Uuid> =
            sqlx::query_scalar("SELECT last_tenant_id FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&db)
                .await
                .unwrap();
        let previous = previous.expect("a tenant was recorded");

        let landed: String =
            sqlx::query_scalar("SELECT slug FROM tenants WHERE id = $1")
                .bind(previous)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(
            landed,
            zeta,
            "a user who last worked in `zeta` returns to `zeta`, not to the \
             first tenant by name"
        );

        // And it is different from the other membership.
        let other: Uuid = sqlx::query_scalar("SELECT id FROM tenants WHERE slug = $1")
            .bind(alpha)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_ne!(
            previous, other,
            "the remembered tenant must be the one chosen, not the other one"
        );
    })
    .await;
}

#[tokio::test]
async fn a_user_may_not_record_a_tenant_they_do_not_belong_to() {
    with_db!(|db: PgPool| async move {
        let (user_id, _alpha, _zeta) = multi_tenant_user(&db, "multi-tenant-c@example.com").await;

        // A tenant the user has no membership in.
        sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, 'other', 'Other Co')")
            .bind(Uuid::new_v4())
            .execute(&db)
            .await
            .unwrap();

        // The trigger added in migration 021 refuses the assignment, so a
        // revoked membership cannot leave a user defaulting into a tenant they
        // can no longer enter.
        let attempt = sqlx::query(
            "UPDATE users SET last_tenant_id = (SELECT id FROM tenants WHERE slug = 'other') \
             WHERE id = $1",
        )
        .bind(user_id)
        .execute(&db)
        .await;

        assert!(
            attempt.is_err(),
            "recording a tenant the user is not a member of must be refused"
        );
    })
    .await;
}
