//! Shared response utilities
#![allow(dead_code)]

use axum::{
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use tracing::debug;

/// Standard API response structure
#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ErrorInfo>,
    pub meta: Option<MetaInfo>,
    pub pagination: Option<PaginationInfo>,
}

/// Error information in API response
#[derive(Debug, Clone, Serialize)]
pub struct ErrorInfo {
    pub code: u16,
    pub message: String,
    pub details: Option<Value>,
    pub field: Option<String>, // For validation errors
}

/// Metadata information
#[derive(Debug, Clone, Serialize)]
pub struct MetaInfo {
    pub timestamp: String,
    pub request_id: Option<String>,
    pub version: String,
    pub endpoint: Option<String>,
}

/// Pagination information
#[derive(Debug, Clone, Serialize)]
pub struct PaginationInfo {
    pub page: u32,
    pub limit: u32,
    pub total: u64,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
    pub offset: u32,
}

/// Response builder for creating consistent API responses
#[derive(Debug, Clone)]
pub struct ResponseBuilder {
    headers: HeaderMap,
    request_id: Option<String>,
    endpoint: Option<String>,
}

impl ResponseBuilder {
    /// Create a new response builder
    pub fn new() -> Self {
        Self {
            headers: HeaderMap::new(),
            request_id: None,
            endpoint: None,
        }
    }

    /// Add request ID to response metadata
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// Add endpoint information to response metadata
    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Add custom header
    pub fn with_header(mut self, name: &str, value: &str) -> Result<Self, axum::Error> {
        let header_name: HeaderName = name.parse().map_err(|e: axum::http::header::InvalidHeaderName| {
            axum::Error::new(format!("Invalid header name '{}': {}", name, e))
        })?;
        let header_value: HeaderValue = value.parse().map_err(|e: axum::http::header::InvalidHeaderValue| {
            axum::Error::new(format!("Invalid header value '{}': {}", value, e))
        })?;
        self.headers.insert(header_name, header_value);
        Ok(self)
    }

    /// Create success response
    pub fn success<T: Serialize>(self, data: T) -> impl IntoResponse {
        debug!("Creating success response");

        let meta = MetaInfo {
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_id: self.request_id,
            version: "1.0.0".to_string(),
            endpoint: self.endpoint,
        };

        let response = ApiResponse {
            success: true,
            data: Some(data),
            error: None,
            meta: Some(meta),
            pagination: None,
        };

        let mut response = Json(response).into_response();

        // Add custom headers
        for (name, value) in self.headers.iter() {
            response.headers_mut().insert(name.clone(), value.clone());
        }

        response
    }

    /// Create paginated success response
    pub fn success_paginated<T: Serialize>(
        self,
        data: T,
        pagination: PaginationInfo,
    ) -> impl IntoResponse {
        debug!("Creating paginated success response");

        let meta = MetaInfo {
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_id: self.request_id,
            version: "1.0.0".to_string(),
            endpoint: self.endpoint,
        };

        let response = ApiResponse {
            success: true,
            data: Some(data),
            error: None,
            meta: Some(meta),
            pagination: Some(pagination),
        };

        let mut response = Json(response).into_response();

        // Add custom headers
        for (name, value) in self.headers.iter() {
            response.headers_mut().insert(name.clone(), value.clone());
        }

        response
    }

    /// Create error response
    pub fn error(
        self,
        status: StatusCode,
        message: &str,
        details: Option<Value>,
    ) -> impl IntoResponse {
        debug!("Creating error response: {} - {}", status, message);

        let meta = MetaInfo {
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_id: self.request_id,
            version: "1.0.0".to_string(),
            endpoint: self.endpoint,
        };

        let error_info = ErrorInfo {
            code: status.as_u16(),
            message: message.to_string(),
            details,
            field: None,
        };

        let response: ApiResponse<()> = ApiResponse {
            success: false,
            data: None,
            error: Some(error_info),
            meta: Some(meta),
            pagination: None,
        };

        let mut response = (status, Json(response)).into_response();

        // Add custom headers
        for (name, value) in self.headers.iter() {
            response.headers_mut().insert(name.clone(), value.clone());
        }

        response
    }

    /// Create validation error response
    pub fn validation_error(self, errors: Vec<ValidationError>) -> impl IntoResponse {
        debug!("Creating validation error response with {} errors", errors.len());

        let details: Value = json!({
            "validation_errors": errors
        });

        self.error(StatusCode::BAD_REQUEST, "Validation failed", Some(details))
    }

    /// Create not found response
    pub fn not_found(self, resource: &str) -> impl IntoResponse {
        self.error(
            StatusCode::NOT_FOUND,
            &format!("{} not found", resource),
            None,
        )
    }

    /// Create unauthorized response
    pub fn unauthorized(self, message: &str) -> impl IntoResponse {
        self.error(
            StatusCode::UNAUTHORIZED,
            message,
            None,
        )
    }

    /// Create forbidden response
    pub fn forbidden(self, message: &str) -> impl IntoResponse {
        self.error(
            StatusCode::FORBIDDEN,
            message,
            None,
        )
    }

    /// Create conflict response
    pub fn conflict(self, message: &str) -> impl IntoResponse {
        self.error(
            StatusCode::CONFLICT,
            message,
            None,
        )
    }

    /// Create created response (201)
    pub fn created<T: Serialize>(self, data: T) -> impl IntoResponse {
        debug!("Creating created response");

        let meta = MetaInfo {
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_id: self.request_id,
            version: "1.0.0".to_string(),
            endpoint: self.endpoint,
        };

        let response = ApiResponse {
            success: true,
            data: Some(data),
            error: None,
            meta: Some(meta),
            pagination: None,
        };

        let mut response = (StatusCode::CREATED, Json(response)).into_response();

        // Add custom headers
        for (name, value) in self.headers.iter() {
            response.headers_mut().insert(name.clone(), value.clone());
        }

        response
    }

    /// Create no content response (204)
    pub fn no_content(self) -> impl IntoResponse {
        debug!("Creating no content response");

        let mut response = StatusCode::NO_CONTENT.into_response();

        // Add custom headers
        for (name, value) in self.headers.iter() {
            response.headers_mut().insert(name.clone(), value.clone());
        }

        response
    }
}

impl Default for ResponseBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Validation error structure
#[derive(Debug, Clone, Serialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub code: Option<String>,
    pub value: Option<Value>,
}

impl ValidationError {
    pub fn new(field: String, message: String) -> Self {
        Self {
            field,
            message,
            code: None,
            value: None,
        }
    }

    pub fn with_code(mut self, code: String) -> Self {
        self.code = Some(code);
        self
    }

    pub fn with_value(mut self, value: Value) -> Self {
        self.value = Some(value);
        self
    }
}

/// Create a simple success response
pub fn success_response<T: Serialize>(data: T) -> impl IntoResponse {
    ResponseBuilder::new().success(data)
}

/// Create a simple error response
pub fn error_response(status: StatusCode, message: &str) -> impl IntoResponse {
    ResponseBuilder::new().error(status, message, None)
}

/// Create a validation error response
pub fn validation_error_response(errors: Vec<ValidationError>) -> impl IntoResponse {
    ResponseBuilder::new().validation_error(errors)
}

/// Create a not found response
pub fn not_found_response(resource: &str) -> impl IntoResponse {
    ResponseBuilder::new().not_found(resource)
}

/// Create an unauthorized response
pub fn unauthorized_response(message: &str) -> impl IntoResponse {
    ResponseBuilder::new().unauthorized(message)
}

/// Create a forbidden response
pub fn forbidden_response(message: &str) -> impl IntoResponse {
    ResponseBuilder::new().forbidden(message)
}

/// Create a conflict response
pub fn conflict_response(message: &str) -> impl IntoResponse {
    ResponseBuilder::new().conflict(message)
}

/// Create a created response
pub fn created_response<T: Serialize>(data: T) -> impl IntoResponse {
    ResponseBuilder::new().created(data)
}

/// Create a no content response
pub fn no_content_response() -> impl IntoResponse {
    ResponseBuilder::new().no_content()
}

/// Response utilities for common scenarios
pub mod utils {
    use super::*;

    /// Convert any serializable data to success response
    pub fn ok<T: Serialize>(data: T) -> impl IntoResponse {
        success_response(data)
    }

    /// Convert message to success response with empty data
    pub fn message(message: &str) -> impl IntoResponse {
        success_response(json!({ "message": message }))
    }

    /// Internal server error response
    pub fn internal_error(message: &str) -> impl IntoResponse {
        error_response(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    /// Bad request response
    pub fn bad_request(message: &str) -> impl IntoResponse {
        error_response(StatusCode::BAD_REQUEST, message)
    }

    /// Service unavailable response
    pub fn service_unavailable(message: &str) -> impl IntoResponse {
        error_response(StatusCode::SERVICE_UNAVAILABLE, message)
    }

    /// Too many requests response
    pub fn too_many_requests(message: &str) -> impl IntoResponse {
        error_response(StatusCode::TOO_MANY_REQUESTS, message)
    }
}

/// Extension trait for converting common types to responses
pub trait IntoApiResponse<T: Serialize> {
    fn into_api_response(self) -> impl IntoResponse;
    fn into_created_response(self) -> impl IntoResponse;
}

impl<T: Serialize> IntoApiResponse<T> for T {
    fn into_api_response(self) -> impl IntoResponse {
        success_response(self)
    }

    fn into_created_response(self) -> impl IntoResponse {
        created_response(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_response_builder() {
        let builder = ResponseBuilder::new().with_request_id("test-123".to_string());

        assert_eq!(builder.request_id, Some("test-123".to_string()));
    }

    #[test]
    fn test_validation_error() {
        let error = ValidationError::new("email".to_string(), "Invalid email".to_string())
            .with_code("INVALID_EMAIL".to_string())
            .with_value(json!("test@example"));

        assert_eq!(error.field, "email");
        assert_eq!(error.message, "Invalid email");
        assert_eq!(error.code, Some("INVALID_EMAIL".to_string()));
    }

    #[test]
    fn test_pagination_info() {
        let pagination = PaginationInfo {
            page: 2,
            limit: 20,
            total: 55,
            total_pages: 3,
            has_next: true,
            has_prev: true,
            offset: 20,
        };

        assert_eq!(pagination.page, 2);
        assert_eq!(pagination.limit, 20);
        assert_eq!(pagination.total, 55);
        assert!(pagination.has_next);
        assert!(pagination.has_prev);
    }

    #[test]
    fn test_meta_info() {
        let meta = MetaInfo {
            timestamp: "2023-01-01T00:00:00Z".to_string(),
            request_id: Some("req-123".to_string()),
            version: "1.0.0".to_string(),
            endpoint: Some("/api/v1/users".to_string()),
        };

        assert_eq!(meta.request_id, Some("req-123".to_string()));
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.endpoint, Some("/api/v1/users".to_string()));
    }

    #[test]
    fn test_error_info() {
        let error = ErrorInfo {
            code: 404,
            message: "User not found".to_string(),
            details: Some(json!({"user_id": 123})),
            field: None,
        };

        assert_eq!(error.code, 404);
        assert_eq!(error.message, "User not found");
        assert!(error.details.is_some());
    }
}