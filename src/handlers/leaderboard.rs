use std::hash::Hash;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Query, State},
    Json,
};
use cached::Cached;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::osu_api::{BeatmapEnum, GetID};
use crate::{
    custom_cache::CustomCache,
    database::leaderboard::{LeaderboardBeatmap, LeaderboardUser},
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

pub struct LeaderboardCache<K: Hash + Eq + Clone, V: Clone> {
    /// In theory, it's better to use RwLock here, but [`CustomCache::cache_get`]
    /// takes &mut self reference, so we can't separate read and write operations
    cache: Mutex<CustomCache<K, Vec<V>>>,
}

impl<K: Hash + Eq + Clone, V: Clone> LeaderboardCache<K, V> {
    pub fn new(expire_in: u32) -> Self {
        Self {
            cache: Mutex::new(CustomCache::new(expire_in)),
        }
    }
    pub fn cached_query(
        &self,
        key: &K,
        start: u32,
        limit: u32,
    ) -> Result<Option<Vec<V>>, AppError> {
        let mut locked_cache = self.cache.lock().map_err(|_| AppError::Mutex)?;
        let Some(leaderboard) = locked_cache.cache_get(key) else {
            return Ok(None);
        };
        Ok(Some(
            leaderboard
                .iter()
                .skip(start as usize)
                .take(limit as usize)
                .cloned()
                .collect(),
        ))
    }

    pub fn add_leaderboard(&self, key: &K, leaderboard: Vec<V>) -> Result<(), AppError> {
        let mut locked_cache = self.cache.lock().map_err(|_| AppError::Mutex)?;
        locked_cache.cache_set(key.clone(), leaderboard);
        Ok(())
    }
}

#[derive(Clone, Serialize, JsonSchema)]
pub struct LeaderboardResponse<T> {
    leaderboard: Vec<T>,
}

pub async fn get_user_leaderboard(
    Query(query): Query<LeaderboardQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<LeaderboardResponse<LeaderboardUser>>, AppError> {
    let leaderboard_cache_limit = 500;

    if let Some(leaderboard) = state.user_leaderboard_cache.cached_query(
        &(query.ranked, query.country.clone()),
        query.start,
        query.limit,
    )? {
        return Ok(Json(LeaderboardResponse { leaderboard }));
    }
    let mut leaderboard = state
        .db
        .user_leaderboard(
            query.country.clone(),
            query.ranked,
            leaderboard_cache_limit,
            0,
        )
        .await?;
    leaderboard.shrink_to_fit();

    let limited_leaderboard = leaderboard
        .iter()
        .skip(query.start as usize)
        .take(query.limit as usize)
        .cloned()
        .collect();

    state
        .user_leaderboard_cache
        .add_leaderboard(&(query.ranked, query.country), leaderboard)?;
    Ok(Json(LeaderboardResponse {
        leaderboard: limited_leaderboard,
    }))
}

pub async fn get_beatmap_leaderboard(
    Query(query): Query<LeaderboardQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<LeaderboardResponse<LeaderboardBeatmap>>, AppError> {
    let leaderboard_cache_limit = 200;

    if let Some(leaderboard) =
        state
            .beatmap_leaderboard_cache
            .cached_query(&query.ranked, query.start, query.limit)?
    {
        return Ok(Json(LeaderboardResponse { leaderboard }));
    }

    let leaderboard = state
        .db
        .beatmap_leaderboard(query.ranked, leaderboard_cache_limit, 0)
        .await?;

    let beatmaps_to_request: Vec<u32> = leaderboard
        .iter()
        .map(|entry| entry.beatmap.get_id())
        .collect();

    let access_token = state.credentials_grant_client.get_access_token().await?;
    let mut beatmaps = state
        .cached_combined_requester
        .clone()
        .get_beatmaps_with_user(&beatmaps_to_request, &access_token)
        .await?;
    let mut leaderboard: Vec<LeaderboardBeatmap> = leaderboard
        .into_iter()
        .filter_map(|entry| {
            // we can use remove here since all of the maps should be unique
            let new_beatmap = beatmaps.remove(&entry.beatmap.get_id())?;
            Some(LeaderboardBeatmap {
                beatmap: BeatmapEnum::All(new_beatmap),
                count: entry.count,
            })
        })
        .collect();
    leaderboard.shrink_to_fit();

    let limited_leaderboard = leaderboard
        .iter()
        .skip(query.start as usize)
        .take(query.limit as usize)
        .cloned()
        .collect();

    state
        .beatmap_leaderboard_cache
        .add_leaderboard(&query.ranked, leaderboard)?;
    Ok(Json(LeaderboardResponse {
        leaderboard: limited_leaderboard,
    }))
}
