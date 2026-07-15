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
    ///   `postgres:postgres`) — warns.
    /// - `security.jwt_secret` against a known list of placeholder substrings
    ///   and a minimum length — **hard error** outside dev.
    ///
    /// Apps with extra surfaces (SMTP, signing keys, etc.) should add their
    /// own checks alongside this one.
    ///
    /// # Errors
    /// Returns `Err` when running outside a dev environment with a JWT secret
    /// that is a placeholder or too short to be a real key.
    pub fn validate_defaults(&self, env: &str) -> Result<(), String> {
        validate_defaults(env, &self.database.url, &self.security.jwt_secret)
    }
}

/// The shortest secret we accept outside dev. HS256 keys shorter than the hash
/// output (32 bytes) weaken the MAC, and short secrets are guessable.
const MIN_JWT_SECRET_LEN: usize = 32;

/// Stateless variant of [`AppConfig::validate_defaults`]. Easier to call
/// from places that don't hold the full config (e.g. tests, scripts).
///
/// The JWT secret is checked against the **resolved config value**, not
/// `std::env::var("JWT_SECRET")`: the secret can legitimately arrive from the
/// YAML default, in which case reading the env var sees an empty string and the
/// check silently passes on a secret nobody set.
///
/// A bad secret is fatal rather than a warning because the tenant guard
/// (`backbone_auth::tenant`) derives `company_id` from the token signature
/// alone. Anyone who can guess the secret can mint a token for any tenant, so a
/// placeholder secret in production is a cross-tenant breach, not a lint.
///
/// # Errors
/// Returns `Err` outside dev when the JWT secret is a placeholder or shorter
/// than [`MIN_JWT_SECRET_LEN`].
pub fn validate_defaults(env: &str, db_url: &str, jwt_secret: &str) -> Result<(), String> {
    if is_dev_env(env) {
        return Ok(());
    }

    if db_url.contains("root:password") || db_url.contains("postgres:postgres") {
        tracing::warn!(
            "Database URL contains default credentials in '{}' environment. \
             Use strong credentials in production.",
            env
        );
    }

    // Substrings, so a placeholder still trips the check when it has been
    // decorated (`change-me-in-production`, `my-changeme-secret`, ...).
    let placeholders = [
        "change-me",
        "change-this",
        "changeme",
        "your-super-secret",
        "dev-jwt-secret",
        "not-for-production",
        "secret",
    ];
    let lowered = jwt_secret.to_ascii_lowercase();
    if let Some(hit) = placeholders.iter().find(|p| lowered.contains(**p)) {
        return Err(format!(
            "JWT secret looks like a placeholder (contains '{hit}') in '{env}' environment. \
             The tenant guard trusts this signature to prove company_id — a guessable secret \
             lets anyone mint a token for any tenant. Set a strong JWT_SECRET."
        ));
    }
    if jwt_secret.len() < MIN_JWT_SECRET_LEN {
        return Err(format!(
            "JWT secret is {} bytes in '{env}' environment; minimum is {MIN_JWT_SECRET_LEN}. \
             Set a strong JWT_SECRET.",
            jwt_secret.len()
        ));
    }
    Ok(())
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

    const DEV_DB: &str = "postgres://postgres:postgres@localhost/db";
    /// 32+ bytes, no placeholder substring.
    const STRONG_SECRET: &str = "f3a91c7d2b8e45061a9fc3e78d2b415c9e07a6fd";

    #[test]
    fn validate_defaults_accepts_anything_in_dev() {
        // Dev is deliberately permissive — the skeleton must stay runnable on a
        // fresh clone with no env set.
        assert!(validate_defaults("dev", DEV_DB, "change-me-in-production").is_ok());
    }

    #[test]
    fn validate_defaults_accepts_a_strong_secret_in_prod() {
        assert!(validate_defaults("production", DEV_DB, STRONG_SECRET).is_ok());
    }

    #[test]
    fn the_shipped_default_secret_is_rejected_in_prod() {
        // Regression: `application.yml` ships `${JWT_SECRET:change-me-in-production}`, and the old
        // placeholder list ("changeme", "change-this", ...) matched none of it — so the skeleton's own
        // default sailed through. It must not.
        let err = validate_defaults("production", DEV_DB, "change-me-in-production")
            .expect_err("shipped default must be rejected outside dev");
        assert!(err.contains("placeholder"), "unexpected error: {err}");
    }

    #[test]
    fn short_secrets_are_rejected_in_prod() {
        let err = validate_defaults("production", DEV_DB, "f3a91c7d2b8e4506")
            .expect_err("a 16-byte secret must be rejected");
        assert!(err.contains("minimum"), "unexpected error: {err}");
    }

    #[test]
    fn validate_defaults_reads_config_not_the_env_var() {
        // Regression: the check used to read std::env::var("JWT_SECRET"). A secret supplied by the
        // YAML default left that empty, so the check passed on a secret nobody chose. A strong env
        // var must not excuse a weak configured value.
        std::env::set_var("JWT_SECRET", STRONG_SECRET);
        let result = validate_defaults("production", DEV_DB, "change-me-in-production");
        std::env::remove_var("JWT_SECRET");
        assert!(result.is_err(), "config value must be what is checked");
    }
}
