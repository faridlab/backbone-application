//! Backbone — minimal application skeleton.
//!
//! A runnable starting point for any Backbone-based service. Boots an HTTP
//! server with a `/health` endpoint and a Postgres connection pool, runs
//! framework base migrations, and idles. Add business logic by registering
//! modules into `App` after the database is ready.
//!
//! Run with: `cargo run`
//! Health check: `curl http://localhost:8080/health`

use anyhow::Result;
use axum::{response::Json, routing::get, Router};
use serde_json::json;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

mod configuration;
mod infrastructure;
mod middleware;
mod shared;

use configuration::AppConfig;
use infrastructure::database::DatabaseManager;
use infrastructure::database::migrations::MigrationManager;
use shared::AppState;
use backbone_health::{HealthChecker, HealthConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize logging (basic tracing-subscriber; observability stack
    //    can be wired in by individual applications).
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🌀 Backbone skeleton starting");

    // 2. Load configuration from config/application*.yml + env overrides.
    let app_config = AppConfig::load()
        .map_err(|e| {
            error!("failed to load config: {e}");
            e
        })?;
    info!("✅ Config loaded");

    // 3. Connect to Postgres.
    let database = DatabaseManager::new(&app_config.database)
        .await
        .map_err(|e| {
            error!("failed to connect to database: {e}");
            anyhow::anyhow!("database connection failed: {e}")
        })?;
    info!("✅ Database connected");

    // 4. Run framework base migrations (creates _migrations, system_users,
    //    user_sessions, audit_logs, module_configurations).
    let migration_manager = MigrationManager::new(database.pool().clone());
    let migration_result = migration_manager
        .migrate()
        .await
        .map_err(|e| anyhow::anyhow!("migration failed: {e}"))?;
    info!(
        "✅ Migrations: {} total, {} pending",
        migration_result.total_migrations, migration_result.total_pending
    );

    // 5. Initialize a basic health checker. Components can be registered
    //    here by individual applications.
    let health_checker = HealthChecker::new(HealthConfig::default());

    // 6. Build shared application state.
    let _state = Arc::new(AppState::new(
        app_config.clone(),
        database.pool().clone(),
        health_checker,
    ));

    // 7. Build the HTTP router. The skeleton ships with one route. Modules
    //    added later contribute their own routes via `register(&mut app)`.
    //    Request logging comes from tower_http::trace::TraceLayer.
    //    CORS is configured via env vars; see middleware/cors.rs for details.
    let mut app = Router::new()
        .route("/health", get(health_handler))
        .layer(TraceLayer::new_for_http());
    if let Some(cors) = middleware::cors::default_cors_layer() {
        app = app.layer(cors);
    }

    // 8. Bind and serve.
    let addr = app_config.server_addr();
    let listener = TcpListener::bind(addr).await?;
    info!("🚀 Listening on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

/// Minimal health endpoint. Returns 200 with a JSON body. Real applications
/// should replace this with a richer health check that hits the database,
/// downstream services, etc.
async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "backbone-skeleton",
    }))
}
