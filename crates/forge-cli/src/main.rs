use clap::{Parser, Subcommand};
use reqwest::Client;
use std::env;

#[derive(Parser)]
#[command(name = "forge")]
#[command(about = "Forge Execution Engine CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check API health status
    Health,
    /// Job management
    Jobs {
        #[command(subcommand)]
        action: JobCommands,
    },
}

#[derive(Subcommand)]
enum JobCommands {
    /// List all active jobs
    List,
    /// Create a new job
    Create {
        /// Name of the job
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv(); // Load .env if present

    let cli = Cli::parse();
    
    // Fallback to localhost if not specified in environment
    let api_url = env::var("FORGE_API_URL")
        .unwrap_or_else(|_| "http://localhost:3000/api/v1".to_string());
    
    let client = Client::new();

    match &cli.command {
        Commands::Health => {
            println!("Checking Forge API health at {}", api_url);
            let url = format!("{}/health/live", api_url);
            let res = client.get(&url).send().await?;
            
            if res.status().is_success() {
                let body: serde_json::Value = res.json().await?;
                println!("✅ Status: OK\n{}", serde_json::to_string_pretty(&body)?);
            } else {
                println!("❌ Error: Received status code {}", res.status());
            }
        }
        Commands::Jobs { action } => {
            match action {
                JobCommands::List => {
                    let url = format!("{}/jobs", api_url);
                    let res = client.get(&url).send().await?;
                    if res.status().is_success() {
                        let body: serde_json::Value = res.json().await?;
                        println!("📦 Jobs List:\n{}", serde_json::to_string_pretty(&body)?);
                    } else {
                        println!("❌ Error: {}", res.status());
                    }
                }
                JobCommands::Create { name } => {
                    let url = format!("{}/jobs", api_url);
                    
                    // Generate a dummy tenant ID for now or it could be passed as an argument
                    let tenant_id = "tenant_123456789".to_string();

                    let payload = serde_json::json!({
                        "name": name,
                        "tenant_id": tenant_id
                    });

                    let res = client.post(&url).json(&payload).send().await?;
                    
                    if res.status().is_success() {
                        let body: serde_json::Value = res.json().await?;
                        println!("✅ Job Created Successfully:\n{}", serde_json::to_string_pretty(&body)?);
                    } else {
                        println!("❌ Error creating job: {}", res.status());
                        let error_text = res.text().await?;
                        println!("Details: {}", error_text);
                    }
                }
            }
        }
    }

    Ok(())
}
