use forge_domain::{Execution, ExecutionStatus, Job, JobStatus, TenantId, JobVersionId};
use forge_scheduler::CronSchedule;
use forge_storage::db::Database;
use chrono::Utc;
use std::env;

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

    println!("\n=== All Tests Passed! ===");
    Ok(())
}
