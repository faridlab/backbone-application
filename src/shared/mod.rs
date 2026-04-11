//! Shared application state and utilities for the minimal backbone skeleton.

use backbone_health::HealthChecker;
use crate::configuration::AppConfig;
use sqlx::PgPool;

pub mod error;
pub mod pagination;
pub mod response;

/// Shared application state.
///
/// The skeleton ships with the bare minimum: app config, a Postgres pool,
/// and a health checker. Modules added later can extend this struct (or
/// wrap it) with their own service handles via the `register(&mut app)`
/// pattern.
#[derive(Clone)]
pub struct AppState {
    #[allow(dead_code)]
    pub config: AppConfig,
    #[allow(dead_code)]
    pub db_pool: PgPool,
    pub health_checker: HealthChecker,
}

impl AppState {
    pub fn new(
        config: AppConfig,
        db_pool: PgPool,
        health_checker: HealthChecker,
    ) -> Self {
        Self {
            config,
            db_pool,
            health_checker,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pagination::*;

    #[test]
    fn test_pagination_params_defaults() {
        let params = PaginationParams::default();
        assert_eq!(params.page, Some(1));
        assert_eq!(params.limit, Some(20));
    }

    #[test]
    fn test_pagination_info() {
        let info = PaginationInfo::new(2, 20, 55);
        assert_eq!(info.page, 2);
        assert_eq!(info.limit, 20);
        assert_eq!(info.total, 55);
        assert_eq!(info.total_pages, 3);
        assert!(info.has_next);
        assert!(info.has_prev);
    }
}
