use forge_domain::{
    Execution, ExecutionStatus, Job, JobStatus, TenantId, JobVersionId
};

fn main() {
    println!("--- Testing Forge Domain Models ---\n");

    // 1. Create a tenant ID
    let tenant_id = TenantId::new();
    println!("Created Tenant ID: {}", tenant_id);

    // 2. Create a new Job
    let mut job = Job::new(tenant_id, "Nightly Database Backup".to_string());
    println!("Created Job: {} (Status: {:?})", job.name, job.status);

    // 3. Try to activate the job without a version (Should fail)
    match job.transition_to(JobStatus::Active) {
        Ok(_) => println!("Successfully activated job (Wait, this shouldn't happen!)"),
        Err(e) => println!("Expected error when activating job: {}", e),
    }

    // 4. Assign a mock version and activate (Should succeed)
    job.current_version_id = Some(JobVersionId::new());
    match job.transition_to(JobStatus::Active) {
        Ok(_) => println!("Successfully activated job! New status: {:?}", job.status),
        Err(e) => println!("Failed to activate job: {}", e),
    }
    
    println!("\n--- Testing Executions ---");

    // 5. Create an Execution for the job
    let mut execution = Execution::new(tenant_id, job.id, job.current_version_id.unwrap());
    println!("Created Execution: {} (Status: {:?})", execution.id, execution.status);

    // 6. Transition Execution: Queued -> Dispatched -> Running -> Succeeded
    println!("Dispatching...");
    execution.transition_to(ExecutionStatus::Dispatched).unwrap();
    
    println!("Running...");
    execution.transition_to(ExecutionStatus::Running).unwrap();
    
    println!("Succeeding...");
    execution.transition_to(ExecutionStatus::Succeeded).unwrap();
    println!("Final Execution Status: {:?}", execution.status);

    // 7. Try an invalid transition: Succeeded -> Running (Should fail)
    match execution.transition_to(ExecutionStatus::Running) {
        Ok(_) => println!("Transitioned to running (Wait, this shouldn't happen!)"),
        Err(e) => println!("Expected error on terminal state transition: {}", e),
    }
}