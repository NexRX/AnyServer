use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::types::ApiError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Too many requests: {0}")]
    TooManyRequests(String),

    #[error("Too many requests: {message}")]
    TooManyRequestsWithRetry {
        message: String,
        retry_after_secs: u64,
    },

    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::TooManyRequests(msg) => (StatusCode::TOO_MANY_REQUESTS, msg),
            AppError::TooManyRequestsWithRetry {
                message,
                retry_after_secs,
            } => {
                let body = Json(ApiError {
                    error: message,
                    details: None,
                });
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [(
                        axum::http::header::RETRY_AFTER,
                        retry_after_secs.to_string(),
                    )],
                    body,
                )
                    .into_response();
            }
            AppError::Internal(msg) => {
                // Log the full internal error server-side for debugging.
                tracing::error!("Internal error: {}", msg);

                // In dev mode (non-bundle-frontend), include the original
                // message in the `details` field to aid debugging.  In
                // production builds the client only sees a generic message.
                #[cfg(not(feature = "bundle-frontend"))]
                let body = Json(ApiError {
                    error: "An internal error occurred".to_string(),
                    details: Some(msg),
                });

                #[cfg(feature = "bundle-frontend")]
                let body = Json(ApiError {
                    error: "An internal error occurred".to_string(),
                    details: None,
                });

                return (StatusCode::INTERNAL_SERVER_ERROR, body).into_response();
            }
            AppError::Anyhow(err) => {
                tracing::error!("Unhandled internal error: {:#}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal error occurred".to_string(),
                )
            }
        };

        let body = Json(ApiError {
            error: message,
            details: None,
        });

        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        // Log the full database error server-side for debugging.
        tracing::error!("Database error: {:#}", err);

        // Distinguish transient errors from permanent ones for the
        // generic client-facing message, but never reveal schema details.
        match &err {
            sqlx::Error::PoolTimedOut => AppError::Internal(
                "The server is temporarily overloaded. Please try again.".to_string(),
            ),
            _ => AppError::Internal("A database error occurred".to_string()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        // Log the full I/O error server-side (may contain filesystem
        // paths, permission details, etc. that must not reach clients).
        tracing::error!("IO error: {:#}", err);
        AppError::Internal("An I/O error occurred".to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        if err.is_io() {
            tracing::error!("serde_json I/O error (likely database corruption): {}", err);
            AppError::Internal("Failed to process stored data".into())
        } else {
            let category = match err.classify() {
                serde_json::error::Category::Syntax => "syntax error",
                serde_json::error::Category::Data => "unexpected or missing field",
                serde_json::error::Category::Eof => "unexpected end of input",
                serde_json::error::Category::Io => "I/O error",
            };
            AppError::BadRequest(format!(
                "Invalid JSON at line {} column {}: {}",
                err.line(),
                err.column(),
                category,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlx_error_produces_generic_message() {
        // Simulate a database error with sensitive information.
        let sqlx_err = sqlx::Error::ColumnNotFound("users.password_hash".into());
        let app_err: AppError = sqlx_err.into();

        match &app_err {
            AppError::Internal(msg) => {
                assert!(
                    !msg.contains("users"),
                    "Generic message should not contain table/column names, got: {}",
                    msg
                );
                assert!(
                    !msg.contains("password_hash"),
                    "Generic message should not contain column names, got: {}",
                    msg
                );
                assert!(
                    msg.contains("database error"),
                    "Should mention database error generically, got: {}",
                    msg
                );
            }
            other => panic!("Expected AppError::Internal, got: {:?}", other),
        }
    }

    #[test]
    fn test_sqlx_pool_timeout_produces_overloaded_message() {
        let sqlx_err = sqlx::Error::PoolTimedOut;
        let app_err: AppError = sqlx_err.into();

        match &app_err {
            AppError::Internal(msg) => {
                assert!(
                    msg.contains("overloaded"),
                    "Pool timeout should mention overloaded, got: {}",
                    msg
                );
                assert!(
                    !msg.contains("pool"),
                    "Should not mention pool internals, got: {}",
                    msg
                );
            }
            other => panic!("Expected AppError::Internal, got: {:?}", other),
        }
    }

    #[test]
    fn test_io_error_produces_generic_message() {
        let io_err = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No such file or directory: /app/data/servers/secret-uuid/config.json",
        );
        let app_err: AppError = io_err.into();

        match &app_err {
            AppError::Internal(msg) => {
                assert!(
                    !msg.contains("/app/data"),
                    "Generic message should not contain filesystem paths, got: {}",
                    msg
                );
                assert!(
                    !msg.contains("secret-uuid"),
                    "Generic message should not contain UUIDs from paths, got: {}",
                    msg
                );
                assert!(
                    msg.contains("I/O error"),
                    "Should mention I/O error generically, got: {}",
                    msg
                );
            }
            other => panic!("Expected AppError::Internal, got: {:?}", other),
        }
    }

    #[test]
    fn test_sqlx_error_does_not_leak_sql_fragments() {
        // RowNotFound is a common error that could be confused with
        // informational leakage in some frameworks.
        let sqlx_err = sqlx::Error::RowNotFound;
        let app_err: AppError = sqlx_err.into();

        match &app_err {
            AppError::Internal(msg) => {
                assert!(
                    !msg.contains("SELECT"),
                    "Should not contain SQL keywords, got: {}",
                    msg
                );
                assert!(
                    !msg.contains("RowNotFound"),
                    "Should not contain SQLx variant names, got: {}",
                    msg
                );
            }
            other => panic!("Expected AppError::Internal, got: {:?}", other),
        }
    }

    #[test]
    fn test_io_error_with_permission_denied_is_generic() {
        let io_err = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Permission denied: /etc/shadow",
        );
        let app_err: AppError = io_err.into();

        match &app_err {
            AppError::Internal(msg) => {
                assert!(
                    !msg.contains("/etc/shadow"),
                    "Should not leak sensitive paths, got: {}",
                    msg
                );
                assert!(
                    !msg.contains("Permission denied"),
                    "Should not leak OS error details, got: {}",
                    msg
                );
            }
            other => panic!("Expected AppError::Internal, got: {:?}", other),
        }
    }

    #[test]
    fn test_bad_request_errors_are_not_sanitized() {
        // BadRequest, NotFound, etc. contain user-facing messages that
        // are hand-written by developers and safe to return.
        let err = AppError::BadRequest("Username is required".into());
        match err {
            AppError::BadRequest(msg) => {
                assert_eq!(msg, "Username is required");
            }
            _ => panic!("wrong variant"),
        }
    }
}
