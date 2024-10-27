use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use cached::proc_macro::cached;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    custom_cache::CustomCache, database::leaderboard::Leaderboard, error::AppError, AppState,
};

#[derive(Debug, Deserialize, JsonSchema)]
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

#[derive(Clone, Serialize, JsonSchema)]
pub struct LeaderboardResponse {
    leaderboard: Vec<Leaderboard>,
}

#[cached(
    ty = "CustomCache<String, Json<LeaderboardResponse>>",
    create = "{CustomCache::new(600)}",
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
