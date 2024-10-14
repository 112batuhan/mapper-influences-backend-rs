use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use cached::proc_macro::cached;
use serde::{Deserialize, Serialize};

use crate::{database::leaderboard::Leaderboard, error::AppError, AppState};

#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    ranked: bool,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    start: u32,
}
fn default_limit() -> u32 {
    100
}

#[derive(Clone, Serialize)]
pub struct LeaderboardResponse {
    leaderboard: Vec<Leaderboard>,
}

#[cached(
    time = 60,
    key = "String",
    convert = r#"{format!("{:?}",query_parameters)}"#,
    result = true
)]
pub async fn get_leaderboard(
    Query(query_parameters): Query<LeaderboardQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<LeaderboardResponse>, AppError> {
    let leaderboard = state
        .db
        .leaderboard(
            query_parameters.country,
            query_parameters.ranked,
            query_parameters.limit,
            query_parameters.start,
        )
        .await?;
    Ok(Json(LeaderboardResponse { leaderboard }))
}
