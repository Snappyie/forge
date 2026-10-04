//! Contexts: server URL plus credentials, persisted between runs.
//!
//! Tokens are stored in a file with owner-only permissions where the platform
//! supports it, and never printed.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{CliError, Result};

/// A named server + credential pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub name: String,
    pub api_url: String,
    pub tenant_id: Option<String>,
    /// The most recent access token.
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

impl Context {
    pub fn new(name: &str, api_url: &str) -> Self {
        Self {
            name: name.to_string(),
            api_url: api_url.trim_end_matches('/').to_string(),
            tenant_id: None,
            access_token: None,
            refresh_token: None,
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.access_token.is_some()
    }
}

/// The set of known contexts, plus which one is selected.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// The selected context.
    pub current: Option<String>,
    pub contexts: Vec<Context>,
}

impl Config {
    pub fn active(&self) -> Option<&Context> {
        let name = self.current.as_ref()?;
        self.contexts.iter().find(|c| &c.name == name)
    }

    pub fn active_mut(&mut self) -> Option<&mut Context> {
        let name = self.current.clone()?;
        self.contexts.iter_mut().find(|c| c.name == name)
    }

    pub fn get(&self, name: &str) -> Option<&Context> {
        self.contexts.iter().find(|c| c.name == name)
    }

    pub fn put(&mut self, context: Context) {
        // Read the name before the value is moved into the replacement, so the
        // lookup does not borrow a value the arm consumes.
        let name = context.name.clone();
        match self.contexts.iter_mut().find(|c| c.name == name) {
            Some(existing) => *existing = context,
            None => self.contexts.push(context),
        }
        if self.current.is_none() {
            self.current = Some(name);
        }
    }
}

/// Where the config lives.
pub fn config_path() -> Result<PathBuf> {
    if let Ok(explicit) = std::env::var("FORGE_CONFIG") {
        return Ok(PathBuf::from(explicit));
    }
    let dir = dirs::config_dir()
        .ok_or_else(|| CliError::Config("could not determine a config directory".into()))?
        .join("forge");
    Ok(dir.join("config.json"))
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|e| CliError::Config(format!("{} is not valid: {e}", path.display()))),
        // A missing file is the normal first-run state, not an error.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(CliError::Config(format!(
            "could not read {}: {e}",
            path.display()
        ))),
    }
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::Config(format!("could not create {}: {e}", parent.display())))?;
    }
    let rendered = serde_json::to_string_pretty(config)
        .map_err(|e| CliError::Config(format!("could not render config: {e}")))?;

    // Write to a temporary file and rename, so an interrupted write cannot
    // leave a truncated config that loses every stored token.
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, &rendered)
        .map_err(|e| CliError::Config(format!("could not write {}: {e}", temp.display())))?;

    restrict_permissions(&temp)?;
    std::fs::rename(&temp, &path)
        .map_err(|e| CliError::Config(format!("could not replace {}: {e}", path.display())))?;
    restrict_permissions(&path)?;
    Ok(())
}

/// Narrows permissions to owner-only where the platform allows it.
///
/// The file holds bearer tokens, so it must not be world-readable.
fn restrict_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| CliError::Config(format!("could not stat {}: {e}", path.display())))?
            .permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)
            .map_err(|e| CliError::Config(format!("could not secure {}: {e}", path.display())))?;
    }
    #[cfg(not(unix))]
    {
        // No POSIX permission bits; the file lives under the user's own
        // profile directory, which carries the platform's own protection.
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_context_normalises_its_url() {
        let context = Context::new("prod", "http://localhost:3000/api/v1/");
        assert_eq!(context.api_url, "http://localhost:3000/api/v1");
        assert!(!context.is_authenticated());
    }

    #[test]
    fn an_empty_config_has_no_active_context() {
        let mut config = Config::default();
        assert!(config.active().is_none());
        assert!(config.active_mut().is_none());
    }

    #[test]
    fn the_first_context_becomes_current() {
        let mut config = Config::default();
        config.put(Context::new("dev", "http://localhost:3000"));
        assert_eq!(config.current.as_deref(), Some("dev"));
        assert!(config.active().is_some());
    }

    #[test]
    fn putting_a_context_replaces_rather_than_duplicates() {
        let mut config = Config::default();
        config.put(Context::new("dev", "http://one"));
        config.put(Context::new("dev", "http://two"));

        assert_eq!(config.contexts.len(), 1);
        assert_eq!(config.get("dev").unwrap().api_url, "http://two");
    }

    #[test]
    fn adding_a_second_context_does_not_change_the_current_one() {
        let mut config = Config::default();
        config.put(Context::new("dev", "http://one"));
        config.put(Context::new("prod", "http://two"));
        assert_eq!(config.current.as_deref(), Some("dev"));
    }

    #[test]
    fn a_config_round_trips_through_json() {
        let mut config = Config::default();
        let mut context = Context::new("prod", "http://api.example.com");
        context.access_token = Some("token-abc".into());
        context.tenant_id = Some("tenant-1".into());
        config.put(context);

        let rendered = serde_json::to_string(&config).unwrap();
        let parsed: Config = serde_json::from_str(&rendered).unwrap();

        assert_eq!(parsed.current.as_deref(), Some("prod"));
        assert_eq!(
            parsed.active().unwrap().access_token.as_deref(),
            Some("token-abc")
        );
    }

    #[test]
    fn the_config_path_honours_an_explicit_override() {
        // Exercised through the environment variable the loader consults.
        let previous = std::env::var("FORGE_CONFIG").ok();
        // SAFETY: single-threaded test setup.
        unsafe { std::env::set_var("FORGE_CONFIG", "/tmp/forge-test-config.json") };
        let path = config_path().unwrap();
        assert_eq!(path, PathBuf::from("/tmp/forge-test-config.json"));

        match previous {
            Some(value) => unsafe { std::env::set_var("FORGE_CONFIG", value) },
            None => unsafe { std::env::remove_var("FORGE_CONFIG") },
        }
    }
}
