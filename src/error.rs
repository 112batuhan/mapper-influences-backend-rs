use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Missing user {0}")]
    MissingUser(u32),

    #[error("Bio too long")]
    BioTooLong,

    #[error("Missing user_token cookie")]
    MissingTokenCookie,

    #[error("Jwt verification error")]
    JwtVerification,

    #[error("Error related to Sephomore: {0}")]
    SephomoreError(#[from] tokio::sync::AcquireError),

    #[error("Unhandled surrealdb error: {0}")]
    UnhandledDb(#[from] surrealdb::Error),

    #[error("Unhandled Reqwest Error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Unhandled Jwt error: {0}")]
    Jwt(#[from] jwt_simple::Error),
}

#[derive(Serialize)]
struct ErrorMessage {
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let body = Json(ErrorMessage {
            message: self.to_string(),
        });
        let status_code = match self {
            AppError::UnhandledDb(_)
            | AppError::Reqwest(_)
            | AppError::Jwt(_)
            | AppError::SephomoreError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::MissingTokenCookie | AppError::JwtVerification => StatusCode::UNAUTHORIZED,
            AppError::BioTooLong => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::MissingUser(_) => StatusCode::NOT_FOUND,
        };
        (status_code, body).into_response()
    }
}
