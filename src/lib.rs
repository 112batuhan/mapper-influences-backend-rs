use database::DatabaseClient;
use jwt::JwtUtil;
use osu_api::RequestClient;

pub mod custom_cache;
pub mod database;
pub mod error;
pub mod handlers;
pub mod jwt;
pub mod osu_api;

pub struct AppState {
    pub db: DatabaseClient,
    pub request: RequestClient,
    pub jwt: JwtUtil,
}

impl AppState {
    pub async fn new() -> AppState {
        AppState {
            db: DatabaseClient::new()
                .await
                .expect("failed to initialize db connection"),
            request: RequestClient::new(10),
            jwt: JwtUtil::new_jwt(),
        }
    }
}
