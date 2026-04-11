//! CORS middleware configuration

use axum::{
    http::{header, Method},
    response::Response,
};
use tower_http::{
    cors::{Any, CorsLayer},
    cors::AllowHeaders,
};
use tracing::info;

/// CORS configuration
#[derive(Debug, Clone)]
pub struct CorsConfig {
    /// Allowed origins (None means any)
    pub allowed_origins: Option<Vec<String>>,

    /// Allowed methods (None means any)
    pub allowed_methods: Option<Vec<Method>>,

    /// Allowed headers (None means any)
    pub allowed_headers: Option<Vec<String>>,

    /// Exposed headers
    pub exposed_headers: Vec<String>,

    /// Allow credentials
    pub allow_credentials: bool,

    /// Max age for preflight requests (seconds)
    pub max_age: Option<u64>,

    /// Enable CORS for all routes
    pub enabled: bool,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: Some(vec![
                "http://localhost:3000".to_string(),
                "http://localhost:3001".to_string(),
                "http://127.0.0.1:3000".to_string(),
                "http://127.0.0.1:3001".to_string(),
            ]),
            allowed_methods: Some(vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
                Method::PATCH,
            ]),
            allowed_headers: Some(vec![
                header::AUTHORIZATION.to_string(),
                header::ACCEPT.to_string(),
                header::CONTENT_TYPE.to_string(),
                "X-Requested-With".to_string(),
                "X-CSRF-Token".to_string(),
            ]),
            exposed_headers: vec![
                header::CONTENT_LENGTH.to_string(),
                "X-Total-Count".to_string(),
                "X-Page-Count".to_string(),
            ],
            allow_credentials: true,
            max_age: Some(3600), // 1 hour
            enabled: true,
        }
    }
}

impl CorsConfig {
    /// Create CORS config from environment variables
    pub fn from_env() -> Self {
        let enabled = std::env::var("CORS_ENABLED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        if !enabled {
            return Self {
                enabled: false,
                ..Default::default()
            };
        }

        let allowed_origins = std::env::var("CORS_ORIGINS")
            .ok()
            .map(|origins| origins.split(',').map(|s| s.trim().to_string()).collect());

        let allowed_methods_str = std::env::var("CORS_METHODS").ok();
        let allowed_methods = allowed_methods_str.map(|methods| {
            methods
                .split(',')
                .filter_map(|s| {
                    match s.trim().to_uppercase().as_str() {
                        "GET" => Some(Method::GET),
                        "POST" => Some(Method::POST),
                        "PUT" => Some(Method::PUT),
                        "DELETE" => Some(Method::DELETE),
                        "OPTIONS" => Some(Method::OPTIONS),
                        "PATCH" => Some(Method::PATCH),
                        "HEAD" => Some(Method::HEAD),
                        _ => None,
                    }
                })
                .collect()
        });

        let allowed_headers = std::env::var("CORS_HEADERS")
            .ok()
            .map(|headers| headers.split(',').map(|s| s.trim().to_string()).collect());

        let allow_credentials = std::env::var("CORS_CREDENTIALS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        let max_age = std::env::var("CORS_MAX_AGE")
            .ok()
            .and_then(|s| s.parse().ok());

        Self {
            allowed_origins,
            allowed_methods,
            allowed_headers,
            allow_credentials,
            max_age,
            enabled: true,
            ..Default::default()
        }
    }

    /// Create permissive CORS layer (allow all origins)
    pub fn permissive() -> Self {
        Self {
            allowed_origins: None, // Allow any origin
            allowed_methods: None, // Allow any method
            allowed_headers: None, // Allow any header
            allow_credentials: true,
            max_age: Some(3600),
            enabled: true,
            ..Default::default()
        }
    }

    /// Create restrictive CORS layer (specific domains only)
    pub fn restrictive(allowed_origins: Vec<String>) -> Self {
        Self {
            allowed_origins: Some(allowed_origins),
            allowed_methods: Some(vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
            ]),
            allowed_headers: Some(vec![
                header::AUTHORIZATION.to_string(),
                header::CONTENT_TYPE.to_string(),
            ]),
            allow_credentials: false,
            max_age: Some(3600),
            enabled: true,
            ..Default::default()
        }
    }

    /// Create development CORS layer (very permissive)
    pub fn development() -> Self {
        Self {
            allowed_origins: Some(vec![
                "http://localhost:*".to_string(),
                "http://127.0.0.1:*".to_string(),
                "http://0.0.0.0:*".to_string(),
            ]),
            allowed_methods: None, // Allow any method
            allowed_headers: None, // Allow any header
            allow_credentials: true,
            max_age: Some(86400), // 24 hours
            enabled: true,
            ..Default::default()
        }
    }

    /// Create production CORS layer
    pub fn production(allowed_origins: Vec<String>) -> Self {
        Self {
            allowed_origins: Some(allowed_origins),
            allowed_methods: Some(vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
                Method::PATCH,
            ]),
            allowed_headers: Some(vec![
                header::AUTHORIZATION.to_string(),
                header::ACCEPT.to_string(),
                header::CONTENT_TYPE.to_string(),
                "X-Requested-With".to_string(),
            ]),
            allow_credentials: true,
            max_age: Some(3600), // 1 hour
            enabled: true,
            ..Default::default()
        }
    }

    /// Build CORS layer from this configuration
    pub fn build_layer(self) -> Option<CorsLayer> {
        if !self.enabled {
            info!("🌐 CORS middleware disabled");
            return None;
        }

        info!("🌐 Configuring CORS middleware");

        let mut cors = CorsLayer::new();

        // When allow_credentials is true, CORS spec forbids using wildcard (Any)
        // for origins, headers, or methods. Use explicit values instead.
        let use_credentials = self.allow_credentials;

        // Configure origins
        if let Some(origins) = self.allowed_origins {
            if origins.iter().any(|o| o.contains('*')) {
                if use_credentials {
                    // Expand wildcard patterns to explicit localhost ports
                    let expanded: Vec<axum::http::HeaderValue> = [
                        "http://localhost:3000", "http://localhost:3001",
                        "http://localhost:5173", "http://localhost:8080",
                        "http://127.0.0.1:3000", "http://127.0.0.1:3001",
                        "http://127.0.0.1:5173", "http://127.0.0.1:8080",
                    ].iter().filter_map(|o| o.parse().ok()).collect();
                    cors = cors.allow_origin(expanded);
                } else {
                    cors = cors.allow_origin(Any);
                }
            } else {
                let parsed: Vec<axum::http::HeaderValue> = origins
                    .iter()
                    .filter_map(|o| o.parse().ok())
                    .collect();
                cors = cors.allow_origin(parsed);
            }
        } else if use_credentials {
            cors = cors.allow_origin([
                "http://localhost:3000".parse().unwrap(),
                "http://localhost:3001".parse().unwrap(),
            ]);
        } else {
            cors = cors.allow_origin(Any);
        }

        // Configure methods
        if let Some(methods) = self.allowed_methods {
            cors = cors.allow_methods(methods);
        } else if use_credentials {
            cors = cors.allow_methods([
                Method::GET, Method::POST, Method::PUT,
                Method::DELETE, Method::OPTIONS, Method::PATCH,
            ]);
        } else {
            cors = cors.allow_methods(Any);
        }

        // Configure headers
        if let Some(headers) = self.allowed_headers {
            let parsed: Vec<axum::http::HeaderName> = headers
                .iter()
                .filter_map(|h| h.parse().ok())
                .collect();
            cors = cors.allow_headers(parsed);
        } else if use_credentials {
            cors = cors.allow_headers([
                header::AUTHORIZATION, header::ACCEPT, header::CONTENT_TYPE,
                "X-Requested-With".parse().unwrap(), "X-CSRF-Token".parse().unwrap(),
            ]);
        } else {
            cors = cors.allow_headers(Any);
        }

        // Configure other settings
        cors = cors.allow_credentials(use_credentials);

        if let Some(max_age) = self.max_age {
            cors = cors.max_age(std::time::Duration::from_secs(max_age));
        }

        // Configure exposed headers
        if !self.exposed_headers.is_empty() {
            let parsed: Vec<axum::http::HeaderName> = self.exposed_headers
                .iter()
                .filter_map(|h| h.parse().ok())
                .collect();
            cors = cors.expose_headers(parsed);
        }

        Some(cors)
    }
}

/// Create default CORS layer for development
pub fn default_cors_layer() -> Option<CorsLayer> {
    CorsConfig::default().build_layer()
}

/// Create permissive CORS layer
pub fn permissive_cors_layer() -> Option<CorsLayer> {
    CorsConfig::permissive().build_layer()
}

/// Create restrictive CORS layer
pub fn restrictive_cors_layer(allowed_origins: Vec<String>) -> Option<CorsLayer> {
    CorsConfig::restrictive(allowed_origins).build_layer()
}

/// Create environment-based CORS layer
pub fn env_cors_layer() -> Option<CorsLayer> {
    CorsConfig::from_env().build_layer()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cors_config_default() {
        let config = CorsConfig::default();
        assert!(config.enabled);
        assert!(config.allow_credentials);
        assert_eq!(config.max_age, Some(3600));
        assert!(config.allowed_origins.is_some());
        assert!(config.allowed_origins.as_ref().unwrap().contains(&"http://localhost:3000".to_string()));
    }

    #[test]
    fn test_cors_config_permissive() {
        let config = CorsConfig::permissive();
        assert!(config.enabled);
        assert!(config.allowed_origins.is_none()); // Allow any
        assert!(config.allowed_methods.is_none());  // Allow any
        assert!(config.allowed_headers.is_none());  // Allow any
    }

    #[test]
    fn test_cors_config_development() {
        let config = CorsConfig::development();
        assert!(config.enabled);
        assert_eq!(config.max_age, Some(86400));
        assert!(config.allowed_origins.is_some());
        assert!(config.allowed_origins.as_ref().unwrap().iter().any(|o| o.contains('*')));
    }

    #[test]
    fn test_cors_config_from_env() {
        std::env::set_var("CORS_ENABLED", "false");
        let config = CorsConfig::from_env();
        assert!(!config.enabled);

        std::env::remove_var("CORS_ENABLED");

        std::env::set_var("CORS_ORIGINS", "http://example.com,http://test.com");
        std::env::set_var("CORS_METHODS", "GET,POST,PUT");
        std::env::set_var("CORS_CREDENTIALS", "false");

        let config = CorsConfig::from_env();
        assert!(config.enabled);
        assert!(config.allowed_origins.is_some());
        assert!(config.allowed_origins.as_ref().unwrap().contains(&"http://example.com".to_string()));
        assert!(config.allowed_methods.is_some());
        assert!(!config.allow_credentials);

        std::env::remove_var("CORS_ORIGINS");
        std::env::remove_var("CORS_METHODS");
        std::env::remove_var("CORS_CREDENTIALS");
    }
}