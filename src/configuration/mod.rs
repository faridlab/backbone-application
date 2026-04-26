//! Application configuration loader for the minimal backbone skeleton.
//!
//! Loads YAML config files (`config/application.yml` plus an environment-
//! specific override like `config/application-dev.yml`) and substitutes
//! `${VAR:default}` style placeholders from the process environment.
//!
//! The struct intentionally has a small surface — only what the skeleton
//! itself needs. Modules add their own config types and load them
//! independently when registered into the app.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_workers")]
    pub workers: usize,
    #[serde(default = "default_keep_alive")]
    pub keep_alive: u64,
    #[serde(default = "default_timeout")]
    pub read_timeout: u64,
    #[serde(default = "default_timeout")]
    pub write_timeout: u64,
}

fn default_workers() -> usize { 4 }
fn default_keep_alive() -> u64 { 75 }
fn default_timeout() -> u64 { 30 }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_conn")]
    pub max_connections: u32,
    #[serde(default = "default_min_conn")]
    pub min_connections: u32,
    #[serde(default = "default_db_timeout")]
    pub connect_timeout: u64,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout: u64,
    #[serde(default = "default_max_lifetime")]
    pub max_lifetime: u64,
}

fn default_max_conn() -> u32 { 20 }
fn default_min_conn() -> u32 { 5 }
fn default_db_timeout() -> u64 { 30 }
fn default_idle_timeout() -> u64 { 600 }
fn default_max_lifetime() -> u64 { 1800 }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
}

fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> String { "json".to_string() }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub jwt_secret: String,
    #[serde(default = "default_jwt_algo")]
    pub jwt_algorithm: String,
    #[serde(default)]
    pub jwt_issuer: String,
    #[serde(default)]
    pub jwt_audience: String,
    #[serde(default = "default_jwt_exp")]
    pub jwt_expiration: u64,
    #[serde(default)]
    pub cors_origins: Vec<String>,
}

fn default_jwt_algo() -> String { "HS256".to_string() }
fn default_jwt_exp() -> u64 { 86400 }

impl AppConfig {
    /// Load `config/application.yml`, then layer
    /// `config/application-{APP_ENV}.yml` on top if it exists, then expand
    /// `${VAR:default}` placeholders from the process environment.
    pub fn load() -> Result<Self> {
        let env = std::env::var("APP_ENV").unwrap_or_else(|_| "dev".to_string());
        info!("loading config for APP_ENV={env}");

        let base_path = Path::new("config/application.yml");
        let env_path_str = format!("config/application-{env}.yml");
        let env_path = Path::new(&env_path_str);

        let base = std::fs::read_to_string(base_path)
            .with_context(|| format!("reading {}", base_path.display()))?;
        let base = expand_env(&base);
        debug!("loaded base config from {}", base_path.display());

        let merged = if env_path.exists() {
            let overlay = std::fs::read_to_string(env_path)
                .with_context(|| format!("reading {}", env_path.display()))?;
            let overlay = expand_env(&overlay);
            debug!("loaded overlay config from {}", env_path.display());
            // Simple overlay: parse both, deserialize the overlay over the base
            // by deserializing the base then deserializing the overlay into
            // the same struct (last one wins for top-level keys).
            // For a richer merge, swap to a config crate later.
            let mut value: serde_yaml::Value =
                serde_yaml::from_str(&base).context("parsing base yaml")?;
            let overlay_value: serde_yaml::Value =
                serde_yaml::from_str(&overlay).context("parsing overlay yaml")?;
            merge_yaml(&mut value, overlay_value);
            serde_yaml::from_value(value).context("deserializing merged config")?
        } else {
            serde_yaml::from_str(&base).context("deserializing base config")?
        };

        Ok(merged)
    }

    pub fn server_addr(&self) -> std::net::SocketAddr {
        format!("{}:{}", self.server.host, self.server.port)
            .parse()
            .expect("invalid server.host:port")
    }

    /// Emit `tracing::warn!` lines for any configuration value that looks
    /// like a development placeholder when running outside a dev environment.
    /// Should be called once at startup, after `load()` and after the tracing
    /// subscriber is installed.
    ///
    /// Currently checks:
    /// - `database.url` for default credentials (`root:password`,
    ///   `postgres:postgres`).
    /// - `JWT_SECRET` env var against a known list of placeholder substrings.
    ///
    /// Apps with extra surfaces (SMTP, signing keys, etc.) should add their
    /// own checks alongside this one.
    pub fn validate_defaults(&self, env: &str) {
        validate_defaults(env, &self.database.url);
    }
}

/// Stateless variant of [`AppConfig::validate_defaults`]. Easier to call
/// from places that don't hold the full config (e.g. tests, scripts).
pub fn validate_defaults(env: &str, db_url: &str) {
    let is_dev = is_dev_env(env);
    if !is_dev {
        if db_url.contains("root:password") || db_url.contains("postgres:postgres") {
            tracing::warn!(
                "Database URL contains default credentials in '{}' environment. \
                 Use strong credentials in production.",
                env
            );
        }
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_default();
        let placeholders = [
            "change-this",
            "your-super-secret",
            "dev-jwt-secret",
            "not-for-production",
            "changeme",
        ];
        if placeholders.iter().any(|p| jwt_secret.contains(p)) {
            tracing::warn!(
                "JWT_SECRET appears to be a placeholder. Set a strong secret for production."
            );
        }
    }
}

/// `"dev" | "development" | "local"` — case-insensitive.
fn is_dev_env(env: &str) -> bool {
    matches!(
        env.to_ascii_lowercase().as_str(),
        "dev" | "development" | "local"
    )
}

/// Expand `${VAR:default}` and `${VAR}` placeholders in a YAML source string.
fn expand_env(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut spec = String::new();
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                }
                spec.push(c);
            }
            let (name, default) = match spec.split_once(':') {
                Some((n, d)) => (n.to_string(), d.to_string()),
                None => (spec, String::new()),
            };
            out.push_str(&std::env::var(&name).unwrap_or(default));
        } else {
            out.push(c);
        }
    }
    out
}

/// Recursively merge two YAML values: keys from `overlay` override `base`.
fn merge_yaml(base: &mut serde_yaml::Value, overlay: serde_yaml::Value) {
    match (base, overlay) {
        (serde_yaml::Value::Mapping(b), serde_yaml::Value::Mapping(o)) => {
            for (k, v) in o {
                if let Some(existing) = b.get_mut(&k) {
                    merge_yaml(existing, v);
                } else {
                    b.insert(k, v);
                }
            }
        }
        (b, o) => *b = o,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_env_with_default() {
        std::env::remove_var("METAPHOR_TEST_X");
        assert_eq!(expand_env("${METAPHOR_TEST_X:hello}"), "hello");
    }

    #[test]
    fn expand_env_from_env() {
        std::env::set_var("METAPHOR_TEST_Y", "world");
        assert_eq!(expand_env("${METAPHOR_TEST_Y:fallback}"), "world");
        std::env::remove_var("METAPHOR_TEST_Y");
    }

    #[test]
    fn dev_env_classifier_is_case_insensitive() {
        assert!(is_dev_env("dev"));
        assert!(is_dev_env("DEV"));
        assert!(is_dev_env("Development"));
        assert!(is_dev_env("local"));
        assert!(!is_dev_env("production"));
        assert!(!is_dev_env("staging"));
    }

    #[test]
    fn validate_defaults_is_silent_in_dev() {
        // Just confirms no panic; the warns themselves are tracing-side
        // effects that aren't asserted on here.
        validate_defaults("dev", "postgres://postgres:postgres@localhost/db");
    }

    #[test]
    fn validate_defaults_runs_in_prod_without_panicking() {
        std::env::set_var("JWT_SECRET", "change-this-please");
        validate_defaults("production", "postgres://postgres:postgres@localhost/db");
        std::env::remove_var("JWT_SECRET");
    }
}
