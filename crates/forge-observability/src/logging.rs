//! Structured logging setup (spec 12, spec 17.1 `FORGE_LOG_*`).

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// How log lines are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable, for local development.
    Text,
    /// One JSON object per line, for a log pipeline.
    Json,
}

impl LogFormat {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "json" => Some(LogFormat::Json),
            "text" | "pretty" => Some(LogFormat::Text),
            _ => None,
        }
    }
}

/// Installs the global tracing subscriber.
///
/// `RUST_LOG` takes precedence over `log_level`, matching the usual tracing
/// convention, so an operator can raise verbosity for one process without
/// changing configuration.
pub fn init(level: &str, format: LogFormat) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    let result = match format {
        LogFormat::Json => tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .try_init(),
        LogFormat::Text => tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .try_init(),
    };

    // A subscriber is global and can only be installed once; a second call in
    // the same process is not an error worth failing startup over.
    if let Err(e) = result {
        eprintln!("tracing subscriber already installed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_format_parses_case_insensitively() {
        assert_eq!(LogFormat::parse("json"), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse("JSON"), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse(" text "), Some(LogFormat::Text));
        assert_eq!(LogFormat::parse("pretty"), Some(LogFormat::Text));
        assert_eq!(LogFormat::parse("xml"), None);
    }
}