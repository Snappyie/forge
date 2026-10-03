use forge_domain::{Execution, ExecutionStatus, Job, JobStatus, TenantId, JobVersionId};
use forge_scheduler::CronSchedule;
use forge_storage::db::Database;
use forge_executor::{Worker, Lease};
use chrono::Utc;
use std::env;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Forge End-to-End Components ===\n");

    // 1. Test Domain Models (Phase 1)
    println!("--- 1. Domain Models ---");
    let tenant_id = TenantId::new();
    let mut job = Job::new(tenant_id, "Monthly Report Generation".to_string());
    job.current_version_id = Some(JobVersionId::new());
    job.transition_to(JobStatus::Active)?;
    println!("Successfully created and activated Job: {} (Status: {:?})", job.name, job.status);

    let mut execution = Execution::new(tenant_id, job.id, job.current_version_id.unwrap());
    execution.transition_to(ExecutionStatus::Dispatched)?;
    execution.transition_to(ExecutionStatus::Running)?;
    execution.transition_to(ExecutionStatus::Succeeded)?;
    println!("Execution ran and transitioned to final state: {:?}\n", execution.status);

    // 2. Test Scheduler Cron Parsing (Phase 2)
    println!("--- 2. Scheduler Cron Parsing ---");
    let cron_expression = "0 0 12 * * * *"; // Every day at 12:00 PM
    let schedule = CronSchedule::parse(cron_expression)?;
    
    let now = Utc::now();
    if let Some(next) = schedule.next_run_at(now) {
        println!("Current time: {}", now);
        println!("Schedule: {}", cron_expression);
        println!("Next run at: {}\n", next);
    }

    // 3. Test Database Connection & Migrations (Phase 2)
    println!("--- 3. Database Connection & Migrations ---");
    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".to_string());
    
    println!("Connecting to database and running migrations (URL: {})...", db_url);
    let db = Database::new(&db_url).await?;
    println!("Database connected and all migrations ran successfully!");
    
    // Test the pool is active
    let row: (i32,) = sqlx::query_as("SELECT 1")
        .fetch_one(db.pool())
        .await?;
    println!("Simple query 'SELECT 1' returned: {}", row.0);

    // 4. Test Executor / Worker Protocol (Phase 3)
    println!("\n--- 4. Executor / Worker Protocol ---");
    let mut worker = Worker::new(tenant_id, "node-1.forge.internal".to_string(), vec!["docker".to_string()]);
    println!("Registered Worker: {} on host {} (Status: {:?})", worker.id, worker.hostname, worker.status);
    
    // Simulate heartbeat
    thread::sleep(Duration::from_millis(50));
    worker.heartbeat();
    println!("Heartbeat sent! Last heartbeat at: {}", worker.last_heartbeat_at);

    // Issue a lease
    let mut lease = Lease::new(execution.id, worker.id, 2); // 2 second lease
    println!("Issued Lease for Execution {} to Worker {} (Expires: {})", lease.execution_id, lease.worker_id, lease.expires_at);
    println!("Is lease expired? {}", lease.is_expired());
    
    // Renew lease
    lease.renew(30);
    println!("Renewed lease for 30s. New expiration: {}", lease.expires_at);

    // 5. Test HTTP API (Phase 4)
    println!("\n--- 5. HTTP API & Endpoints ---");
    let router = forge_api::create_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    
    // Spawn server in background
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    println!("Started Axum API server on port {}", port);
    
    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{}/api/v1/health/live", port);
    let res = client.get(&health_url).send().await?;
    let status = res.status();
    let body = res.text().await?;
    
    println!("GET /api/v1/health/live returned {}: {}", status, body);

    println!("\n=== All Tests Passed! ===");
    Ok(())
}
