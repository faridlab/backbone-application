//! Infrastructure layer for the minimal backbone skeleton.
//!
//! Ships with the database connection pool and migration runner. Other
//! infrastructure concerns (messaging orchestrators, external integrations,
//! cross-module health rollups) are intentionally not in the skeleton.

pub mod database;
