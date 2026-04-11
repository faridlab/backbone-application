//! Database Connection Management

use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use crate::configuration::DatabaseConfig;
use crate::shared::error::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Circuit Breaker
// ---------------------------------------------------------------------------

/// Circuit breaker states: Closed (normal), Open (rejecting), HalfOpen (probing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — all requests pass through.
    Closed = 0,
    /// Failures exceeded threshold — requests are rejected immediately.
    Open = 1,
    /// Cooling off — a limited number of probe requests are allowed through.
    HalfOpen = 2,
}

impl From<u8> for CircuitState {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::Open,
            2 => Self::HalfOpen,
            _ => Self::Closed,
        }
    }
}

/// Configuration for the circuit breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit.
    pub failure_threshold: u32,
    /// How long the circuit stays open before transitioning to half-open.
    pub reset_timeout: Duration,
    /// Number of successful probes in half-open state before closing again.
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            half_open_max_requests: 2,
        }
    }
}

/// Lock-free circuit breaker with three-state machine.
pub struct CircuitBreaker {
    state: AtomicU8,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    opened_at: Mutex<Option<Instant>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            opened_at: Mutex::new(None),
            config,
        }
    }

    /// Current circuit state.
    pub fn state(&self) -> CircuitState {
        let raw = self.state.load(Ordering::SeqCst);
        // Check Open → HalfOpen transition on read
        if raw == CircuitState::Open as u8 {
            if self.reset_timeout_elapsed() {
                self.transition_to(CircuitState::HalfOpen);
                return CircuitState::HalfOpen;
            }
        }
        CircuitState::from(raw)
    }

    /// Whether a request should be allowed through.
    pub fn is_request_allowed(&self) -> bool {
        match self.state() {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => {
                // Allow up to half_open_max_requests probes
                let current = self.success_count.load(Ordering::SeqCst)
                    + self.failure_count.load(Ordering::SeqCst);
                current < self.config.half_open_max_requests
            }
        }
    }

    /// Record a successful operation.
    pub fn record_success(&self) {
        match self.state() {
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::SeqCst);
            }
            CircuitState::HalfOpen => {
                let prev = self.success_count.fetch_add(1, Ordering::SeqCst);
                if prev + 1 >= self.config.half_open_max_requests {
                    self.transition_to(CircuitState::Closed);
                    tracing::info!("Circuit breaker closed — database recovered");
                }
            }
            CircuitState::Open => {} // shouldn't happen — requests are rejected
        }
    }

    /// Record a failed operation.
    pub fn record_failure(&self) {
        match self.state() {
            CircuitState::Closed => {
                let prev = self.failure_count.fetch_add(1, Ordering::SeqCst);
                if prev + 1 >= self.config.failure_threshold {
                    self.transition_to(CircuitState::Open);
                    tracing::warn!(
                        threshold = self.config.failure_threshold,
                        reset_timeout_secs = self.config.reset_timeout.as_secs(),
                        "Circuit breaker opened — database failures exceeded threshold"
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open immediately re-opens
                self.transition_to(CircuitState::Open);
                tracing::warn!("Circuit breaker re-opened — probe request failed");
            }
            CircuitState::Open => {}
        }
    }

    /// Consecutive failure count.
    pub fn failure_count(&self) -> u32 {
        self.failure_count.load(Ordering::SeqCst)
    }

    // -- private helpers --

    fn transition_to(&self, new_state: CircuitState) {
        self.state.store(new_state as u8, Ordering::SeqCst);
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        if new_state == CircuitState::Open {
            *self.opened_at.lock().unwrap() = Some(Instant::now());
        }
    }

    fn reset_timeout_elapsed(&self) -> bool {
        self.opened_at
            .lock()
            .unwrap()
            .map(|t| t.elapsed() >= self.config.reset_timeout)
            .unwrap_or(false)
    }
}

/// Database connection manager with circuit breaker protection.
pub struct DatabaseManager {
    pool: PgPool,
    min_connections: u32,
    circuit_breaker: CircuitBreaker,
}

impl DatabaseManager {
    /// Create a new database manager with the given configuration
    pub async fn new(config: &DatabaseConfig) -> AppResult<Self> {
        tracing::info!("🗄️ Initializing database connection pool");

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.connect_timeout))
            .idle_timeout(Duration::from_secs(config.idle_timeout))
            .connect(&config.url)
            .await
            .map_err(|e| {
                AppError::Database(e)
            })?;

        // Test the connection
        Self::test_connection(&pool).await?;

        tracing::info!("✅ Database connection pool initialized successfully");
        tracing::info!("🔢 Pool configuration: min={}, max={}", config.min_connections, config.max_connections);

        let circuit_breaker = CircuitBreaker::new(CircuitBreakerConfig::default());
        tracing::info!("🔌 Circuit breaker initialized (state: Closed)");

        Ok(Self { pool, min_connections: config.min_connections, circuit_breaker })
    }

    /// Get the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get database connection pool statistics
    pub fn pool_stats(&self) -> PoolStats {
        PoolStats {
            size: self.pool.size(),
            idle: self.pool.num_idle() as u32,
        }
    }

    /// Close the database connection pool
    pub async fn close(&self) {
        tracing::info!("🔌 Closing database connection pool");
        self.pool.close().await;
    }

    /// Prewarm the connection pool by establishing `min_connections` upfront.
    ///
    /// By default, SQLx establishes connections lazily. This method forces
    /// all minimum connections to be created before serving traffic,
    /// avoiding latency spikes on early requests.
    pub async fn prewarm_pool(&self) -> AppResult<()> {
        let min = self.min_connections;
        let start = std::time::Instant::now();

        let handles: Vec<_> = (0..min)
            .map(|_| {
                let pool = self.pool.clone();
                tokio::spawn(async move { pool.acquire().await })
            })
            .collect();

        for handle in handles {
            handle
                .await
                .map_err(|e| AppError::internal(format!("Pool warmup task failed: {e}")))?
                .map_err(|e| AppError::internal(format!("Pool warmup connection failed: {e}")))?;
        }

        tracing::info!(
            min_connections = min,
            elapsed_ms = start.elapsed().as_millis() as u64,
            "Database pool prewarmed"
        );
        Ok(())
    }

    /// Test database connection
    async fn test_connection(pool: &PgPool) -> AppResult<()> {
        tracing::debug!("🔍 Testing database connection");

        sqlx::query("SELECT 1")
            .execute(pool)
            .await
            .map_err(|e| AppError::Database(e))?;

        tracing::debug!("✅ Database connection test successful");
        Ok(())
    }

    /// Current circuit breaker state.
    pub fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.state()
    }

    /// Run database health check (integrated with circuit breaker).
    pub async fn health_check(&self) -> DatabaseHealth {
        let start = Instant::now();

        // If circuit is open, skip the actual probe — report unhealthy immediately
        if self.circuit_breaker.state() == CircuitState::Open {
            return DatabaseHealth {
                status: DatabaseStatus::Unhealthy,
                response_time_ms: 0,
                error: Some("Circuit breaker is open — database unreachable".to_string()),
                stats: self.pool_stats(),
            };
        }

        match Self::test_connection(&self.pool).await {
            Ok(_) => {
                self.circuit_breaker.record_success();
                let status = match self.circuit_breaker.state() {
                    CircuitState::HalfOpen => DatabaseStatus::Degraded,
                    _ => DatabaseStatus::Healthy,
                };
                DatabaseHealth {
                    status,
                    response_time_ms: start.elapsed().as_millis() as u64,
                    error: None,
                    stats: self.pool_stats(),
                }
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                let status = match self.circuit_breaker.state() {
                    CircuitState::Open => DatabaseStatus::Unhealthy,
                    CircuitState::HalfOpen => DatabaseStatus::Degraded,
                    CircuitState::Closed => DatabaseStatus::Degraded,
                };
                DatabaseHealth {
                    status,
                    response_time_ms: start.elapsed().as_millis() as u64,
                    error: Some(e.to_string()),
                    stats: self.pool_stats(),
                }
            }
        }
    }

    /// Get connection for a specific module (if using multiple databases)
    pub async fn get_module_connection(&self, _module: &str) -> AppResult<sqlx::PgPool> {
        // In a real implementation, this might return different pools
        // for different modules if they have separate databases
        Ok(self.pool.clone())
    }
}

/// Database connection pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub size: u32,
    pub idle: u32,
}

impl PoolStats {
    pub fn active(&self) -> u32 {
        self.size - self.idle
    }

    pub fn utilization_percent(&self) -> f64 {
        if self.size == 0 {
            return 0.0;
        }
        (self.active() as f64 / self.size as f64) * 100.0
    }
}

/// Database health status
#[derive(Debug, Clone)]
pub struct DatabaseHealth {
    pub status: DatabaseStatus,
    pub response_time_ms: u64,
    pub error: Option<String>,
    pub stats: PoolStats,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Database transaction helper
pub struct TransactionManager;

impl TransactionManager {
    /// Begin a new database transaction
    pub async fn begin(pool: &PgPool) -> AppResult<sqlx::Transaction<'static, sqlx::Postgres>> {
        pool.begin().await.map_err(AppError::Database)
    }

    /// Commit a transaction
    pub async fn commit(tx: sqlx::Transaction<'static, sqlx::Postgres>) -> AppResult<()> {
        tx.commit().await.map_err(AppError::Database)
    }

    /// Rollback a transaction
    pub async fn rollback(tx: sqlx::Transaction<'static, sqlx::Postgres>) -> AppResult<()> {
        tx.rollback().await.map_err(AppError::Database)
    }

    /// Execute multiple operations in a transaction with automatic rollback on error
    pub async fn execute_operations(
        pool: &PgPool,
        operations: Vec<Operation>,
    ) -> AppResult<Vec<OperationResult>> {
        let mut tx = pool.begin().await.map_err(AppError::Database)?;
        let mut results = Vec::new();

        for operation in operations {
            let result = match operation {
                Operation::Query { query, params } => {
                    Self::execute_query(&mut tx, &query, params).await
                }
                Operation::Execute { sql, params } => {
                    Self::execute_sql(&mut tx, &sql, params).await
                }
            };

            if matches!(result, OperationResult::Error { .. }) {
                // Rollback on error
                let _ = tx.rollback().await;
                results.push(result);
                return Ok(results);
            }

            results.push(result);
        }

        tx.commit().await.map_err(AppError::Database)?;
        Ok(results)
    }

    async fn execute_query(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        query: &str,
        _params: Vec<serde_json::Value>,
    ) -> OperationResult {
        match sqlx::query(query)
            .fetch_all(&mut **tx)
            .await
        {
            Ok(rows) => OperationResult::Success {
                rows_affected: rows.len(),
                data: None, // PgRow doesn't implement Serialize
            },
            Err(e) => OperationResult::Error {
                error: e.to_string(),
                sql: Some(query.to_string()),
            }
        }
    }

    async fn execute_sql(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        sql: &str,
        _params: Vec<serde_json::Value>,
    ) -> OperationResult {
        match sqlx::query(sql)
            .execute(&mut **tx)
            .await
        {
            Ok(result) => OperationResult::Success {
                rows_affected: result.rows_affected() as usize,
                data: None,
            },
            Err(e) => OperationResult::Error {
                error: e.to_string(),
                sql: Some(sql.to_string()),
            }
        }
    }
}

/// Database operation for transaction management
#[derive(Debug, Clone)]
pub enum Operation {
    Query {
        query: String,
        params: Vec<serde_json::Value>,
    },
    Execute {
        sql: String,
        params: Vec<serde_json::Value>,
    },
}

/// Result of a database operation
#[derive(Debug, Clone)]
pub enum OperationResult {
    Success {
        rows_affected: usize,
        data: Option<serde_json::Value>,
    },
    Error {
        error: String,
        sql: Option<String>,
    },
}

impl OperationResult {
    pub fn is_success(&self) -> bool {
        matches!(self, OperationResult::Success { .. })
    }

    pub fn rows_affected(&self) -> usize {
        match self {
            OperationResult::Success { rows_affected, .. } => *rows_affected,
            OperationResult::Error { .. } => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_pool_stats() {
        let stats = PoolStats {
            size: 10,
            idle: 3,
        };

        assert_eq!(stats.active(), 7);
        assert_eq!(stats.utilization_percent(), 70.0);
    }

    #[test]
    fn test_database_health() {
        let health = DatabaseHealth {
            status: DatabaseStatus::Healthy,
            response_time_ms: 45,
            error: None,
            stats: PoolStats { size: 10, idle: 5 },
        };

        assert_eq!(health.status, DatabaseStatus::Healthy);
        assert_eq!(health.response_time_ms, 45);
        assert!(health.error.is_none());
    }

    #[test]
    fn test_operation_result() {
        let success = OperationResult::Success {
            rows_affected: 5,
            data: None,
        };

        let error = OperationResult::Error {
            error: "Connection failed".to_string(),
            sql: Some("SELECT * FROM users".to_string()),
        };

        assert!(success.is_success());
        assert!(!error.is_success());
        assert_eq!(success.rows_affected(), 5);
        assert_eq!(error.rows_affected(), 0);
    }

    // -- Circuit Breaker tests --

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.is_request_allowed());
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        });

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure(); // 3rd failure → Open
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.is_request_allowed());
    }

    #[test]
    fn test_circuit_breaker_success_resets_failure_count() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        });

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.failure_count(), 2);

        cb.record_success(); // resets counter
        assert_eq!(cb.failure_count(), 0);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout: Duration::from_millis(10),
            half_open_max_requests: 2,
        });

        cb.record_failure(); // → Open
        assert_eq!(cb.state(), CircuitState::Open);

        std::thread::sleep(Duration::from_millis(15));

        // After timeout, state() should auto-transition to HalfOpen
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        assert!(cb.is_request_allowed());
    }

    #[test]
    fn test_circuit_breaker_half_open_closes_on_success() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout: Duration::from_millis(10),
            half_open_max_requests: 2,
        });

        cb.record_failure(); // → Open
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_success();
        cb.record_success(); // 2nd success → Closed
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_reopens_on_failure() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout: Duration::from_millis(10),
            half_open_max_requests: 2,
        });

        cb.record_failure(); // → Open
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_failure(); // → Open again
        assert_eq!(cb.state(), CircuitState::Open);
    }

    // Note: Integration tests would require a test database
    // These would test actual database connections and transactions
}