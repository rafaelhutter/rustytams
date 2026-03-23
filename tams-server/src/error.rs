use salvo::http::StatusCode;
use salvo::prelude::*;
use serde::Serialize;

use tams_types::error::StoreError;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    NotFound,
    Unauthorized,
    Forbidden,
    BadRequest,
    InternalServerError,
}

#[derive(Debug, Serialize)]
pub struct AppError {
    #[serde(skip)]
    status: StatusCode,
    #[serde(rename = "type")]
    error_type: ErrorType,
    summary: String,
    time: String,
}

impl AppError {
    fn new(status: StatusCode, error_type: ErrorType, summary: impl Into<String>) -> Self {
        Self {
            status,
            error_type,
            summary: summary.into(),
            time: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn not_found(summary: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, ErrorType::NotFound, summary)
    }

    pub fn unauthorized(summary: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, ErrorType::Unauthorized, summary)
    }

    pub fn forbidden(summary: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, ErrorType::Forbidden, summary)
    }

    pub fn bad_request(summary: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorType::BadRequest, summary)
    }

    pub fn internal(summary: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorType::InternalServerError,
            summary,
        )
    }

    pub fn write_to(self, res: &mut Response) {
        res.status_code(self.status);
        res.render(Json(self));
    }
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound(msg) => AppError::not_found(msg),
            StoreError::ReadOnly => AppError::forbidden("Resource is read-only"),
            StoreError::BadRequest(msg) => AppError::bad_request(msg),
            StoreError::Internal(msg) => AppError::internal(msg),
        }
    }
}
