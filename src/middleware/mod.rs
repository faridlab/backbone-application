//! HTTP middleware for the minimal backbone skeleton.
//!
//! Ships with CORS only. Request logging is provided by
//! `tower_http::trace::TraceLayer` in `main.rs`. Auth, audit, and
//! security-headers middleware were intentionally omitted from the
//! skeleton — add them per application as needed.

pub mod cors;
