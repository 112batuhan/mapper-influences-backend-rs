use std::sync::{Arc, Mutex};

use axum::{
    extract::{Query, State},
    Json,
};

use cached::Cached;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    custom_cache::CustomCache,
    database::leaderboard::{self, LeaderboardUser},
    error::AppError,
    AppState,
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

const LEADERBOARD_CACHE_LIMIT: u32 = 500;

pub type LeaderboardKey = (bool, Option<String>);

pub struct LeaderboardCache {
    cache: Mutex<CustomCache<LeaderboardKey, Vec<LeaderboardUser>>>,
}

impl LeaderboardCache {
    pub fn new(expire_in: u32) -> Self {
        Self {
            cache: Mutex::new(CustomCache::new(expire_in)),
        }
    }
    pub fn cached_query(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<Option<Vec<LeaderboardUser>>, AppError> {
        let mut locked_cache = self.cache.lock().map_err(|_| AppError::Mutex)?;
        let Some(leaderboard) = locked_cache.cache_get(&(query.ranked, query.country.clone()))
        else {
            return Ok(None);
        };
        Ok(Some(
            leaderboard
                .iter()
                .skip(query.start as usize)
                .take(query.limit as usize)
                .cloned()
                .collect(),
        ))
    }

    pub fn add_leaderboard(
        &self,
        query: &LeaderboardQuery,
        leaderboard: Vec<LeaderboardUser>,
    ) -> Result<(), AppError> {
        let mut locked_cache = self.cache.lock().map_err(|_| AppError::Mutex)?;
        locked_cache.cache_set((query.ranked, query.country.clone()), leaderboard);
        Ok(())
    }
}

#[derive(Clone, Serialize, JsonSchema)]
pub struct LeaderboardResponse {
    leaderboard: Vec<LeaderboardUser>,
}

// TODO: maybe you can avoid cloning the country?
// Shouldn't be too bad since country code is only two characters
pub async fn get_leaderboard(
    Query(query_parameters): Query<LeaderboardQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<LeaderboardResponse>, AppError> {
    if let Some(leaderboard) = state.leaderboard_cache.cached_query(&query_parameters)? {
        return Ok(Json(LeaderboardResponse { leaderboard }));
    }
    let leaderboard = state
        .db
        .leaderboard(
            query_parameters.country.clone(),
            query_parameters.ranked,
            LEADERBOARD_CACHE_LIMIT,
            0,
        )
        .await?;

    let limited_leaderboard = leaderboard
        .iter()
        .skip(query_parameters.start as usize)
        .take(query_parameters.limit as usize)
        .cloned()
        .collect();

    state
        .leaderboard_cache
        .add_leaderboard(&query_parameters, leaderboard)?;
    Ok(Json(LeaderboardResponse {
        leaderboard: limited_leaderboard,
    }))
}
