//! Backbone — minimal application skeleton.
//!
//! A runnable starting point for any Backbone-based service. Boots an HTTP
//! server with health probes, configurable maintenance gate, audit logging,
//! and a Postgres connection pool. Modules add their routes and handlers
//! by registering themselves into `app` after the database is ready.
//!
//! ## Subcommands
//!
//! - `serve` (default) — start the HTTP server.
//! - `migrate` — placeholder migration entrypoint (intended to be replaced
//!   with `metaphor migration run-all` orchestration in real services).
//! - `healthcheck` — probe `/health` and exit 0 on 2xx, non-zero otherwise.
//!   Used by the Dockerfile `HEALTHCHECK` directive in distroless images.
//!
//! ## Quick start
//!
//! ```bash
//! cargo run                # serve
//! cargo run -- healthcheck # probe /health
//! curl http://localhost:8080/health
//! curl http://localhost:8080/maintenance/status
//! ```

use std::sync::Arc;

use anyhow::Result;
use axum::{routing::get, Router};
use backbone_health::{routes::health_routes, HealthChecker, HealthConfig};
use backbone_maintenance::{
    admin_toggle_handler, maintenance_middleware, status_handler, MaintenanceConfig,
    MaintenanceState,
};
use backbone_observability::audit::audit_middleware;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

mod configuration;
mod infrastructure;
mod middleware;
mod shared;

use configuration::AppConfig;
use infrastructure::database::migrations::MigrationManager;
use infrastructure::database::DatabaseManager;
use shared::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // Subcommand dispatch must run BEFORE config / observability init so
    // `healthcheck` stays cheap (Docker re-invokes it on every interval).
    let mut args = std::env::args().skip(1);
    if let Some(cmd) = args.next() {
        match cmd.as_str() {
            "healthcheck" => return run_healthcheck().await,
            "migrate" => return run_migrate().await,
            "serve" => {} // explicit default — fall through
            other => anyhow::bail!(
                "unknown subcommand '{}' (supported: serve, migrate, healthcheck)",
                other
            ),
        }
    }

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🌀 Backbone skeleton starting");

    let app_config = AppConfig::load().map_err(|e| {
        error!("failed to load config: {e}");
        e
    })?;
    info!("✅ Config loaded");

    // Warn on dev-default values that leak into non-dev environments.
    let env = std::env::var("APP_ENV").unwrap_or_else(|_| "dev".to_string());
    app_config.validate_defaults(&env);

    let database = DatabaseManager::new(&app_config.database)
        .await
        .map_err(|e| {
            error!("failed to connect to database: {e}");
            anyhow::anyhow!("database connection failed: {e}")
        })?;
    info!("✅ Database connected");

    // Pre-warm the connection pool to its minimum size so the first
    // request doesn't pay the cold-connect tax.
    if let Err(e) = database.prewarm_pool().await {
        // Non-fatal: log and proceed; the pool will lazily fill on demand.
        tracing::warn!("Pool prewarm failed (continuing): {e}");
    }

    let migration_manager = MigrationManager::new(database.pool().clone());
    let migration_result = migration_manager
        .migrate()
        .await
        .map_err(|e| anyhow::anyhow!("migration failed: {e}"))?;
    info!(
        "✅ Migrations: {} total, {} pending",
        migration_result.total_migrations, migration_result.total_pending
    );

    let health_checker = Arc::new(HealthChecker::new(HealthConfig::default()));

    let _state = Arc::new(AppState::new(
        app_config.clone(),
        database.pool().clone(),
        // AppState clones the inner checker; cheap since the Arc handles refs
        HealthChecker::new(HealthConfig::default()),
    ));

    // Maintenance gate. `MaintenanceConfig::default()` is "off" so the
    // skeleton starts open. Real services should populate this from yaml.
    let maintenance_state = MaintenanceState::from_config(&MaintenanceConfig::default());

    // Routes that must remain reachable while the gate is on. Both paths
    // begin with `/maintenance` and live inside the default allow_paths.
    let maintenance_router = Router::new()
        .route("/maintenance/status", get(status_handler))
        .route(
            "/maintenance",
            axum::routing::post(admin_toggle_handler),
        )
        .with_state(maintenance_state.clone());

    let mut app = Router::new()
        .merge(health_routes(health_checker))
        .merge(maintenance_router)
        // Audit logging (innermost — runs after maintenance/cors so the
        // event reflects the actual response status the client sees).
        .layer(axum::middleware::from_fn(audit_middleware))
        // Maintenance gate (outermost — short-circuits before any other
        // layer pays its cost when the system is in maintenance).
        .layer(axum::middleware::from_fn_with_state(
            maintenance_state.clone(),
            maintenance_middleware,
        ))
        .layer(TraceLayer::new_for_http());
    if let Some(cors) = middleware::cors::default_cors_layer() {
        app = app.layer(cors);
    }

    let addr = app_config.server_addr();
    let listener = TcpListener::bind(addr).await?;
    info!("🚀 Listening on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn run_healthcheck() -> Result<()> {
    backbone_health::cli::run_healthcheck(8080)
        .await
        .map_err(|e| anyhow::anyhow!("healthcheck failed: {e}"))
}

async fn run_migrate() -> Result<()> {
    eprintln!(
        "WARN: backbone-app migrate is a placeholder. Real services should \
         delegate to `metaphor migration run-all` (which applies module \
         migrations against the target DB). Exiting 0."
    );
    Ok(())
}
