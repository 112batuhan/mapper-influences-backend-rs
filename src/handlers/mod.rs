use std::sync::Arc;

use itertools::Itertools;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    error::AppError,
    osu_api::{BeatmapEnum, CombinedRequester, GetID},
};

pub mod activity;
pub mod auth;
pub mod influence;
pub mod leaderboard;
pub mod osu_api;
pub mod user;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PaginationQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    start: u32,
}
fn default_limit() -> u32 {
    100
}

/// A shortcut to use in user and influence endpoints.
/// This is not usable for multiple influences as this function would send requests for each
/// influence. They have their own implementation to save requests
async fn swap_beatmaps(
    cached_combined_requester: Arc<CombinedRequester>,
    osu_token: &str,
    beatmaps: &mut Vec<BeatmapEnum>,
) -> Result<(), AppError> {
    let beatmaps_to_request: Vec<u32> = beatmaps.iter().map(|map| map.get_id()).unique().collect();

    let mut requested_beatmaps = cached_combined_requester
        .clone()
        .get_beatmaps_with_user(&beatmaps_to_request, osu_token)
        .await?;

    // to keep the order, we iterate user beatmaps
    let new_beatmaps: Vec<BeatmapEnum> = beatmaps
        .iter()
        .filter_map(|beatmap_enum| {
            // remove should be ok, we keep beatmaps as set in db, so they should be unique
            let beatmap = requested_beatmaps.remove(&beatmap_enum.get_id())?;
            Some(BeatmapEnum::All(beatmap))
        })
        .collect();

    *beatmaps = new_beatmaps;
    Ok(())
}
