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
use backbone_observability::audit::{audit_middleware, AuditConfig};
use axum::middleware::from_fn_with_state;
use backbone_auth::company::{company_auth, CompanyVerifier};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

mod asset_gl_sink;
mod configuration;
mod infrastructure;
mod middleware;
mod shared;

use asset_gl_sink::AssetAccountingGlSink;
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

    // Reject dev-default values that leak into non-dev environments. A placeholder JWT secret is
    // fatal, not a warning: the tenant guard proves `company_id` from the token signature alone, so a
    // guessable secret is a cross-tenant breach. Fail at boot, where it is loud and cheap.
    let env = std::env::var("APP_ENV").unwrap_or_else(|_| "dev".to_string());
    app_config.validate_defaults(&env).map_err(|e| {
        error!("insecure configuration: {e}");
        anyhow::anyhow!("insecure configuration: {e}")
    })?;

    // Prometheus metrics. backbone-observability spawns a DEDICATED HTTP server
    // (separate from the app router) — `/metrics` is NOT mounted on the app port,
    // it lives at http://<host>:{METRICS_PORT}/metrics. The prod stack already
    // sets METRICS_PORT=9090 and Prometheus scrapes http://<app>:9090/metrics;
    // without this call that scrape gets connection-refused and dashboards/alerts
    // silently have no data. The handle must stay alive for the program's lifetime
    // or the spawned server exits.
    let metrics_enabled = std::env::var("METRICS_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);
    let metrics_port: u16 = std::env::var("METRICS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9090);
    let _metrics_handle = backbone_observability::init_metrics(
        &backbone_observability::MetricsConfig {
            enabled: metrics_enabled,
            exporter: backbone_observability::MetricsExporterType::Prometheus,
            port: metrics_port,
        },
    )?;
    info!(
        "✅ Metrics: enabled={metrics_enabled} http://0.0.0.0:{metrics_port}/metrics"
    );

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

    // Outbox relay (ADR-0011): fence each producer schema's outbox_events and drain it logged in as the
    // `metaphor_relay` role (a NOBYPASSRLS login the fence policy admits via `current_user =
    // 'metaphor_relay'`). The fence and the relay ship TOGETHER so a fenced outbox never silently
    // stalls delivery. Opt-in: inactive unless database.relay_url + outbox_schemas are configured;
    // create the role with scripts/rls_app_role.sql and point RELAY_DATABASE_URL at it.
    if app_config.database.outbox_schemas.is_empty() {
        tracing::info!(
            "outbox relay inactive — set database.relay_url + outbox_schemas (and run scripts/rls_app_role.sql) to enable"
        );
    } else if let Some(relay_url) = app_config
        .database
        .relay_url
        .as_ref()
        .map(String::as_str)
        .filter(|u| !u.is_empty())
    {
        match sqlx::postgres::PgPoolOptions::new().max_connections(4).connect(relay_url).await {
            Ok(relay_pool) => {
                // The relay drains onto the in-proc integration bus. The skeleton registers a logging
                // handler by default so drained events are visible; real apps register domain handlers
                // (and/or swap the bus for a broker). The outbox record's `id` IS the envelope id —
                // preserve it (NOT a fresh uuid) so consumer-side inbox dedup aligns end-to-end.
                let bus = backbone_messaging::IntegrationEventBus::new();
                bus.register_handler(Arc::new(backbone_messaging::IntegrationLoggingHandler::all()))
                    .await;
                for schema in &app_config.database.outbox_schemas {
                    if let Err(e) = backbone_outbox::outbox::migrate(database.pool(), schema).await {
                        tracing::error!("outbox migrate failed for '{schema}': {e} — relay for '{schema}' skipped");
                        continue;
                    }
                    let runner_schema = schema.clone();
                    let runner_bus = bus.clone();
                    let pool = relay_pool.clone();
                    let cfg = backbone_outbox::runner::RelayConfig::new(schema.clone());
                    tokio::spawn(backbone_outbox::runner::run(pool, cfg, move |rec| {
                        let bus = runner_bus.clone();
                        let schema = runner_schema.clone();
                        async move {
                            let envelope = backbone_messaging::IntegrationEventEnvelope {
                                id: rec.id.to_string(),
                                event_type: rec.event_type.clone(),
                                source_context: rec.aggregate_type.clone(),
                                aggregate_id: rec.aggregate_id.clone(),
                                occurred_at: rec.occurred_at,
                                published_at: chrono::Utc::now(),
                                version: rec.version as u32,
                                correlation_id: rec.correlation_id.clone(),
                                causation_id: rec.causation_id.clone(),
                                payload: rec.payload.clone(),
                            };
                            bus.publish_envelope(envelope)
                                .await
                                .map_err(|e| backbone_outbox::OutboxError::Publish(format!("{schema}: {e}")))
                        }
                    }, async {
                        let _ = tokio::signal::ctrl_c().await;
                    }));
                    info!("✅ Outbox relay started for schema '{schema}'");
                }
            }
            Err(e) => tracing::error!("outbox relay pool failed to connect (relay disabled): {e}"),
        }
    } else {
        tracing::warn!("outbox_schemas configured but database.relay_url is empty — no relay started");
    }

    // Billing→tax audit-mirror dispatcher (a sibling of the outbox relay above). Drains
    // billing.outbox_events → tax.record_tax_transaction / void_for_invoice.
    // NOTE: "billing" must NOT be in database.outbox_schemas — that schema is drained HERE, not by
    // the bus relay above, or the two double-drain the same rows.
    let tax_module = backbone_tax::TaxModule::builder()
        .with_database(database.pool().clone())
        .build()?;
    backbone_outbox::outbox::migrate(database.pool(), "billing").await?;
    let dispatcher_pool = database.pool().clone();
    let dispatcher_efaktur = tax_module.efaktur_service.clone();
    tokio::spawn(backbone_billing_tax::run_dispatcher(
        dispatcher_pool,
        "billing",
        dispatcher_efaktur,
        async { let _ = tokio::signal::ctrl_c().await; },
    ));
    info!("✅ billing→tax dispatcher started (draining billing.outbox_events)");

    // backbone-asset — the fixed-asset register. Its lifecycle (register/activate/depreciate/dispose) is
    // the only path that may change financial state, and it posts through a REAL GlPostSink into
    // backbone-accounting's ledger (the financial tables are read-only by default via all_crud_routes).
    let asset_gl: Arc<dyn backbone_asset::application::service::GlPostSink> =
        Arc::new(AssetAccountingGlSink::new(database.pool().clone()));
    let asset_module = backbone_asset::AssetsModule::builder()
        .with_database(database.pool().clone())
        .with_gl_sink(asset_gl)
        .build()?;

    // Tenant boundary (council ops-ux-security-readiness, move #2): every asset route — reads AND the
    // validated lifecycle verbs — runs behind `company_auth`. It proves the caller's company from the
    // signed JWT (`CompanyContext`) and binds it onto a request-dedicated connection. The lifecycle
    // handlers source `company_id` from that context (never the request body), so the cross-tenant
    // write primitive is closed. MUST be paired with a non-superuser DB role
    // (`scripts/rls_app_role.sql` → `metaphor_app`) so the RLS fence actually binds.
    let verifier = CompanyVerifier::hs256(app_config.security.jwt_secret.as_bytes());
    let asset_routes = asset_module
        .read_only_routes()
        .merge(asset_module.lifecycle_routes())
        .route_layer(from_fn_with_state(verifier, company_auth));
    info!("✅ backbone-asset mounted behind company_auth (lifecycle verbs post to accounting)");

    // Scheduled depreciation job (council ops-ux-security-readiness #5b): a background sweep that posts
    // every asset's due depreciation across ALL tenants. Idempotent (each period posts once), so the
    // hourly interval is safe to repeat. The job has no caller principal — it enumerates via the
    // `asset.due_depreciation_assets` SECURITY DEFINER function (bypasses RLS) and `run_due_depreciation`
    // re-scopes per asset for the writes. Matches the app's existing tokio::spawn background-task pattern.
    let dep_pool = database.pool().clone();
    let dep_gl = Arc::new(AssetAccountingGlSink::new(dep_pool.clone()));
    tokio::spawn(async move {
        let write = backbone_asset::application::service::AssetWriteService::new(dep_pool);
        let sink = backbone_asset::application::service::LoggingSink;
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match write.run_due_depreciation(chrono::Utc::now(), dep_gl.as_ref(), &sink).await {
                Ok(s) if s.periods_posted > 0 => info!(
                    target: "job.asset_depreciation",
                    assets = s.assets_depreciated, periods = s.periods_posted, fully = s.fully_depreciated,
                    "scheduled depreciation posted"
                ),
                Ok(_) => { /* nothing due this tick — quiet */ }
                Err(e) => error!(target: "job.asset_depreciation", error = %e, "scheduled depreciation failed"),
            }
        }
    });
    info!("✅ scheduled depreciation job started (hourly sweep, idempotent)");

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
        // backbone-asset: read-only financial tables + the validated GL-backed lifecycle verbs,
        // both behind company_auth (the tenant boundary — see `asset_routes` above).
        .merge(asset_routes)
        // The PgPool as an Extension so the `company_auth` middleware (ADR-0008) can establish a
        // request-dedicated connection once domain modules are mounted and the app role is flipped.
        // Without this layer, `company_auth` falls back to per-statement scoping and the hand-written
        // write-service paths fail closed under the non-super role.
        .layer(axum::Extension(database.pool().clone()))
        // Domain module routers merge HERE — see docs/composition-recipe.md for the pattern:
        //   .merge(payment_module.all_crud_routes())
        //   .merge(gateway_module.all_crud_routes())
        // Add modules on demand via `metaphor add module ... --to backbone`.
        // Audit logging (innermost — runs after maintenance/cors so the
        // event reflects the actual response status the client sees).
        // `audit_middleware` is stateful (takes `State<Arc<AuditConfig>>`), so it must be
        // wired with `from_fn_with_state`. `trust_proxy_headers: false` is the safe default
        // (only enable behind a trusted reverse proxy that sets X-Forwarded-For).
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(AuditConfig { trust_proxy_headers: false }),
            audit_middleware,
        ))
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
