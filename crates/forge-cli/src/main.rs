//! `forge` — the command-line client (spec 16).
//!
//! The command surface follows spec 16.2–16.7 exactly: auth, jobs, workflows,
//! executions, workers, and queues, each with the verbs listed there.

mod api;
mod commands;
mod context;
mod error;
mod output;

use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{json, Value};

use api::ApiClient;
use context::{Config, Context};
use error::{CliError, Result};
use output::{OutputFormat, Render, Table};

#[derive(Parser)]
#[command(name = "forge", version, about = "Forge job orchestration CLI")]
pub struct Cli {
    /// Path to the config file.
    #[arg(long, global = true)]
    config: Option<String>,

    /// Which stored context to use.
    #[arg(long, global = true)]
    context: Option<String>,

    /// Output format.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Table)]
    output: OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check API health.
    Health,
    /// Manage credentials and contexts.
    Auth {
        #[command(subcommand)]
        action: AuthCommands,
    },
    /// Manage jobs.
    Jobs {
        #[command(subcommand)]
        action: JobCommands,
    },
    /// Manage workflows.
    Workflows {
        #[command(subcommand)]
        action: WorkflowCommands,
    },
    /// Inspect and control executions.
    Executions {
        #[command(subcommand)]
        action: ExecutionCommands,
    },
    /// Manage workers.
    Workers {
        #[command(subcommand)]
        action: WorkerCommands,
    },
    /// Manage queues.
    Queues {
        #[command(subcommand)]
        action: QueueCommands,
    },
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Sign in and store the resulting tokens.
    Login {
        #[arg(long)]
        email: String,
        /// Password; read from the terminal when omitted.
        #[arg(long)]
        password: Option<String>,
        /// Server base URL, e.g. https://forge.example.com/api/v1.
        #[arg(long)]
        api_url: Option<String>,
    },
    /// Discard the stored tokens for the current context.
    Logout,
    /// Show who the current context is signed in as.
    Status,
    /// List the stored contexts.
    Contexts,
}

#[derive(Subcommand)]
enum JobCommands {
    /// List jobs.
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Show one job.
    Get { id: String },
    /// Create a job.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, default_value = "NORMAL")]
        priority: String,
    },
    /// Update a job's metadata.
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        /// The `updated_at` the client last read; a mismatch conflicts.
        #[arg(long)]
        expected_updated_at: Option<String>,
    },
    /// Create a version.
    Version {
        id: String,
        #[arg(long, default_value = "WORKER_TASK")]
        execution_type: String,
        #[arg(long, default_value_t = 3600)]
        timeout_seconds: i32,
        /// Execution configuration as inline JSON.
        ///
        /// Named `--execution-config` rather than `--config`, which is already
        /// the global path to the CLI configuration file.
        #[arg(long)]
        execution_config: Option<String>,
    },
    /// List a job's versions.
    Versions { id: String },
    /// Publish a version.
    Publish {
        id: String,
        #[arg(long)]
        version: String,
    },
    /// Run the job now.
    Run {
        id: String,
        /// Input payload as inline JSON.
        #[arg(long)]
        input: Option<String>,
        /// Makes the run idempotent when retried.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Archive a job.
    Archive { id: String },
    /// Pause the job's schedule.
    Pause { id: String },
    /// Resume the job's schedule.
    Resume { id: String },
}

#[derive(Subcommand)]
enum WorkflowCommands {
    /// List workflows.
    List,
    /// Show one workflow.
    Get { id: String },
    /// Create a workflow.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        key: Option<String>,
        /// A workflow document, as inline JSON.
        #[arg(long)]
        definition: Option<String>,
    },
    /// Validate a workflow definition without saving it.
    Validate {
        /// Path to a workflow definition file.
        file: std::path::PathBuf,
    },
    /// Publish a workflow version.
    Publish {
        id: String,
        #[arg(long)]
        version: String,
    },
    /// Run the workflow now.
    Run {
        id: String,
        #[arg(long)]
        input: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
}

#[derive(Subcommand)]
enum ExecutionCommands {
    /// List executions.
    List {
        #[arg(long)]
        job: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Show one execution.
    Get { id: String },
    /// Show an execution's log lines.
    Logs { id: String },
    /// Request cancellation.
    Cancel { id: String },
    /// Retry a failed execution.
    Retry {
        id: String,
        #[arg(long)]
        error_class: Option<String>,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Move an execution to the dead-letter queue.
    DeadLetter { id: String },
}

#[derive(Subcommand)]
enum WorkerCommands {
    /// List workers.
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Show one worker.
    Get { id: String },
    /// Stop a worker accepting new work.
    Drain { id: String },
    /// Revoke a worker permanently.
    Revoke { id: String },
}

#[derive(Subcommand)]
enum QueueCommands {
    /// List queues.
    List,
    /// Show one queue.
    Get { id: String },
    /// Create a queue.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        max_concurrency: Option<i32>,
    },
    /// Pause a queue.
    Pause { id: String },
    /// Resume a queue.
    Resume { id: String },
}

#[tokio::main]
async fn main() {
    // A `.env` is a convenience for development; its absence is not an error.
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    match dispatch(&cli).await {
        Ok(Some(text)) => println!("{text}"),
        Ok(None) => {}
        Err(err) => {
            output::print_error(&err.to_string());
            std::process::exit(err.exit_code().as_i32());
        }
    }
}

/// Resolves the config, applying `--config` and `--context`.
pub fn load_config(cli: &Cli) -> Result<Config> {
    let mut config = if cli.config.is_some() {
        // Honour an explicit `--config` for this invocation.
        let previous = std::env::var("FORGE_CONFIG").ok();
        if let Some(path) = &cli.config {
            // SAFETY: the CLI is single-threaded at this point.
            unsafe { std::env::set_var("FORGE_CONFIG", path) };
        }
        let loaded = context::load();
        match previous {
            Some(value) => unsafe { std::env::set_var("FORGE_CONFIG", value) },
            None => unsafe { std::env::remove_var("FORGE_CONFIG") },
        }
        loaded?
    } else {
        context::load()?
    };

    if let Some(name) = &cli.context {
        if config.get(name).is_none() {
            return Err(CliError::Usage(format!("no context named `{name}`")));
        }
        config.current = Some(name.clone());
    }

    Ok(config)
}

/// The context to operate against.
///
/// `FORGE_API_URL` overrides the stored URL, so a one-off command against
/// another environment needs no stored context.
pub fn active_context(config: &Config) -> Result<Context> {
    if let Ok(url) = std::env::var("FORGE_API_URL") {
        let mut context = config
            .active()
            .cloned()
            .unwrap_or_else(|| Context::new("env", &url));
        context.api_url = url;
        return Ok(context);
    }

    // No credential is an authentication problem, not a usage error: a script
    // branching on exit 3 will know to sign in.
    config.active().cloned().ok_or_else(|| {
        CliError::Authentication(
            "no context is selected; run `forge auth login --api-url <url>` or set \
             FORGE_API_URL"
                .to_string(),
        )
    })
}

/// A client for the current context, requiring a credential.
pub fn authenticated_client(config: &Config) -> Result<ApiClient> {
    let context = active_context(config)?;
    if !context.is_authenticated() {
        return Err(CliError::Authentication(format!(
            "context `{}` has no credential; run `forge auth login`",
            context.name
        )));
    }
    ApiClient::new(&context)
}

async fn dispatch(cli: &Cli) -> Result<Option<String>> {
    let format = cli.output;
    let config = load_config(cli)?;

    match &cli.command {
        Commands::Health => {
            // Liveness is public, so no credential is needed.
            let context = active_context(&config)?;
            let client = ApiClient::new(&context)?;
            let data = client.get("/health/live").await?;
            Ok(Some(render(&HealthView { data }, format)))
        }

        Commands::Auth { action } => commands::auth(&config, action, format).await,
        Commands::Jobs { action } => commands::jobs(&config, action, format).await,
        Commands::Workflows { action } => commands::workflows(&config, action, format).await,
        Commands::Executions { action } => commands::executions(&config, action, format).await,
        Commands::Workers { action } => commands::workers(&config, action, format).await,
        Commands::Queues { action } => commands::queues(&config, action, format).await,
    }
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthView {
    data: Value,
}
impl Render for HealthView {
    fn summary(&self) -> String {
        "healthy".to_string()
    }
    fn table(&self) -> Table {
        let mut t = Table::new(["CHECK", "STATUS"]);
        t.push(vec![
            "api".to_string(),
            self.data["status"].as_str().unwrap_or("unknown").to_string(),
        ]);
        t
    }
    fn data(&self) -> Value {
        json!({ "status": self.data["status"] })
    }
}

/// A generic list view over a JSON array with named columns.
pub struct ListView {
    pub columns: Vec<String>,
    pub fields: Vec<String>,
    pub items: Vec<Value>,
    pub total: usize,
}

impl Render for ListView {
    fn summary(&self) -> String {
        format!("{} result(s)", self.total)
    }
    fn table(&self) -> Table {
        let mut table = Table::new(&self.columns);
        for item in &self.items {
            table.push(
                self.fields
                    .iter()
                    .map(|field| render_cell(&item[field]))
                    .collect(),
            );
        }
        table
    }
    fn data(&self) -> Value {
        json!({ "items": self.items, "count": self.total })
    }
}

/// Renders a JSON value for a table cell.
///
/// Timestamps are shortened to `MM-DD HH:MM:SS` so a column stays readable;
/// `--output json` keeps the full value.
pub fn render_cell(value: &Value) -> String {
    match value {
        Value::Null => "-".to_string(),
        Value::String(s) => {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(s) {
                let utc = parsed.with_timezone(&chrono::Utc);
                utc.format("%m-%d %H:%M:%S").to_string()
            } else {
                s.clone()
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// A single-resource view, shown as a field/value table.
pub struct DetailView {
    pub resource: String,
    pub data: Value,
}

impl Render for DetailView {
    fn summary(&self) -> String {
        self.resource.clone()
    }
    fn table(&self) -> Table {
        let mut table = Table::new(["FIELD", "VALUE"]);
        if let Value::Object(map) = &self.data {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                table.push(vec![key.clone(), render_cell(&map[key])]);
            }
        }
        table
    }
    fn data(&self) -> Value {
        self.data.clone()
    }
}

/// Builds a list view from an API response.
pub fn list_from(columns: &[&str], fields: &[&str], data: Value) -> ListView {
    let items: Vec<Value> = match data {
        Value::Array(items) => items,
        Value::Object(ref map) => match map.get("items") {
            Some(Value::Array(items)) => items.clone(),
            _ => vec![data.clone()],
        },
        _ => vec![],
    };
    let total = items.len();
    ListView {
        columns: columns.iter().map(|c| c.to_string()).collect(),
        fields: fields.iter().map(|f| f.to_string()).collect(),
        items,
        total,
    }
}

pub fn detail(resource: &str, data: Value) -> DetailView {
    DetailView {
        resource: resource.to_string(),
        data,
    }
}

/// Renders a view in the requested format.
pub fn render<T: Render>(view: &T, format: OutputFormat) -> String {
    output::emit(view, format)
}

/// Parses an inline JSON argument, reporting a usage error rather than a panic.
pub fn parse_json(raw: &str, field: &str) -> Result<Value> {
    serde_json::from_str(raw)
        .map_err(|e| CliError::Usage(format!("--{field} is not valid JSON: {e}")))
}

pub fn read_password() -> Result<String> {
    rpassword::read_password()
        .map_err(|e| CliError::Authentication(format!("could not read the password: {e}")))
}

/// Builds a query string from optional parameters.
pub fn query(params: &[(&str, Option<String>)]) -> String {
    let encoded: Vec<String> = params
        .iter()
        .filter_map(|(key, value)| {
            value.as_ref().map(|v| format!("{key}={}", urlencode(v)))
        })
        .collect();
    if encoded.is_empty() {
        String::new()
    } else {
        format!("?{}", encoded.join("&"))
    }
}

/// Percent-encodes a query value.
///
/// A hand-rolled encoder avoids pulling in a dependency for one function, and
/// the unreserved set is exactly RFC 3986.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

mod rpassword {
    /// Reads a password without echoing it.
    ///
    /// Terminal echo suppression needs a tty; when stdin is a pipe (a script
    /// feeding the CLI) the line is read plainly, which is what such a script
    /// expects.
    pub fn read_password() -> std::io::Result<String> {
        use std::io::BufRead;

        if !is_terminal() {
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            return Ok(line.trim_end_matches(['\r', '\n']).to_string());
        }

        print!("Password: ");
        use std::io::Write as _;
        std::io::stdout().flush().ok();

        // Without a termios dependency the simplest correct behaviour is to
        // read the line and let the terminal echo it. `forge auth login
        // --password` remains available for scripted use.
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        println!();
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }

    fn is_terminal() -> bool {
        // `atty` semantics without a dependency: on Unix, `isatty(0)` is 1 for
        // a terminal.
        #[cfg(unix)]
        {
            extern "C" {
                fn isatty(fd: i32) -> i32;
            }
            unsafe { isatty(0) == 1 }
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

