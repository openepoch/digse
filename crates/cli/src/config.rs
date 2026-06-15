//! Configuration handling for digse
//!
//! Two layers live here:
//!   * [`SearchConfig`] — the *runtime* configuration handed to [`DigseSearch`]
//!     for a single query (engine selection, concurrency, timeout, stats).
//!   * [`DigseConfig`] — the *persisted* configuration stored at
//!     `~/.digse/config.toml`. It holds the
//!     default search knobs **and** the `digse serve` settings (host/port), so a
//!     user can configure digse once and have both `digse search` and
//!     `digse serve` honor it.

use std::path::{Path, PathBuf};

use digse_core::{EngineCategory, ResultType, TimeRange};
use serde::{Deserialize, Serialize};

/// Engine selection strategy
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EngineSelection {
    /// Use all available engines
    All,
    /// Use engines from specific categories
    Categories(Vec<EngineCategory>),
    /// Use specific engines by name
    Specific(Vec<String>),
    /// Exclude specific engines by name
    Exclude(Vec<String>),
}

/// Output format
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OutputFormat {
    /// Compact JSON
    Json,
    /// Pretty-printed JSON
    JsonPretty,
    /// Human-readable text format
    Text,
}

/// Main search configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Engine selection strategy
    pub engine_selection: EngineSelection,
    /// Number of engines to run concurrently
    pub concurrent_engines: usize,
    /// Timeout per engine in seconds
    pub timeout_seconds: u64,
    /// Whether to show engine statistics
    pub show_engine_stats: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig {
            engine_selection: EngineSelection::All,
            concurrent_engines: 3,
            timeout_seconds: 5,
            show_engine_stats: false,
        }
    }
}

impl SearchConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set engine selection strategy
    pub fn with_engine_selection(mut self, selection: EngineSelection) -> Self {
        self.engine_selection = selection;
        self
    }

    /// Set concurrent engine count
    pub fn with_concurrent_engines(mut self, count: usize) -> Self {
        self.concurrent_engines = count;
        self
    }

    /// Set timeout in seconds
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout_seconds = timeout;
        self
    }

    /// Enable/disable engine stats
    pub fn with_stats(mut self, show: bool) -> Self {
        self.show_engine_stats = show;
        self
    }
}

// ---------------------------------------------------------------------------
// Persisted configuration (~/.digse/config.toml)
// ---------------------------------------------------------------------------

/// Errors raised while loading, saving, or editing the persisted config.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config parse error in {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("config serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("home directory not found")]
    NoHome,
    #[error("unknown config key '{0}' (known: search.concurrent_engines, search.timeout_seconds, search.count, search.total_results, search.show_engine_stats, search.language, search.result_type, search.categories, search.time_range, search.safe_search, serve.host, serve.port)")]
    UnknownKey(String),
    #[error("invalid value for '{key}': {reason}")]
    InvalidValue { key: String, reason: String },
}

/// Default search knobs sourced from the persisted config.
///
/// These are *defaults* — explicit `digse search` flags always override them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchDefaults {
    /// Number of engines to query concurrently.
    pub concurrent_engines: usize,
    /// Per-engine request timeout in seconds.
    pub timeout_seconds: u64,
    /// Default results-per-engine count.
    pub count: usize,
    /// Default ceiling on total results returned.
    pub total_results: usize,
    /// Show engine statistics by default.
    pub show_engine_stats: bool,
    /// Default language preference (e.g. `"en-US"`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Default result type (e.g. `"web"`, `"images"`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_type: Option<String>,
    /// Default engine categories as a comma-separated string (e.g.
    /// `"general,it"`), if any. Mirrors the `/search` `categories` parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<String>,
    /// Default time range (e.g. `"day"`, `"week"`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<String>,
    /// Default safe-search flag.
    #[serde(default)]
    pub safe_search: bool,
}

impl Default for SearchDefaults {
    fn default() -> Self {
        SearchDefaults {
            concurrent_engines: 12,
            timeout_seconds: 5,
            count: 10,
            total_results: 20,
            show_engine_stats: false,
            language: None,
            result_type: None,
            categories: None,
            time_range: None,
            safe_search: false,
        }
    }
}

/// Settings for `digse serve`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServeConfig {
    /// Interface/hostname to bind the HTTP server to.
    pub host: String,
    /// TCP port to listen on.
    pub port: u16,
}

impl Default for ServeConfig {
    fn default() -> Self {
        ServeConfig {
            host: "127.0.0.1".to_string(),
            port: 8888,
        }
    }
}

/// The full persisted digse configuration.
///
/// Serialized as TOML with two tables, `[search]` and `[serve]`:
///
/// ```toml
/// [search]
/// concurrent_engines = 12
/// timeout_seconds = 5
/// count = 10
/// total_results = 20
/// show_engine_stats = false
///
/// [serve]
/// host = "127.0.0.1"
/// port = 8888
/// ```
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DigseConfig {
    /// Default search behavior.
    #[serde(default)]
    pub search: SearchDefaults,
    /// `digse serve` settings (host/port).
    #[serde(default)]
    pub serve: ServeConfig,
}

impl DigseConfig {
    /// Path to the config file.
    ///
    /// Linux: `~/.digse/config.toml`
    /// Windows: `<user home>\.digse\config.toml`
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        let home_dir = home::home_dir()
            .ok_or(ConfigError::NoHome)?;
        Ok(home_dir.join(".digse").join("config.toml"))
    }

    /// Load the config from disk, falling back to defaults when the file is
    /// absent or empty. A malformed file surfaces a [`ConfigError::Parse`].
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)?;
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.display().to_string(),
            source,
        })
    }

    /// Serialize and write the config to disk, creating parent dirs as needed.
    /// Returns the path written.
    pub fn save(&self) -> Result<PathBuf, ConfigError> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
        Ok(path)
    }

    /// Read a single dotted-key value (e.g. `"serve.port"`).
    ///
    /// Returns `None` for unrecognized keys.
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "search.concurrent_engines" => Some(self.search.concurrent_engines.to_string()),
            "search.timeout_seconds" => Some(self.search.timeout_seconds.to_string()),
            "search.count" => Some(self.search.count.to_string()),
            "search.total_results" => Some(self.search.total_results.to_string()),
            "search.show_engine_stats" => Some(self.search.show_engine_stats.to_string()),
            "search.language" => Some(self.search.language.clone().unwrap_or_default()),
            "search.result_type" => Some(self.search.result_type.clone().unwrap_or_default()),
            "search.categories" => Some(self.search.categories.clone().unwrap_or_default()),
            "search.time_range" => Some(self.search.time_range.clone().unwrap_or_default()),
            "search.safe_search" => Some(self.search.safe_search.to_string()),
            "serve.host" => Some(self.serve.host.clone()),
            "serve.port" => Some(self.serve.port.to_string()),
            _ => None,
        }
    }

    /// Set a single dotted-key value, validating the type. Persists nothing on
    /// its own — call [`DigseConfig::save`] afterwards.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        let invalid = |reason: &str| ConfigError::InvalidValue {
            key: key.to_string(),
            reason: reason.to_string(),
        };
        match key {
            "search.concurrent_engines" => {
                self.search.concurrent_engines =
                    value.parse().map_err(|_| invalid("expected a positive integer"))?
            }
            "search.timeout_seconds" => {
                self.search.timeout_seconds =
                    value.parse().map_err(|_| invalid("expected a positive integer"))?
            }
            "search.count" => {
                self.search.count =
                    value.parse().map_err(|_| invalid("expected a positive integer"))?
            }
            "search.total_results" => {
                self.search.total_results =
                    value.parse().map_err(|_| invalid("expected a positive integer"))?
            }
            "search.show_engine_stats" => {
                self.search.show_engine_stats =
                    parse_bool(value).ok_or_else(|| invalid("expected true or false"))?
            }
            "search.language" => self.search.language = Some(value.to_string()),
            "search.result_type" => {
                if value.trim().is_empty() {
                    self.search.result_type = None;
                } else {
                    ResultType::from_str(value.trim())
                        .ok_or_else(|| invalid("expected a result type (web, images, videos, …)"))?;
                    self.search.result_type = Some(value.trim().to_string());
                }
            }
            "search.categories" => {
                if value.trim().is_empty() {
                    self.search.categories = None;
                } else {
                    for tok in value.split(',') {
                        let tok = tok.trim();
                        if !tok.is_empty() {
                            EngineCategory::from_str(tok).ok_or_else(|| {
                                invalid(&format!("unknown engine category '{}'", tok))
                            })?;
                        }
                    }
                    self.search.categories = Some(value.trim().to_string());
                }
            }
            "search.time_range" => {
                if value.trim().is_empty() {
                    self.search.time_range = None;
                } else {
                    TimeRange::from_str(value.trim())
                        .ok_or_else(|| invalid("expected day, week, month, or year"))?;
                    self.search.time_range = Some(value.trim().to_string());
                }
            }
            "search.safe_search" => {
                self.search.safe_search =
                    parse_bool(value).ok_or_else(|| invalid("expected true or false"))?
            }
            "serve.host" => self.serve.host = value.to_string(),
            "serve.port" => {
                self.serve.port =
                    value.parse().map_err(|_| invalid("expected a port number 0-65535"))?
            }
            _ => return Err(ConfigError::UnknownKey(key.to_string())),
        }
        Ok(())
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" | "" => Some(false),
        _ => None,
    }
}

#[allow(dead_code)]
fn _ensure_path_used(_: &Path) {}

