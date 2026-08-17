use std::marker::PhantomData;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::osu_api::{BeatmapEnum, GetID};
use crate::{
    cache::RedisCache,
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

fn user_leaderboard_key(ranked: bool, country: Option<&str>) -> String {
    format!("{}:{}", ranked, country.unwrap_or("global"))
}

/// Whole leaderboards are cached under a single key, the pagination is applied
/// on the cached entry after we read it back.
pub struct LeaderboardCache<V> {
    cache: Arc<RedisCache>,
    key_prefix: &'static str,
    expire_in: u64,
    value: PhantomData<fn() -> V>,
}

impl<V: Serialize + DeserializeOwned + Clone> LeaderboardCache<V> {
    pub fn new(cache: Arc<RedisCache>, key_prefix: &'static str, expire_in: u64) -> Self {
        Self {
            cache,
            key_prefix,
            expire_in,
            value: PhantomData,
        }
    }
    pub async fn cached_query(
        &self,
        key: &str,
        start: u32,
        limit: u32,
    ) -> Result<Option<Vec<V>>, AppError> {
        let full_key = format!("{}{}", self.key_prefix, key);
        let Some(leaderboard) = self.cache.get::<Vec<V>>(&full_key).await? else {
            return Ok(None);
        };
        Ok(Some(
            leaderboard
                .into_iter()
                .skip(start as usize)
                .take(limit as usize)
                .collect(),
        ))
    }

    pub async fn add_leaderboard(&self, key: &str, leaderboard: &[V]) -> Result<(), AppError> {
        let full_key = format!("{}{}", self.key_prefix, key);
        self.cache
            .set(&full_key, &leaderboard, self.expire_in)
            .await?;
        Ok(())
    }
}

pub async fn get_user_leaderboard(
    Query(query): Query<LeaderboardQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<LeaderboardUser>>, AppError> {
    let leaderboard_cache_limit = 500;
    let cache_key = user_leaderboard_key(query.ranked, query.country.as_deref());

    if let Some(leaderboard) = state
        .user_leaderboard_cache
        .cached_query(&cache_key, query.start, query.limit)
        .await?
    {
        return Ok(Json(leaderboard));
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
        .add_leaderboard(&cache_key, &leaderboard)
        .await?;
    Ok(Json(limited_leaderboard))
}

pub async fn get_beatmap_leaderboard(
    Query(query): Query<LeaderboardQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<LeaderboardBeatmap>>, AppError> {
    let leaderboard_cache_limit = 200;
    let cache_key = query.ranked.to_string();

    if let Some(leaderboard) = state
        .beatmap_leaderboard_cache
        .cached_query(&cache_key, query.start, query.limit)
        .await?
    {
        return Ok(Json(leaderboard));
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
        .add_leaderboard(&cache_key, &leaderboard)
        .await?;
    Ok(Json(limited_leaderboard))
}
