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

/// How many entries are built and cached, independent of the requested page
const USER_LEADERBOARD_LIMIT: u32 = 500;
const BEATMAP_LEADERBOARD_LIMIT: u32 = 200;

pub fn user_leaderboard_key(ranked: bool, country: Option<&str>) -> String {
    format!("{}:{}", ranked, country.unwrap_or("global"))
}

pub fn beatmap_leaderboard_key(ranked: bool) -> String {
    ranked.to_string()
}

/// Builds the leaderboard from scratch, bypassing the cache. Shared by the handler
/// on a cache miss and by the background refresh in [`crate::cache_update`]
pub async fn build_user_leaderboard(
    state: &AppState,
    ranked: bool,
    country: Option<String>,
) -> Result<Vec<LeaderboardUser>, AppError> {
    let mut leaderboard = state
        .db
        .user_leaderboard(country, ranked, USER_LEADERBOARD_LIMIT, 0)
        .await?;
    leaderboard.shrink_to_fit();
    Ok(leaderboard)
}

/// See [`build_user_leaderboard`]. This one also fills in the beatmap and user
/// data from the osu! API, which is cached separately.
pub async fn build_beatmap_leaderboard(
    state: &AppState,
    ranked: bool,
) -> Result<Vec<LeaderboardBeatmap>, AppError> {
    let leaderboard = state
        .db
        .beatmap_leaderboard(ranked, BEATMAP_LEADERBOARD_LIMIT, 0)
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
    Ok(leaderboard)
}

/// Whole leaderboards are cached under a single key. Paginating them is up to the
/// handler, so a cached and a freshly built leaderboard go through the same code.
pub struct LeaderboardCache<V> {
    cache: Arc<RedisCache>,
    key_prefix: &'static str,
    expire_in: u64,
    value: PhantomData<fn() -> V>,
}

impl<V: Serialize + DeserializeOwned> LeaderboardCache<V> {
    pub fn new(cache: Arc<RedisCache>, key_prefix: &'static str, expire_in: u64) -> Self {
        Self {
            cache,
            key_prefix,
            expire_in,
            value: PhantomData,
        }
    }
    pub async fn get_leaderboard(&self, key: &str) -> Result<Option<Vec<V>>, AppError> {
        let full_key = format!("{}{}", self.key_prefix, key);
        self.cache.get::<Vec<V>>(&full_key).await
    }

    pub async fn add_leaderboard(&self, key: &str, leaderboard: &[V]) -> Result<(), AppError> {
        let full_key = format!("{}{}", self.key_prefix, key);
        self.cache
            .set(&full_key, &leaderboard, self.expire_in)
            .await?;
        Ok(())
    }
}

/// The single place a leaderboard gets cut down to the requested page, no matter
/// whether it came from the cache or was just built
fn paginate<V>(leaderboard: Vec<V>, start: u32, limit: u32) -> Vec<V> {
    leaderboard
        .into_iter()
        .skip(start as usize)
        .take(limit as usize)
        .collect()
}

pub async fn get_user_leaderboard(
    Query(query): Query<LeaderboardQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<LeaderboardUser>>, AppError> {
    let cache_key = user_leaderboard_key(query.ranked, query.country.as_deref());

    let leaderboard = match state
        .user_leaderboard_cache
        .get_leaderboard(&cache_key)
        .await?
    {
        Some(leaderboard) => leaderboard,
        None => {
            // Only the country agnostic leaderboards are refreshed in the background,
            // the rest is built here on demand
            let leaderboard = build_user_leaderboard(&state, query.ranked, query.country).await?;
            state
                .user_leaderboard_cache
                .add_leaderboard(&cache_key, &leaderboard)
                .await?;
            leaderboard
        }
    };

    Ok(Json(paginate(leaderboard, query.start, query.limit)))
}

pub async fn get_beatmap_leaderboard(
    Query(query): Query<LeaderboardQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<LeaderboardBeatmap>>, AppError> {
    let cache_key = beatmap_leaderboard_key(query.ranked);

    let leaderboard = match state
        .beatmap_leaderboard_cache
        .get_leaderboard(&cache_key)
        .await?
    {
        Some(leaderboard) => leaderboard,
        None => {
            let leaderboard = build_beatmap_leaderboard(&state, query.ranked).await?;
            state
                .beatmap_leaderboard_cache
                .add_leaderboard(&cache_key, &leaderboard)
                .await?;
            leaderboard
        }
    };

    Ok(Json(paginate(leaderboard, query.start, query.limit)))
}
