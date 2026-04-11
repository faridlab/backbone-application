//! Database infrastructure: connection pool + migration runner.

pub mod connection;
pub mod migrations;

pub use connection::DatabaseManager;
