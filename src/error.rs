use aide::OperationIo;
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug, OperationIo)]
pub enum AppError {
    #[error("Missing influence")]
    MissingInfluence,

    #[error("Missing user {0}")]
    MissingUser(u32),

    #[error("Missing user_token cookie")]
    MissingTokenCookie,

    #[error("Jwt verification error")]
    JwtVerification,

    #[error("Wrong admin password")]
    WrongAdminPassword,

    #[error("Mutex error")]
    Mutex,

    #[error("RwLock error")]
    RwLock,

    //TODO: make this better?
    #[error("Value Missing")]
    MissingLayerJson,

    #[error("Activity stream closed")]
    ActivityStreamClosed,

    #[error("SurrealDB serialization error: {0}")]
    SurrealDbSerialization(String),

    #[error("Map with id {0} could not be found on osu! API")]
    NonExistingMap(u32),

    #[error("Tokio task error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("Error related to Sephomore: {0}")]
    SephomoreError(#[from] tokio::sync::AcquireError),

    #[error("Unhandled surrealdb error: {0}")]
    UnhandledDb(#[from] surrealdb::Error),

    #[error("Unhandled Reqwest Error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Failed to decode json text: {0}")]
    SerdeJson(#[from] serde_json::Error),

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
            | AppError::Mutex
            | AppError::RwLock
            | AppError::SerdeJson(_)
            | AppError::TaskJoin(_)
            | AppError::ActivityStreamClosed
            | AppError::SurrealDbSerialization(_)
            | AppError::SephomoreError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::MissingTokenCookie
            | AppError::JwtVerification
            | AppError::WrongAdminPassword => StatusCode::UNAUTHORIZED,
            AppError::MissingLayerJson => StatusCode::UNPROCESSABLE_ENTITY,

            AppError::MissingInfluence | AppError::MissingUser(_) | Self::NonExistingMap(_) => {
                StatusCode::NOT_FOUND
            }
        };
        (status_code, body).into_response()
    }
}
