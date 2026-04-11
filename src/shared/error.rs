//! Shared error handling utilities
#![allow(dead_code)]

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;
use tracing::{error, warn};

/// Application error type
#[derive(Error, Debug)]
pub enum AppError {
    /// Database errors
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Validation errors
    #[error("Validation error: {0}")]
    Validation(String),

    /// Not found errors
    #[error("Not found: {0}")]
    NotFound(String),

    /// Unauthorized errors
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Forbidden errors
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Conflict errors
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Rate limit errors
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Bad request errors
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Internal server errors
    #[error("Internal server error: {0}")]
    Internal(#[from] anyhow::Error),

    /// JWT errors
    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    /// Request parsing errors
    #[error("Request parsing error: {0}")]
    RequestParse(String),

    /// File system errors
    #[error("File system error: {0}")]
    FileSystem(#[from] std::io::Error),

    /// HTTP client errors
    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),

    /// Serialization errors (JSON)
    #[error("Serialization error: {0}")]
    SerializationJson(#[from] serde_json::Error),

    /// Serialization errors (string message)
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// UUID parsing errors
    #[error("UUID parsing error: {0}")]
    Uuid(#[from] uuid::Error),

    /// Custom errors with status code
    #[error("Custom error: {0}")]
    Custom(StatusCode, String),
}

impl AppError {
    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::RateLimit(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Jwt(_) => StatusCode::UNAUTHORIZED,
            AppError::RequestParse(_) => StatusCode::BAD_REQUEST,
            AppError::FileSystem(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::HttpClient(_) => StatusCode::BAD_GATEWAY,
            AppError::SerializationJson(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Uuid(_) => StatusCode::BAD_REQUEST,
            AppError::Custom(code, _) => *code,
        }
    }

    /// Get the error category for logging/metrics
    pub fn category(&self) -> &'static str {
        match self {
            AppError::Database(_) => "database",
            AppError::Config(_) => "configuration",
            AppError::Validation(_) => "validation",
            AppError::NotFound(_) => "not_found",
            AppError::Unauthorized(_) => "authentication",
            AppError::Forbidden(_) => "authorization",
            AppError::Conflict(_) => "conflict",
            AppError::RateLimit(_) => "rate_limit",
            AppError::BadRequest(_) => "bad_request",
            AppError::Internal(_) => "internal",
            AppError::Jwt(_) => "jwt",
            AppError::RequestParse(_) => "request_parse",
            AppError::FileSystem(_) => "file_system",
            AppError::HttpClient(_) => "http_client",
            AppError::SerializationJson(_) => "serialization",
            AppError::Serialization(_) => "serialization",
            AppError::Uuid(_) => "uuid",
            AppError::Custom(_, _) => "custom",
        }
    }

    /// Check if this error is client error (4xx)
    pub fn is_client_error(&self) -> bool {
        let code = self.status_code();
        code.as_u16() >= 400 && code.as_u16() < 500
    }

    /// Check if this error is server error (5xx)
    pub fn is_server_error(&self) -> bool {
        let code = self.status_code();
        code.as_u16() >= 500 && code.as_u16() < 600
    }

    /// Get user-friendly message (hides internal details)
    pub fn user_message(&self) -> String {
        match self {
            AppError::Database(_) => "Database operation failed. Please try again.".to_string(),
            AppError::Config(_) => "System configuration error. Please contact support.".to_string(),
            AppError::Validation(msg) => format!("Validation failed: {}", msg),
            AppError::NotFound(resource) => format!("{} not found", resource),
            AppError::Unauthorized(msg) => msg.clone(),
            AppError::Forbidden(msg) => msg.clone(),
            AppError::Conflict(msg) => format!("Conflict: {}", msg),
            AppError::RateLimit(msg) => format!("Rate limit exceeded: {}", msg),
            AppError::BadRequest(msg) => format!("Bad request: {}", msg),
            AppError::Internal(_) => "Internal server error. Please try again later.".to_string(),
            AppError::Jwt(_) => "Authentication token is invalid or expired".to_string(),
            AppError::RequestParse(msg) => format!("Invalid request: {}", msg),
            AppError::FileSystem(_) => "File operation failed. Please try again.".to_string(),
            AppError::HttpClient(_) => "External service error. Please try again later.".to_string(),
            AppError::SerializationJson(_) => "Data processing error. Please try again.".to_string(),
            AppError::Serialization(_) => "Data processing error. Please try again.".to_string(),
            AppError::Uuid(_) => "Invalid ID format provided".to_string(),
            AppError::Custom(_, msg) => msg.clone(),
        }
    }

    /// Create a custom error with status code
    pub fn custom(status: StatusCode, message: impl Into<String>) -> Self {
        AppError::Custom(status, message.into())
    }

    /// Create a bad request error
    pub fn bad_request(message: impl Into<String>) -> Self {
        AppError::Custom(StatusCode::BAD_REQUEST, message.into())
    }

    /// Create a not found error
    pub fn not_found(resource: impl Into<String>) -> Self {
        AppError::NotFound(resource.into())
    }

    /// Create an unauthorized error
    pub fn unauthorized(message: impl Into<String>) -> Self {
        AppError::Unauthorized(message.into())
    }

    /// Create a forbidden error
    pub fn forbidden(message: impl Into<String>) -> Self {
        AppError::Forbidden(message.into())
    }

    /// Create a conflict error
    pub fn conflict(message: impl Into<String>) -> Self {
        AppError::Conflict(message.into())
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        AppError::Internal(anyhow::anyhow!(message.into()))
    }

    /// Create a validation error
    pub fn validation(message: impl Into<String>) -> Self {
        AppError::Validation(message.into())
    }
}

/// Convert AppError to HTTP response
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let user_message = self.user_message();

        // Log error with appropriate level
        if self.is_server_error() {
            error!(
                "Server error [{}]: {} - {}",
                self.category(),
                self,
                user_message
            );
        } else {
            warn!(
                "Client error [{}]: {} - {}",
                self.category(),
                self,
                user_message
            );
        }

        let body = Json(json!({
            "success": false,
            "error": {
                "message": user_message,
                "code": status.as_u16(),
                "category": self.category()
            }
        }));

        (status, body).into_response()
    }
}

/// Result type alias for application operations
pub type AppResult<T> = Result<T, AppError>;

/// Error context for adding additional information to errors
pub trait ErrorContext<T> {
    /// Add context to the error
    fn with_context(self, context: &str) -> Result<T, AppError>;

    /// Add file/line context to the error
    fn with_location(self, file: &str, line: u32) -> Result<T, AppError>;

    /// Add both context and location
    fn with_full_context(self, context: &str, file: &str, line: u32) -> Result<T, AppError>;
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: Into<AppError>,
{
    fn with_context(self, context: &str) -> Result<T, AppError> {
        self.map_err(|e| {
            let app_err = e.into();
            match app_err {
                AppError::Custom(_, ref msg) => {
                    AppError::Custom(app_err.status_code(), format!("{}: {}", context, msg))
                }
                _ => AppError::custom(
                    app_err.status_code(),
                    format!("{}: {}", context, app_err),
                ),
            }
        })
    }

    fn with_location(self, file: &str, line: u32) -> Result<T, AppError> {
        self.map_err(|e| {
            let app_err = e.into();
            let location = format!("{}:{}", file, line);
            AppError::custom(
                app_err.status_code(),
                format!("{} [{}]", app_err, location),
            )
        })
    }

    fn with_full_context(self, context: &str, file: &str, line: u32) -> Result<T, AppError> {
        self.with_context(context)
            .with_location(file, line)
    }
}

/// Macro for adding error context
#[macro_export]
macro_rules! err_context {
    ($result:expr, $context:expr) => {
        $result.with_context($context, file!(), line!())
    };
}

/// Validation error with field information
#[derive(Error, Debug)]
#[error("{field}: {message}")]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub code: Option<String>,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            code: None,
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}

impl Into<AppError> for ValidationError {
    fn into(self) -> AppError {
        let message = if let Some(code) = &self.code {
            format!("{}: {} [{}]", self.field, self.message, code)
        } else {
            format!("{}: {}", self.field, self.message)
        };
        AppError::Validation(message)
    }
}

/// Multiple validation errors
#[derive(Error, Debug)]
#[error("Validation failed with {count} error(s)")]
pub struct ValidationErrors {
    pub errors: Vec<ValidationError>,
    pub count: usize,
}

impl ValidationErrors {
    pub fn new(errors: Vec<ValidationError>) -> Self {
        let count = errors.len();
        Self { errors, count }
    }

    pub fn single(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(vec![ValidationError::new(field, message)])
    }

    pub fn add_field(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.errors.push(ValidationError::new(field, message));
        self.count += 1;
    }
}

impl Into<AppError> for ValidationErrors {
    fn into(self) -> AppError {
        let messages: Vec<String> = self
            .errors
            .iter()
            .map(|e| {
                if let Some(code) = &e.code {
                    format!("{}: {} [{}]", e.field, e.message, code)
                } else {
                    format!("{}: {}", e.field, e.message)
                }
            })
            .collect();

        AppError::Validation(messages.join("; "))
    }
}

/// Error handling utilities
pub mod utils {
    use super::*;

    /// Convert any error to AppError
    pub fn to_app_error<E>(error: E, default_status: StatusCode) -> AppError
    where
        E: std::fmt::Display,
    {
        let message = error.to_string();
        AppError::Custom(default_status, message)
    }

    /// Convert database error to user-friendly message
    pub fn handle_database_error(error: sqlx::Error) -> AppError {
        match &error {
            sqlx::Error::RowNotFound => AppError::not_found("Record"),
            sqlx::Error::Database(db_err) => {
                // Handle specific database constraint violations
                if let Some(constraint) = db_err.constraint() {
                    match constraint {
                        "users_email_unique" => AppError::conflict("Email already exists"),
                        "users_username_unique" => AppError::conflict("Username already exists"),
                        _ => AppError::Database(error),
                    }
                } else {
                    AppError::Database(error)
                }
            }
            _ => AppError::Database(error),
        }
    }

    /// Handle JWT errors with user-friendly messages
    pub fn handle_jwt_error(error: jsonwebtoken::errors::Error) -> AppError {
        match error.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                AppError::unauthorized("Token has expired")
            }
            jsonwebtoken::errors::ErrorKind::InvalidSignature => {
                AppError::unauthorized("Invalid token signature")
            }
            jsonwebtoken::errors::ErrorKind::InvalidToken => {
                AppError::unauthorized("Invalid token")
            }
            _ => AppError::Jwt(error),
        }
    }

    /// Create error response from validation errors
    pub fn validation_error_response(errors: ValidationErrors) -> Response {
        let status = StatusCode::BAD_REQUEST;
        let error_messages: Vec<serde_json::Value> = errors
            .errors
            .iter()
            .map(|e| {
                json!({
                    "field": e.field,
                    "message": e.message,
                    "code": e.code
                })
            })
            .collect();

        let body = Json(json!({
            "success": false,
            "error": {
                "message": "Validation failed",
                "code": status.as_u16(),
                "validation_errors": error_messages
            }
        }));

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thiserror::Error;

    #[test]
    fn test_app_error_status_codes() {
        assert_eq!(
            AppError::NotFound("User".to_string()).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::Unauthorized("Access denied".to_string()).status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            AppError::Forbidden("No permission".to_string()).status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            AppError::Validation("Invalid input".to_string()).status_code(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn test_app_error_categories() {
        assert_eq!(AppError::Database(sqlx::Error::RowNotFound).category(), "database");
        assert_eq!(AppError::NotFound("test".to_string()).category(), "not_found");
        assert_eq!(AppError::Validation("test".to_string()).category(), "validation");
        assert_eq!(AppError::Unauthorized("test".to_string()).category(), "authentication");
    }

    #[test]
    fn test_app_error_user_messages() {
        let error = AppError::Database(sqlx::Error::RowNotFound);
        assert_eq!(
            error.user_message(),
            "Database operation failed. Please try again."
        );

        let error = AppError::Validation("Email is required".to_string());
        assert_eq!(
            error.user_message(),
            "Validation failed: Email is required"
        );

        let error = AppError::NotFound("User".to_string());
        assert_eq!(error.user_message(), "User not found");
    }

    #[test]
    fn test_app_error_constructors() {
        let error = AppError::bad_request("Invalid input");
        assert!(matches!(error, AppError::Custom(StatusCode::BAD_REQUEST, _)));

        let error = AppError::not_found("User");
        assert!(matches!(error, AppError::NotFound(_)));

        let error = AppError::unauthorized("Access denied");
        assert!(matches!(error, AppError::Unauthorized(_)));

        let error = AppError::forbidden("No permission");
        assert!(matches!(error, AppError::Forbidden(_)));

        let error = AppError::conflict("Duplicate entry");
        assert!(matches!(error, AppError::Conflict(_)));
    }

    #[test]
    fn test_validation_error() {
        let error = ValidationError::new("email", "Invalid email format")
            .with_code("INVALID_EMAIL");

        let app_error: AppError = error.into();
        assert!(matches!(app_error, AppError::Validation(_)));
        assert!(app_error.user_message().contains("email"));
        assert!(app_error.user_message().contains("Invalid email format"));
        assert!(app_error.user_message().contains("INVALID_EMAIL"));
    }

    #[test]
    fn test_validation_errors() {
        let mut errors = ValidationErrors::new(vec![
            ValidationError::new("email", "Invalid format"),
            ValidationError::new("password", "Too short"),
        ]);

        errors.add_field("name", "Required");

        assert_eq!(errors.count, 3);
        assert_eq!(errors.errors.len(), 3);
    }

    #[test]
    fn test_error_context() {
        let result: Result<(), AppError> = Err(AppError::not_found("User"));

        let contextual_result = result.with_context("While fetching user profile");
        assert!(contextual_result.is_err());

        let error = contextual_result.unwrap_err();
        assert!(error.user_message().contains("While fetching user profile"));
    }

    #[test]
    fn test_error_context_macro() {
        let result: Result<(), AppError> = Err(AppError::bad_request("Invalid input"));

        let contextual_result = err_context!(result, "During registration");
        assert!(contextual_result.is_err());

        let error = contextual_result.unwrap_err();
        assert!(error.user_message().contains("During registration"));
        assert!(error.user_message().contains("Invalid input"));
    }
}